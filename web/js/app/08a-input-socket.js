import { app } from './00-core.js';
import { updateDiagnosticsCard } from './05-power.js';

// Keyboard/mouse transport. While the console is open, HID requests travel
// over one WebSocket (/api/input/ws) instead of one HTTP request per event,
// which removes per-event connection and header overhead. request() routes
// the HID endpoints here when the socket is open and falls back to HTTP
// otherwise, so callers never need to know which transport carried them.
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
// A dropped socket hands its in-flight requests to HTTP, so the user sees
// neither an error nor a lost key; the server also releases held keys when
// a socket ends, so a retried key press is never left stuck.
function retryPendingOverHttp() {
  const pending = [...inputSocketPending.values()];
  inputSocketPending.clear();
  for (const { resolve, reject, timer, fallback } of pending) {
    clearTimeout(timer);
    fallback().then(resolve, reject);
  }
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
    retryPendingOverHttp();
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
  retryPendingOverHttp();
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
    inputSocketPending.set(id, { resolve, reject, timer, fallback });
    inputSocket.send(JSON.stringify({ ...payload, id, type }));
  });
}

export {
  connectInputSocket, disconnectInputSocket, HID_SOCKET_ROUTES, inputSocketReady, sendOverInputSocket,
};
