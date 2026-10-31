  const SETTING_DEFAULTS = {
    suppress_hid_timeout: false,
    capture_pulse: true,
    release_key_toast: true,
    render_pixelated: true,
    auto_expand_inspector: true,
  };
  function getSetting(key, fallback = null) {
    try {
      const v = localStorage.getItem(`wingman_setting_${key}`);
      if (v === null) return fallback !== null ? fallback : SETTING_DEFAULTS[key];
      if (v === 'true') return true;
      if (v === 'false') return false;
      return v;
    } catch (_) {
      return fallback !== null ? fallback : SETTING_DEFAULTS[key];
    }
  }
  function setSetting(key, value) {
    try {
      localStorage.setItem(`wingman_setting_${key}`, String(value));
    } catch (_) {}
  }
  let lastHidToast = 0;
  // Only a write timeout (HTTP 504, e.g. target powered off) is noisy enough to
  // optionally silence; "not configured"/"not ready" failures always surface.
  function handleHidError(error) {
    const message = (error && (error.message || error.error)) || String(error);
    if (error?.status === 504) {
      if (getSetting('suppress_hid_timeout', false)) return;
      const now = performance.now();
      if (now - lastHidToast < 10000) return;
      lastHidToast = now;
    }
    toast(message, true);
  }

  function interactiveTarget(target) {
    return !!target.closest(
      'input,textarea,select,button,a,summary,[role="button"],[role="tab"],[contenteditable="true"],dialog',
    );
  }
  function inputEnabled() {
    return remoteWanted && pageActive && videoWorkspace === 'video' && !app.classList.contains('hidden');
  }
  function localExitShortcut(event) {
    return (
      remoteWanted &&
      event.ctrlKey &&
      event.altKey &&
      (event.code === 'Backquote' || event.key === '`' || event.key === '~')
    );
  }
  function elementVisible(el) {
    if (!el || el.disabled) return false;
    const r = el.getBoundingClientRect(),
      style = getComputedStyle(el);
    return (
      r.width > 0 &&
      r.height > 0 &&
      r.right > 0 &&
      r.bottom > 0 &&
      r.left < innerWidth &&
      r.top < innerHeight &&
      style.visibility !== 'hidden' &&
      style.display !== 'none'
    );
  }
  function visibleLocalToggle() {
    const fullscreen = document.fullscreenElement,
      candidates = [$('#remote-input'), $('#input-state'), $('#inspector-toggle')];
    return (
      candidates.find((el) => elementVisible(el) && (!fullscreen || fullscreen.contains(el))) ||
      candidates.find(elementVisible) ||
      $('#input-state') ||
      $('#remote-input')
    );
  }
  function focusLocalToggle() {
    const target = visibleLocalToggle();
    if (!target) return;
    try {
      target.focus({ preventScroll: true });
    } catch (_) {
      try {
        target.focus();
      } catch (_) {}
    }
  }
  function exitRemoteCapture() {
    setRemote(false);
    if (getSetting('release_key_toast', true)) toast('已退出键鼠转发（本地快捷键 Ctrl+Alt+`）');
    const restore = () => requestAnimationFrame(focusLocalToggle);
    if (document.fullscreenElement) {
      try {
        const pending = document.exitFullscreen();
        if (pending && typeof pending.finally === 'function') pending.catch(() => {}).finally(restore);
        else restore();
      } catch (_) {
        restore();
      }
    } else restore();
  }
  function relativeCaptured() {
    return document.pointerLockElement === viewport;
  }
  function requestRelativeCapture() {
    try {
      const pending = viewport.requestPointerLock?.();
      if (pending && typeof pending.catch === 'function') pending.catch(() => {});
    } catch (_) {}
  }
  function relativeCaptureEnter(event) {
    return (
      event.key === 'Enter' &&
      !event.ctrlKey &&
      !event.altKey &&
      !event.metaKey &&
      !event.shiftKey &&
      document.activeElement === viewport &&
      inputEnabled() &&
      !absoluteMode() &&
      !relativeCaptured()
    );
  }
  function pulseViewport() {
    if (!getSetting('capture_pulse', true)) return;
    viewport.classList.remove('just-captured');
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        viewport.classList.add('just-captured');
        setTimeout(() => viewport.classList.remove('just-captured'), 450);
      });
    });
  }
  function syncInputUi() {
    const enabled = inputEnabled(),
      relative = enabled && !absoluteMode(),
      captured = relative && relativeCaptured();
    $$('#remote-input,.remote-input-mirror').forEach((input) => (input.checked = remoteWanted));
    viewport.classList.toggle('remote', enabled);
    const stateEl = $('#input-state');
    if (stateEl) {
      stateEl.classList.toggle('active', enabled);
      const text = !enabled
        ? t('input_paused', '输入已暂停')
        : relative
          ? captured
            ? t('input_captured', '相对鼠标已捕获')
            : t('input_click_to_capture', '点击画面或按 Enter 捕获鼠标')
          : t('input_forwarding', '键鼠正在转发');
      const textSpan = stateEl.querySelector('.input-text');
      if (textSpan) textSpan.textContent = text;
      else stateEl.textContent = text;
      stateEl.title = text + ' (点击切换键鼠转发)';
    }
    $('#input-release')?.classList.toggle('hidden', !enabled);
    updateDiagnosticsCard();
  }
  function setRemote(enabled) {
    remoteWanted = enabled;
    if (!enabled && relativeCaptured()) document.exitPointerLock();
    syncInputUi();
    if (!enabled) releaseAll();
    else pulseViewport();
  }
  $$('#remote-input,.remote-input-mirror').forEach((input) =>
    input.addEventListener('change', (event) => setRemote(event.target.checked)),
  );
  $('#input-state')?.addEventListener('click', (event) => {
    setRemote(!remoteWanted);
    // A pointer click would otherwise leave focus on this button, where
    // typing is treated as local UI input and Enter toggles forwarding off.
    if (remoteWanted && event.detail > 0) viewport.focus({ preventScroll: true });
    if (getSetting('release_key_toast', true))
      toast(
        remoteWanted ? t('toast_input_started', '已开启键鼠转发') : t('toast_input_paused', '已暂停键鼠转发'),
      );
  });
  $('#input-release')?.addEventListener('click', async () => {
    if (relativeCaptured()) document.exitPointerLock();
    try {
      await releaseAll({ strict: true });
    } catch (error) {
      toast(t('input_release_failed', '释放按键失败，被控机可能仍有按键处于按下状态') + `：${error.message}`, true);
      return;
    }
    if (getSetting('release_key_toast', true)) toast(t('input_released', '已释放所有按键与鼠标锁定'));
  });
  const VIRTUAL_MODIFIERS = new Set(['Control', 'Shift', 'Alt', 'Meta']);
  const activeModifiers = new Set();
  function updateVirtualModifierButtons() {
    $('#virtual-keyboard')
      ?.querySelectorAll('button[data-key]')
      .forEach((b) => {
        if (VIRTUAL_MODIFIERS.has(b.dataset.key)) {
          b.classList.toggle('active', activeModifiers.has(b.dataset.key));
        }
      });
  }
  function resetVirtualModifiers() {
    if (activeModifiers.size > 0) {
      activeModifiers.clear();
      updateVirtualModifierButtons();
    }
  }
  async function sendKey(key, event = {}) {
    if (!inputEnabled()) {
      toast(t('input_paused', '请先开启“转发键鼠”'), true);
      return;
    }
    const res = await request('/api/key', {
      method: 'POST',
      body: JSON.stringify({
        key,
        ctrl: !!event.ctrlKey || activeModifiers.has('Control'),
        shift: !!event.shiftKey || activeModifiers.has('Shift'),
        alt: !!event.altKey || activeModifiers.has('Alt'),
        meta: !!event.metaKey || activeModifiers.has('Meta'),
        hold_ms: key.startsWith('F') ? 100 : 25,
      }),
    });
    if (res && res.keyboard_leds) renderKeyboardLeds(res.keyboard_leds);
    return res;
  }
  async function sendQuickKey(action) {
    if (!inputEnabled()) setRemote(true);
    try {
      let res;
      if (action === 'cad') {
        res = await request('/api/key', {
          method: 'POST',
          body: JSON.stringify({ key: 'Delete', ctrl: true, alt: true, hold_ms: 100 }),
        });
        toast('已发送 Ctrl+Alt+Del');
      } else if (action === 'win') {
        res = await request('/api/key', {
          method: 'POST',
          body: JSON.stringify({ key: 'Meta', meta: true, hold_ms: 60 }),
        });
        toast('已发送 Win');
      } else if (action === 'alttab') {
        res = await request('/api/key', {
          method: 'POST',
          body: JSON.stringify({ key: 'Tab', alt: true, hold_ms: 100 }),
        });
        toast('已发送 Alt+Tab');
      } else if (action === 'esc') {
        res = await request('/api/key', {
          method: 'POST',
          body: JSON.stringify({ key: 'Escape', hold_ms: 40 }),
        });
        toast('已发送 Esc');
      }
      if (res && res.keyboard_leds) renderKeyboardLeds(res.keyboard_leds);
    } catch (e) {
      toast(e.message, true);
    }
  }
  $$('[data-quick-key]').forEach((button) =>
    button.addEventListener('click', () => sendQuickKey(button.dataset.quickKey)),
  );
  let latestKeyboardLeds = { num_lock: false, caps_lock: false, scroll_lock: false, raw: 0 };
  function renderKeyboardLeds(leds) {
    if (!leds) return;
    latestKeyboardLeds = leds;
    const num = !!leds.num_lock,
      caps = !!leds.caps_lock,
      scrl = !!leds.scroll_lock;
    $('#led-num')?.classList.toggle('active', num);
    $('#led-caps')?.classList.toggle('active', caps);
    $('#led-scroll')?.classList.toggle('active', scrl);
    $('#inspector-led-num')?.classList.toggle('active', num);
    $('#inspector-led-caps')?.classList.toggle('active', caps);
    $('#inspector-led-scroll')?.classList.toggle('active', scrl);
    $('#virtual-keyboard')?.querySelector('[data-key="CapsLock"]')?.classList.toggle('active', caps);
  }
  $$('[data-led]').forEach((button) =>
    button.addEventListener('click', async (e) => {
      e.stopPropagation();
      const key = button.dataset.led;
      if (!key) return;
      if (!inputEnabled()) setRemote(true);
      try {
        const res = await sendKey(key);
        if (res && res.keyboard_leds) renderKeyboardLeds(res.keyboard_leds);
        setTimeout(async () => {
          try {
            const l = await request('/api/hid/leds');
            renderKeyboardLeds(l);
          } catch (_) {}
        }, 120);
      } catch (err) {
        handleHidError(err);
      }
    }),
  );
  document.addEventListener('keydown', (event) => {
    if (localExitShortcut(event)) {
      event.preventDefault();
      event.stopPropagation();
      if (!event.repeat) exitRemoteCapture();
      return;
    }
    if (relativeCaptureEnter(event)) {
      event.preventDefault();
      event.stopPropagation();
      if (!event.repeat) requestRelativeCapture();
      return;
    }
    if (!inputEnabled() || interactiveTarget(event.target) || event.repeat) return;
    const key = normalizeKey(event);
    if (!key) return;
    event.preventDefault();
    sendKey(key, event).catch(handleHidError);
  });
  function normalizeKey(event) {
    const code = event.code;
    if (
      /^Key[A-Z]$/.test(code) ||
      /^Digit[0-9]$/.test(code) ||
      /^F(?:[1-9]|1[0-2])$/.test(code) ||
      [
        'Escape',
        'Delete',
        'Enter',
        'Tab',
        'Backspace',
        'Space',
        'ArrowUp',
        'ArrowDown',
        'ArrowLeft',
        'ArrowRight',
        'Home',
        'End',
        'PageUp',
        'PageDown',
        'Insert',
        'Minus',
        'Equal',
        'BracketLeft',
        'BracketRight',
        'Backslash',
        'Semicolon',
        'Quote',
        'Backquote',
        'Comma',
        'Period',
        'Slash',
        'CapsLock',
      ].includes(code)
    )
      return code;
    return null;
  }
  $$('.key-strip [data-key]').forEach((button) =>
    button.addEventListener('click', () => sendKey(button.dataset.key).catch(handleHidError)),
  );
  // Background releases (blur, logout, pause) are best effort; the explicit
  // "release keys" button passes `strict` so the user learns if it failed.
  function releaseAll({ strict = false } = {}) {
    mouseX = mouseY = 0;
    absolutePending = null;
    resetVirtualModifiers();
    if (app.classList.contains('hidden') || bootstrap.authenticated === false) return Promise.resolve();
    const released = request('/api/input/release-all', { method: 'POST', keepalive: true });
    return strict ? released : released.catch(() => {});
  }
  function stopInput() {
    pageActive = false;
    remoteWanted = false;
    resetVirtualModifiers();
    if (relativeCaptured()) document.exitPointerLock();
    syncInputUi();
    return releaseAll();
  }
  function suspendInput() {
    pageActive = false;
    if (relativeCaptured()) document.exitPointerLock();
    syncInputUi();
    return releaseAll();
  }
  function resumeInput() {
    pageActive = !document.hidden && document.hasFocus();
    syncInputUi();
    if (inputEnabled()) scheduleMouse();
  }
  function handleVideoVisibility() {
    if (shouldVideoRun()) {
      startVideo();
      refreshStatus();
    } else if (document.hidden || videoWorkspace !== 'video' || app.classList.contains('hidden')) stopVideo();
  }
  document.addEventListener('visibilitychange', () => {
    if (document.hidden) {
      suspendInput();
      stopVideo();
    } else {
      resumeInput();
      handleVideoVisibility();
    }
  });
  window.addEventListener('blur', suspendInput);
  window.addEventListener('focus', resumeInput);
  window.addEventListener('pagehide', () => {
    suspendInput();
    stopVideo();
  });
  window.addEventListener('pageshow', () => {
    resumeInput();
    handleVideoVisibility();
  });

  function absoluteMode() {
    return bootstrap.capabilities?.pointer_mode === 'absolute';
  }
  function activeVideoSurface() {
    return activeVideoTransport === 'webrtc_h264' ? webrtcFeed : feed;
  }
  function mapAbsolute(clientX, clientY) {
    const surface = activeVideoSurface(),
      sourceWidth = surface instanceof HTMLVideoElement ? surface.videoWidth : surface.naturalWidth,
      sourceHeight = surface instanceof HTMLVideoElement ? surface.videoHeight : surface.naturalHeight;
    if (!sourceWidth || !sourceHeight) return null;
    if (!cachedSurfaceBox) cachedSurfaceBox = surface.getBoundingClientRect();
    const box = cachedSurfaceBox;
    if (!box.width || !box.height) return null;
    let left = box.left,
      top = box.top,
      width = box.width,
      height = box.height;
    if (consoleBox.classList.contains('mode-fit')) {
      const scale = Math.min(box.width / sourceWidth, box.height / sourceHeight);
      width = sourceWidth * scale;
      height = sourceHeight * scale;
      left = box.left + (box.width - width) / 2;
      top = box.top + (box.height - height) / 2;
    }
    const nx = (clientX - left) / width,
      ny = (clientY - top) / height;
    if (nx < 0 || nx >= 1 || ny < 0 || ny >= 1) return null;
    return {
      x: Math.max(0, Math.min(32767, Math.round(nx * 32767))),
      y: Math.max(0, Math.min(32767, Math.round(ny * 32767))),
    };
  }
  document.addEventListener('pointerlockchange', () => {
    if (!relativeCaptured()) mouseX = mouseY = 0;
    syncInputUi();
  });
  document.addEventListener('pointerlockerror', () => toast('浏览器未允许捕获鼠标', true));
  viewport.addEventListener('pointermove', (event) => {
    if (isDraggingPan) {
      event.preventDefault();
      event.stopPropagation();
      const dx = event.clientX - panStartX,
        dy = event.clientY - panStartY;
      viewport.scrollLeft = panScrollX - dx;
      viewport.scrollTop = panScrollY - dy;
      invalidateSurfaceBox();
      return;
    }
    if (!inputEnabled()) return;
    if (absoluteMode()) {
      const point = mapAbsolute(event.clientX, event.clientY);
      absolutePending = point;
      if (!point) return;
    } else {
      if (!relativeCaptured()) return;
      mouseX += event.movementX;
      mouseY += event.movementY;
    }
    scheduleMouse();
  });
  viewport.addEventListener('pointerdown', (event) => {
    if (event.button > 2) return;
    if (
      consoleBox.classList.contains('mode-native') &&
      (isPanActive || event.button === 1 || event.altKey || !inputEnabled())
    ) {
      event.preventDefault();
      event.stopPropagation();
      isDraggingPan = true;
      panStartX = event.clientX;
      panStartY = event.clientY;
      panScrollX = viewport.scrollLeft;
      panScrollY = viewport.scrollTop;
      viewport.classList.add('panning-active');
      try {
        viewport.setPointerCapture?.(event.pointerId);
      } catch (_) {}
      return;
    }
    if (!inputEnabled()) {
      setRemote(true);
      if (getSetting('release_key_toast', true)) toast('已激活键鼠转发 (点击状态栏或关闭开关可暂停)');
      viewport.focus({ preventScroll: true });
      return;
    }
    event.preventDefault();
    viewport.focus({ preventScroll: true });
    if (!absoluteMode() && !relativeCaptured()) {
      requestRelativeCapture();
      pulseViewport();
      return;
    }
    const button = event.button === 0 ? 1 : event.button === 2 ? 2 : 4;
    if (absoluteMode()) {
      const point = mapAbsolute(event.clientX, event.clientY);
      absolutePending = point;
      if (!point) return;
      request('/api/mouse/absolute', {
        method: 'POST',
        body: JSON.stringify({ action: 'click', ...point, button }),
      }).catch(handleHidError);
    } else
      request('/api/mouse/click', { method: 'POST', body: JSON.stringify({ button }) }).catch(handleHidError);
  });
  function stopPanning(event) {
    if (isDraggingPan) {
      isDraggingPan = false;
      viewport.classList.remove('panning-active');
      try {
        if (event?.pointerId) viewport.releasePointerCapture?.(event.pointerId);
      } catch (_) {}
      invalidateSurfaceBox();
    }
  }
  viewport.addEventListener('pointerup', stopPanning);
  viewport.addEventListener('pointercancel', stopPanning);
  viewport.addEventListener('contextmenu', (event) => {
    if (inputEnabled() || isPanActive) event.preventDefault();
  });
  viewport.addEventListener(
    'wheel',
    (event) => {
      if (consoleBox.classList.contains('mode-native') && (isPanActive || event.altKey || !inputEnabled())) {
        event.preventDefault();
        viewport.scrollLeft += event.deltaX || (event.shiftKey ? event.deltaY : 0);
        viewport.scrollTop += event.shiftKey ? 0 : event.deltaY;
        invalidateSurfaceBox();
        return;
      }
      if (!inputEnabled() || (!absoluteMode() && !relativeCaptured())) return;
      event.preventDefault();
      const wheel = Math.max(-127, Math.min(127, Math.round(-event.deltaY / 40) || Math.sign(-event.deltaY)));
      if (absoluteMode()) {
        const point = mapAbsolute(event.clientX, event.clientY);
        absolutePending = point;
        if (!point) return;
        request('/api/mouse/absolute', {
          method: 'POST',
          body: JSON.stringify({ action: 'scroll', ...point, delta: wheel }),
        }).catch(handleHidError);
      } else
        request('/api/mouse/scroll', { method: 'POST', body: JSON.stringify({ wheel }) }).catch(
          handleHidError,
        );
    },
    { passive: false },
  );
  function scheduleMouse() {
    const pending = absoluteMode() ? absolutePending : mouseX || mouseY;
    if (mouseBusy || mouseTimer || !pending) return;
    const wait = Math.max(0, 16 - (performance.now() - lastMouseSend));
    mouseTimer = setTimeout(flushMouse, wait);
  }
  async function flushMouse() {
    mouseTimer = 0;
    if (mouseBusy || !inputEnabled()) return;
    let url, body;
    if (absoluteMode()) {
      if (!absolutePending) return;
      const point = absolutePending;
      absolutePending = null;
      url = '/api/mouse/absolute';
      body = { action: 'move', ...point };
    } else {
      const dx = Math.max(-127, Math.min(127, mouseX)),
        dy = Math.max(-127, Math.min(127, mouseY));
      if (!dx && !dy) return;
      mouseX -= dx;
      mouseY -= dy;
      url = '/api/mouse/move';
      body = { dx, dy };
    }
    mouseBusy = true;
    lastMouseSend = performance.now();
    try {
      await request(url, { method: 'POST', body: JSON.stringify(body) });
    } catch (e) {
      handleHidError(e);
    } finally {
      mouseBusy = false;
      scheduleMouse();
    }
  }

  const rows = [
    ['Escape', 'F1', 'F2', 'F3', 'F4', 'F5', 'F6', 'F7', 'F8', 'F9', 'F10', 'F11', 'F12', 'Delete'],
    ['`', '1', '2', '3', '4', '5', '6', '7', '8', '9', '0', '-', '=', 'Backspace'],
    ['Tab', 'Q', 'W', 'E', 'R', 'T', 'Y', 'U', 'I', 'O', 'P', '[', ']', '\\'],
    ['CapsLock', 'A', 'S', 'D', 'F', 'G', 'H', 'J', 'K', 'L', ';', "'", 'Enter'],
    ['Shift', 'Z', 'X', 'C', 'V', 'B', 'N', 'M', ',', '.', '/', 'Shift'],
    ['Control', 'Meta', 'Alt', 'Spacebar', 'Alt', 'Meta', 'Control'],
  ];
  const vk = $('#virtual-keyboard');
  rows.forEach((row) => {
    const line = document.createElement('div');
    line.className = 'keyboard-row';
    row.forEach((key) => {
      const b = document.createElement('button');
      b.type = 'button';
      b.textContent = key === 'Spacebar' ? '空格' : key;
      b.dataset.key = key;
      if (key === 'Spacebar') b.className = 'grow';
      // Keep focus off on-screen keys so a later physical Enter/Space cannot
      // re-activate the last key that was clicked.
      b.addEventListener('pointerdown', (event) => event.preventDefault());
      b.addEventListener('click', () => {
        if (VIRTUAL_MODIFIERS.has(key)) {
          if (activeModifiers.has(key)) activeModifiers.delete(key);
          else activeModifiers.add(key);
          updateVirtualModifierButtons();
          return;
        }
        sendKey(key).catch(handleHidError);
      });
      line.append(b);
    });
    vk.append(line);
  });
