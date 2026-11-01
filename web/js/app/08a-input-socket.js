import { app } from './00-core.js';
import { tr } from './01-i18n.js';
import { toast } from './03-session.js';
import { updateDiagnosticsCard } from './05-power.js';

// Keyboard/mouse transport. While the console is open, HID requests travel
// over one WebSocket (/api/input/ws) instead of one HTTP request per event,
// which removes per-event connection and header overhead. request() routes
// the HID endpoints here when the socket is open and falls back to HTTP
// otherwise. The one difference callers see: a request in flight when the
// socket drops may be rejected as possibly undelivered (see below).
const HID_SOCKET_ROUTES = {
  '/api/key': 'key',
  '/api/mouse/move': 'move',
  '/api/mouse/absolute': 'absolute',
  '/api/mouse/click': 'click',
  '/api/mouse/scroll': 'scroll',
  '/api/input/release-all': 'release_all',
};
const INPUT_SOCKET_TIMEOUT_MS = 5000;
let inputSocket = null,
  inputSocketWanted = false,
  inputSocketRetry = 0,
  inputSocketDelay = 500,
  inputSocketNextId = 1;
const inputSocketPending = new Map();

function inputSocketReady() {
  return inputSocket?.readyState === WebSocket.OPEN;
}
// A dropped socket cannot say which in-flight requests the server already
// executed: it runs each message and only then replies, so a lost reply may
// belong to a key press or click that did reach the target. Replaying those
// over HTTP could type a key twice (an Enter on a confirmation prompt), so
// only idempotent requests are retried: an absolute move just restates the
// position and release_all releases what is already released. Everything
// else is rejected as possibly undelivered, with one toast per drop however
// many requests were pending; `reported` tells callers not to toast again.
// (Requests the socket never accepted fall back to HTTP in
// sendOverInputSocket, see below.)
const UNDELIVERED_MESSAGE = '键鼠连接中断，刚才的操作可能未送达被控机，请确认后再重试';
function idempotentInput(type, payload) {
  return type === 'release_all' || (type === 'absolute' && payload.action === 'move');
}
function settlePendingAfterClose() {
  const pending = [...inputSocketPending.values()];
  inputSocketPending.clear();
  let rejected = 0;
  for (const { resolve, reject, timer, fallback, idempotent } of pending) {
    clearTimeout(timer);
    if (idempotent) {
      fallback().then(resolve, reject);
      continue;
    }
    const error = new Error(tr(UNDELIVERED_MESSAGE));
    error.status = 0;
    error.undelivered = true;
    error.reported = true;
    rejected += 1;
    reject(error);
  }
  if (rejected) toast(tr(UNDELIVERED_MESSAGE), true);
}
function connectInputSocket() {
  inputSocketWanted = true;
  clearTimeout(inputSocketRetry);
  if (inputSocket || typeof WebSocket !== 'function') return;
  const scheme = location.protocol === 'https:' ? 'wss' : 'ws',
    socket = new WebSocket(`${scheme}://${location.host}/api/input/ws`);
  inputSocket = socket;
  socket.onopen = () => {
    inputSocketDelay = 500;
    updateDiagnosticsCard();
  };
  socket.onmessage = (event) => {
    let reply;
    try {
      reply = JSON.parse(event.data);
    } catch {
      return;
    }
    const pending = inputSocketPending.get(reply.id);
    if (!pending) return;
    inputSocketPending.delete(reply.id);
    clearTimeout(pending.timer);
    if (reply.ok) pending.resolve(reply);
    else {
      const error = new Error(reply.error || '键鼠操作失败');
      error.status = reply.status;
      pending.reject(error);
    }
  };
  socket.onclose = () => {
    if (inputSocket !== socket) return;
    inputSocket = null;
    settlePendingAfterClose();
    updateDiagnosticsCard();
    // Reconnect with backoff while the console is open; meanwhile request()
    // transparently uses HTTP, which also surfaces 401 after a logout.
    if (inputSocketWanted && !app.classList.contains('hidden')) {
      inputSocketRetry = setTimeout(connectInputSocket, inputSocketDelay);
      inputSocketDelay = Math.min(inputSocketDelay * 2, 10000);
    }
  };
}
function disconnectInputSocket() {
  inputSocketWanted = false;
  clearTimeout(inputSocketRetry);
  const socket = inputSocket;
  inputSocket = null;
  settlePendingAfterClose();
  socket?.close();
}
// Pure pointer motion is disposable: when the socket is backed up, a newer
// position supersedes it. Keys, clicks, scrolls and releases always go out.
const INPUT_SOCKET_BACKLOG_BYTES = 16 * 1024;
function sendOverInputSocket(type, body, fallback) {
  let payload = {};
  if (body) {
    try {
      payload = JSON.parse(body);
    } catch {
      payload = {};
    }
  }
  const motion = type === 'move' || (type === 'absolute' && payload.action === 'move');
  if (motion && inputSocket.bufferedAmount > INPUT_SOCKET_BACKLOG_BYTES) return Promise.resolve({ ok: true });
  const id = inputSocketNextId++;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      inputSocketPending.delete(id);
      const error = new Error('键鼠操作超时');
      error.status = 504;
      reject(error);
    }, INPUT_SOCKET_TIMEOUT_MS);
    inputSocketPending.set(id, { resolve, reject, timer, fallback, idempotent: idempotentInput(type, payload) });
    try {
      inputSocket.send(JSON.stringify({ ...payload, id, type }));
    } catch (_) {
      // The socket refused the message, so the server never saw it: HTTP
      // can carry it without any risk of running it twice.
      inputSocketPending.delete(id);
      clearTimeout(timer);
      fallback().then(resolve, reject);
    }
  });
}

export {
  connectInputSocket, disconnectInputSocket, HID_SOCKET_ROUTES, inputSocketReady, sendOverInputSocket,
};
