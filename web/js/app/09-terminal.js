import { shared } from './state.js';
import { $, $$, app } from './00-core.js';
import { tr } from './01-i18n.js';
import { fitConsoleToVideo, invalidateSurfaceBox } from './02-theme-layout.js';
import { request } from './03-session.js';
import { resetVirtualModifiers } from './08-input.js';

let terminalFit = null,
    terminalFitFrame = 0,
    terminalLastSize = '';
const terminalEncoder = new TextEncoder();
function ensureTerminal() {
  if (shared.terminal) return;
  shared.terminal = new Terminal({
    cursorBlink: true,
    cursorStyle: 'block',
    fontFamily: '"Geist Mono",ui-monospace,SFMono-Regular,Menlo,Monaco,monospace',
    fontSize: 13,
    lineHeight: 1.25,
    scrollback: 5000,
    convertEol: false,
    macOptionIsMeta: true,
    theme: {
      background: '#111111',
      foreground: '#e8e8e8',
      cursor: '#f5f5f5',
      cursorAccent: '#111111',
      selectionBackground: '#ffffff38',
      black: '#1d1f21',
      red: '#ff5f56',
      green: '#27c93f',
      yellow: '#ffbd2e',
      blue: '#5aa9ff',
      magenta: '#c792ea',
      cyan: '#66d9ef',
      white: '#e8e8e8',
      brightBlack: '#666666',
      brightRed: '#ff6e67',
      brightGreen: '#5af78e',
      brightYellow: '#f4f99d',
      brightBlue: '#7aa2f7',
      brightMagenta: '#d2a8ff',
      brightCyan: '#9aedfe',
      brightWhite: '#ffffff',
    },
  });
  terminalFit = new FitAddon.FitAddon();
  shared.terminal.loadAddon(terminalFit);
  shared.terminal.open($('#terminal-host'));
  shared.terminal.onData((data) => {
    if (shared.terminalSocket?.readyState === WebSocket.OPEN) shared.terminalSocket.send(terminalEncoder.encode(data));
  });
  shared.terminal.onResize((size) => terminalResize(size.cols, size.rows));
  new ResizeObserver(scheduleTerminalFit).observe($('#terminal-window'));
  document.fonts?.ready.then(scheduleTerminalFit);
}
function scheduleTerminalFit() {
  if (terminalFitFrame || $('#terminal-window').hidden) return;
  terminalFitFrame = requestAnimationFrame(() => {
    terminalFitFrame = 0;
    terminalFit?.fit();
  });
}
function terminalResize(cols, rows) {
  const size = `${cols}x${rows}`;
  if (shared.terminalSocket?.readyState === WebSocket.OPEN && size !== terminalLastSize) {
    terminalLastSize = size;
    shared.terminalSocket.send(JSON.stringify({ type: 'resize', cols, rows }));
  }
}
function terminalConnect() {
  ensureTerminal();
  if (shared.bootstrap.terminal_ready === false) {
    shared.terminal.writeln(
      `\r\n\x1b[31m${shared.bootstrap.terminal_message || '终端当前不可用；请退出后重新登录以重试。KVM 画面与输入不受影响。'}\x1b[0m`,
    );
    $('#terminal-reconnect')?.classList.remove('hidden');
    return;
  }
  if (shared.terminalSocket && shared.terminalSocket.readyState <= WebSocket.OPEN) return;
  const scheme = location.protocol === 'https:' ? 'wss' : 'ws';
  const socket = new WebSocket(`${scheme}://${location.host}/api/terminal/ws`);
  shared.terminalSocket = socket;
  terminalLastSize = '';
  socket.binaryType = 'arraybuffer';
  socket.onopen = () => {
    if (shared.terminalSocket !== socket) return;
    $('#terminal-reconnect')?.classList.add('hidden');
    syncTerminalTabIndicator();
    scheduleTerminalFit();
    requestAnimationFrame(() => {
      terminalResize(shared.terminal.cols, shared.terminal.rows);
      shared.terminal.focus();
    });
  };
  socket.onmessage = (event) => {
    if (shared.terminalSocket !== socket) return;
    shared.terminal.write(typeof event.data === 'string' ? event.data : new Uint8Array(event.data));
  };
  socket.onerror = () => {
    if (shared.terminalSocket !== socket) return;
    shared.terminal.writeln('\r\n\x1b[31m终端连接失败\x1b[0m');
  };
  socket.onclose = () => {
    if (shared.terminalSocket !== socket) return;
    shared.terminalSocket = null;
    syncTerminalTabIndicator();
    if (shared.videoWorkspace === 'terminal') $('#terminal-reconnect')?.classList.remove('hidden');
  };
}
// The terminal session deliberately survives switching to the video tab
// (#47); a dot on the tab shows it is still running in the background.
function syncTerminalTabIndicator() {
  const tab = $('[data-workspace="terminal"]'),
    running = !!shared.terminalSocket && shared.terminalSocket.readyState === WebSocket.OPEN && shared.videoWorkspace !== 'terminal';
  tab?.classList.toggle('session-live', running);
  if (tab) tab.title = running ? tr('终端会话仍在后台运行') : '';
}
function terminalReset() {
  if (!shared.terminal) return;
  terminalLastSize = '';
  shared.terminal.reset();
}
function terminalDisconnect() {
  if (shared.terminalSocket) {
    const socket = shared.terminalSocket;
    shared.terminalSocket = null;
    socket.close();
  }
  syncTerminalTabIndicator();
  terminalReset();
  $('#terminal-reconnect')?.classList.remove('hidden');
}
const keyboardDialog = $('#keyboard-dialog');
function syncKeyboardDialogState(open) {
  app.classList.toggle('keyboard-open', open);
  $$('#keyboard-toggle,#keyboard-toggle-mobile').forEach((btn) => {
    btn.classList.toggle('active', open);
    btn.setAttribute('aria-expanded', String(open));
  });
  invalidateSurfaceBox();
  fitConsoleToVideo();
}

// DOM wiring; main.js calls this once, in the original fragment order.
export function init() {
  $('#terminal-reconnect')?.addEventListener('click', async () => {
    try {
      shared.bootstrap = await request('/api/bootstrap');
    } catch (_) {}
    terminalReset();
    terminalConnect();
  });
  $('#terminal-clear')?.addEventListener('click', () => shared.terminal?.clear());
  $$('#keyboard-toggle,#keyboard-toggle-mobile').forEach((button) =>
    button.addEventListener('click', () => {
      if (keyboardDialog.open) {
        resetVirtualModifiers();
        keyboardDialog.close();
        syncKeyboardDialogState(false);
      } else {
        keyboardDialog.show();
        syncKeyboardDialogState(true);
      }
    }),
  );
  $('[data-close]', keyboardDialog).addEventListener('click', () => {
    resetVirtualModifiers();
    keyboardDialog.close();
    syncKeyboardDialogState(false);
  });
  keyboardDialog.addEventListener('close', () => {
    resetVirtualModifiers();
    syncKeyboardDialogState(false);
  });
}

export { scheduleTerminalFit, syncTerminalTabIndicator, terminalConnect, terminalDisconnect, terminalReset };
