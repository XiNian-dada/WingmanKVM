  let terminalSocket = null,
    terminal = null,
    terminalFit = null,
    terminalFitFrame = 0,
    terminalLastSize = '';
  const terminalEncoder = new TextEncoder();
  function ensureTerminal() {
    if (terminal) return;
    terminal = new Terminal({
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
    terminal.loadAddon(terminalFit);
    terminal.open($('#terminal-host'));
    terminal.onData((data) => {
      if (terminalSocket?.readyState === WebSocket.OPEN) terminalSocket.send(terminalEncoder.encode(data));
    });
    terminal.onResize((size) => terminalResize(size.cols, size.rows));
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
    if (terminalSocket?.readyState === WebSocket.OPEN && size !== terminalLastSize) {
      terminalLastSize = size;
      terminalSocket.send(JSON.stringify({ type: 'resize', cols, rows }));
    }
  }
  function terminalConnect() {
    ensureTerminal();
    if (bootstrap.terminal_ready === false) {
      terminal.writeln(
        `\r\n\x1b[31m${bootstrap.terminal_message || '终端当前不可用；请退出后重新登录以重试。KVM 画面与输入不受影响。'}\x1b[0m`,
      );
      $('#terminal-reconnect')?.classList.remove('hidden');
      return;
    }
    if (terminalSocket && terminalSocket.readyState <= WebSocket.OPEN) return;
    const scheme = location.protocol === 'https:' ? 'wss' : 'ws';
    const socket = new WebSocket(`${scheme}://${location.host}/api/terminal/ws`);
    terminalSocket = socket;
    terminalLastSize = '';
    socket.binaryType = 'arraybuffer';
    socket.onopen = () => {
      if (terminalSocket !== socket) return;
      $('#terminal-reconnect')?.classList.add('hidden');
      scheduleTerminalFit();
      requestAnimationFrame(() => {
        terminalResize(terminal.cols, terminal.rows);
        terminal.focus();
      });
    };
    socket.onmessage = (event) => {
      if (terminalSocket !== socket) return;
      terminal.write(typeof event.data === 'string' ? event.data : new Uint8Array(event.data));
    };
    socket.onerror = () => {
      if (terminalSocket !== socket) return;
      terminal.writeln('\r\n\x1b[31m终端连接失败\x1b[0m');
    };
    socket.onclose = () => {
      if (terminalSocket !== socket) return;
      terminalSocket = null;
      if (videoWorkspace === 'terminal') $('#terminal-reconnect')?.classList.remove('hidden');
    };
  }
  function terminalReset() {
    if (!terminal) return;
    terminalLastSize = '';
    terminal.reset();
  }
  function terminalDisconnect() {
    if (terminalSocket) {
      const socket = terminalSocket;
      terminalSocket = null;
      socket.close();
    }
    terminalReset();
    $('#terminal-reconnect')?.classList.remove('hidden');
  }
  $('#terminal-reconnect')?.addEventListener('click', async () => {
    try {
      bootstrap = await request('/api/bootstrap');
    } catch (_) {}
    terminalReset();
    terminalConnect();
  });
  $('#terminal-clear')?.addEventListener('click', () => terminal?.clear());
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

