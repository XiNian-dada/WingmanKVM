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
      stateEl.setAttribute('aria-pressed', String(enabled));
      const touchInput = $('[data-touch-action="input"]');
      if (touchInput) {
        touchInput.textContent = enabled ? '释放' : '接管';
        touchInput.classList.toggle('active', enabled);
      }
      // Short, always-visible label: whether typing goes to the target must be
      // obvious at a glance. The long explanation stays in the tooltip.
      const label = !enabled ? '点击接管' : relative ? (captured ? '已捕获' : '点击捕获') : '正在控制';
      const text = !enabled
        ? t('input_paused', '输入已暂停')
        : relative
          ? captured
            ? t('input_captured', '相对鼠标已捕获')
            : t('input_click_to_capture', '点击画面或按 Enter 捕获鼠标')
          : t('input_forwarding', '键鼠正在转发');
      const textSpan = stateEl.querySelector('.input-text');
      if (textSpan) textSpan.textContent = label;
      else stateEl.textContent = label;
      stateEl.title = `${text} ${tr('(点击切换键鼠转发)')}`;
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
          b.setAttribute('aria-pressed', String(activeModifiers.has(b.dataset.key)));
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

  // Paste-as-typing: turn arbitrary text into US-layout keystrokes. Kept pure
  // so the mapping can be exercised without touching the network or DOM.
  const US_UNSHIFTED_SYMBOLS = {
    '`': 'Backquote',
    '-': 'Minus',
    '=': 'Equal',
    '[': 'BracketLeft',
    ']': 'BracketRight',
    '\\': 'Backslash',
    ';': 'Semicolon',
    "'": 'Quote',
    ',': 'Comma',
    '.': 'Period',
    '/': 'Slash',
  };
  const US_SHIFTED_SYMBOLS = {
    '~': 'Backquote',
    '!': 'Digit1',
    '@': 'Digit2',
    '#': 'Digit3',
    $: 'Digit4',
    '%': 'Digit5',
    '^': 'Digit6',
    '&': 'Digit7',
    '*': 'Digit8',
    '(': 'Digit9',
    ')': 'Digit0',
    _: 'Minus',
    '+': 'Equal',
    '{': 'BracketLeft',
    '}': 'BracketRight',
    '|': 'Backslash',
    ':': 'Semicolon',
    '"': 'Quote',
    '<': 'Comma',
    '>': 'Period',
    '?': 'Slash',
  };
  function textToKeystrokes(text) {
    const strokes = [],
      unsupported = [],
      seenUnsupported = new Set(),
      normalized = String(text ?? '').replace(/\r\n?/g, '\n');
    for (const ch of Array.from(normalized)) {
      let key = null,
        shift = false;
      if (/^[a-z]$/.test(ch)) key = `Key${ch.toUpperCase()}`;
      else if (/^[A-Z]$/.test(ch)) {
        key = `Key${ch}`;
        shift = true;
      } else if (/^[0-9]$/.test(ch)) key = `Digit${ch}`;
      else if (ch === ' ') key = 'Space';
      else if (ch === '\n') key = 'Enter';
      else if (ch === '\t') key = 'Tab';
      else if (US_UNSHIFTED_SYMBOLS[ch]) key = US_UNSHIFTED_SYMBOLS[ch];
      else if (US_SHIFTED_SYMBOLS[ch]) {
        key = US_SHIFTED_SYMBOLS[ch];
        shift = true;
      }
      if (!key) {
        if (!seenUnsupported.has(ch)) {
          seenUnsupported.add(ch);
          unsupported.push(ch);
        }
        continue;
      }
      strokes.push({ key, shift, ctrl: false, alt: false, meta: false });
    }
    return { strokes, unsupported };
  }
  function displayUnsupportedChar(ch) {
    return /[\u0000-\u001f\u007f]/.test(ch)
      ? `U+${ch.codePointAt(0).toString(16).toUpperCase().padStart(4, '0')}`
      : ch;
  }
  let pasteRun = null;
  function setPasteStatus(text, tone = '') {
    const el = $('#paste-key-status');
    if (!el) return;
    el.textContent = text;
    el.classList.toggle('error', tone === 'error');
  }
  function updatePasteProgress(done, total) {
    const bar = $('#paste-key-bar'),
      progress = $('#paste-key-progress');
    if (bar) bar.style.width = total ? `${Math.round((done / total) * 100)}%` : '0%';
    if (progress) {
      progress.setAttribute('aria-valuemax', String(total));
      progress.setAttribute('aria-valuenow', String(done));
    }
  }
  function openPasteDialog() {
    const dialog = $('#paste-key-dialog'),
      input = $('#paste-key-text'),
      unsupportedEl = $('#paste-key-unsupported'),
      sendButton = $('#paste-key-send'),
      cancelButton = $('#paste-key-cancel');
    if (!dialog) return;
    if (input) input.value = '';
    if (unsupportedEl) {
      unsupportedEl.hidden = true;
      unsupportedEl.textContent = '';
    }
    updatePasteProgress(0, 0);
    setPasteStatus('');
    if (sendButton) sendButton.disabled = false;
    if (cancelButton) cancelButton.textContent = tr('取消');
    try {
      if (typeof dialog.showModal === 'function') dialog.showModal();
      else dialog.show?.();
    } catch (_) {
      try {
        dialog.show?.();
      } catch (__) {}
    }
    setTimeout(() => input?.focus(), 0);
  }
  function closePasteDialog() {
    const dialog = $('#paste-key-dialog');
    try {
      dialog?.close?.();
    } catch (_) {}
  }
  async function startPasteTyping() {
    const input = $('#paste-key-text'),
      unsupportedEl = $('#paste-key-unsupported'),
      sendButton = $('#paste-key-send'),
      cancelButton = $('#paste-key-cancel');
    const { strokes, unsupported } = textToKeystrokes(input ? input.value : '');
    if (unsupported.length) {
      if (unsupportedEl) {
        unsupportedEl.hidden = false;
        unsupportedEl.textContent = `${tr('无法表示的字符：')}${unsupported
          .map(displayUnsupportedChar)
          .join(' ')}`;
      }
      setPasteStatus(tr('已阻止发送，请先移除无法表示的字符'), 'error');
      toast(tr('文本包含无法表示的字符，已阻止发送'), true);
      return;
    }
    if (unsupportedEl) {
      unsupportedEl.hidden = true;
      unsupportedEl.textContent = '';
    }
    if (!strokes.length) {
      setPasteStatus(tr('没有可发送的字符'));
      return;
    }
    if (!inputEnabled()) setRemote(true);
    if (!inputEnabled()) {
      const message = tr('当前无法发送：请切换到远程画面并开启键鼠转发');
      setPasteStatus(message, 'error');
      toast(message, true);
      return;
    }
    resetVirtualModifiers();
    let canceled = false;
    pasteRun = {
      cancel: () => {
        canceled = true;
      },
    };
    if (sendButton) sendButton.disabled = true;
    if (cancelButton) cancelButton.textContent = tr('取消发送');
    updatePasteProgress(0, strokes.length);
    setPasteStatus(tr(`正在键入 0 / ${strokes.length}`));
    let sent = 0,
      interrupted = false;
    // Letters are typed for the target's current Caps Lock state, otherwise
    // a pasted password would arrive with every letter's case inverted.
    const capsLock = !!latestKeyboardLeds.caps_lock;
    try {
      while (sent < strokes.length) {
        if (canceled) break;
        // sendKey() only toasts when forwarding is off; stop instead of
        // reporting a partial paste as complete.
        if (!inputEnabled()) {
          interrupted = true;
          break;
        }
        const stroke = strokes[sent];
        await sendKey(stroke.key, {
          ctrlKey: stroke.ctrl,
          shiftKey: stroke.shift !== (capsLock && stroke.key.startsWith('Key')),
          altKey: stroke.alt,
          metaKey: stroke.meta,
        });
        sent += 1;
        updatePasteProgress(sent, strokes.length);
        setPasteStatus(tr(`正在键入 ${sent} / ${strokes.length}`));
        if (sent < strokes.length) await new Promise((resolve) => setTimeout(resolve, 15));
      }
      if (interrupted) {
        const message = tr(`已中断：键鼠转发已关闭，已发送 ${sent} / ${strokes.length}`);
        setPasteStatus(message, 'error');
        toast(message, true);
      } else if (canceled) {
        setPasteStatus(tr(`已取消，已发送 ${sent} / ${strokes.length}`));
      } else {
        setPasteStatus(tr(`已发送完成（${strokes.length} 个按键）`));
        toast(tr(`已发送 ${strokes.length} 个按键`));
      }
    } catch (error) {
      setPasteStatus(`${tr('发送失败：')}${error.message}`, 'error');
      toast(error.message, true);
    } finally {
      pasteRun = null;
      if (sendButton) sendButton.disabled = false;
      if (cancelButton) cancelButton.textContent = tr('取消');
    }
  }
  $('#paste-typing-btn')?.addEventListener('click', openPasteDialog);
  $$('[data-paste-close]').forEach((button) => button.addEventListener('click', closePasteDialog));
  $('#paste-key-send')?.addEventListener('click', startPasteTyping);
  $('#paste-key-cancel')?.addEventListener('click', () => {
    if (pasteRun) {
      pasteRun.cancel();
      setPasteStatus(tr('正在取消…'));
      return;
    }
    closePasteDialog();
  });
  $('#paste-key-dialog')?.addEventListener('click', (event) => {
    if (event.target === event.currentTarget) closePasteDialog();
  });
  $('#paste-key-dialog')?.addEventListener('close', () => {
    pasteRun?.cancel();
    pasteRun = null;
  });

  // More shortcuts: less-used built-ins plus user-defined combinations.
  const CUSTOM_COMBOS_KEY = 'wingman_custom_combos';
  const BUILTIN_MENU_COMBOS = [
    { id: 'caf1', key: 'F1', ctrl: true, alt: true, hold_ms: 100 },
    { id: 'caf2', key: 'F2', ctrl: true, alt: true, hold_ms: 100 },
    { id: 'caf3', key: 'F3', ctrl: true, alt: true, hold_ms: 100 },
    { id: 'caf4', key: 'F4', ctrl: true, alt: true, hold_ms: 100 },
    { id: 'caf5', key: 'F5', ctrl: true, alt: true, hold_ms: 100 },
    { id: 'caf6', key: 'F6', ctrl: true, alt: true, hold_ms: 100 },
    { id: 'caf7', key: 'F7', ctrl: true, alt: true, hold_ms: 100 },
    { id: 'prtsc', key: 'PrintScreen', hold_ms: 60 },
  ];
  function comboKeyLabel(key) {
    if (/^Key[A-Z]$/.test(key)) return key.slice(3);
    if (/^Digit[0-9]$/.test(key)) return key.slice(5);
    return (
      {
        Escape: 'Esc',
        Space: 'Space',
        PrintScreen: 'PrtSc',
        ArrowUp: '↑',
        ArrowDown: '↓',
        ArrowLeft: '←',
        ArrowRight: '→',
      }[key] || key
    );
  }
  function comboLabel(combo) {
    const parts = [];
    if (combo.ctrl) parts.push('Ctrl');
    if (combo.alt) parts.push('Alt');
    if (combo.shift) parts.push('Shift');
    if (combo.meta) parts.push('Meta');
    parts.push(comboKeyLabel(combo.key));
    return parts.join('+');
  }
  function readCustomCombos() {
    try {
      const raw = JSON.parse(localStorage.getItem(CUSTOM_COMBOS_KEY) || '[]');
      if (!Array.isArray(raw)) return [];
      return raw
        .filter((combo) => combo && typeof combo.key === 'string' && combo.key)
        .map((combo, index) => ({
          id: String(combo.id || `custom_restored_${index}`),
          key: combo.key,
          ctrl: !!combo.ctrl,
          alt: !!combo.alt,
          shift: !!combo.shift,
          meta: !!combo.meta,
          hold_ms: Number(combo.hold_ms) || undefined,
        }));
    } catch (_) {
      return [];
    }
  }
  function writeCustomCombos(combos) {
    try {
      localStorage.setItem(CUSTOM_COMBOS_KEY, JSON.stringify(combos));
      return true;
    } catch (_) {
      return false;
    }
  }
  async function sendCombo(combo) {
    if (!combo) return;
    if (!inputEnabled()) setRemote(true);
    try {
      const res = await request('/api/key', {
        method: 'POST',
        body: JSON.stringify({
          key: combo.key,
          ctrl: !!combo.ctrl,
          shift: !!combo.shift,
          alt: !!combo.alt,
          meta: !!combo.meta,
          hold_ms: combo.hold_ms || (combo.key.startsWith('F') ? 100 : 40),
        }),
      });
      if (res && res.keyboard_leds) renderKeyboardLeds(res.keyboard_leds);
      toast(tr(`已发送组合键 ${comboLabel(combo)}`));
    } catch (error) {
      toast(error.message, true);
    }
  }
  function createComboItem(combo, custom) {
    const row = document.createElement('div');
    row.className = 'quick-more-row';
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'quick-combo-item';
    button.setAttribute('role', 'menuitem');
    button.dataset.comboId = combo.id;
    button.dataset.key = combo.key;
    button.dataset.ctrl = String(!!combo.ctrl);
    button.dataset.alt = String(!!combo.alt);
    button.dataset.shift = String(!!combo.shift);
    button.dataset.meta = String(!!combo.meta);
    if (custom) button.dataset.customCombo = '1';
    button.textContent = comboLabel(combo);
    button.addEventListener('click', () => {
      closeQuickMore();
      sendCombo(combo);
    });
    row.append(button);
    if (custom) {
      const remove = document.createElement('button');
      remove.type = 'button';
      remove.className = 'quick-combo-delete';
      remove.dataset.removeCombo = combo.id;
      remove.title = tr('删除此组合键');
      remove.setAttribute('aria-label', tr('删除此组合键'));
      remove.textContent = '×';
      remove.addEventListener('click', (event) => {
        event.stopPropagation();
        writeCustomCombos(readCustomCombos().filter((item) => item.id !== combo.id));
        renderQuickCombos();
        toast(tr('已删除自定义组合键'));
      });
      row.append(remove);
    }
    return row;
  }
  function renderQuickCombos() {
    const menu = $('#quick-more-menu');
    if (!menu) return;
    menu.textContent = '';
    // The inline quick keys hide when the console is narrow; repeat them here
    // (CSS shows this group only in that case) so they are always reachable.
    const common = document.createElement('div');
    common.className = 'quick-menu-common';
    for (const [action, label] of [
      ['cad', 'Ctrl+Alt+Del'],
      ['win', 'Win'],
      ['alttab', 'Alt+Tab'],
      ['esc', 'Esc'],
    ]) {
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'quick-combo-item';
      button.setAttribute('role', 'menuitem');
      button.textContent = label;
      button.addEventListener('click', () => {
        closeQuickMore();
        sendQuickKey(action);
      });
      common.append(button);
    }
    const commonSep = document.createElement('div');
    commonSep.className = 'quick-more-sep';
    common.append(commonSep);
    menu.append(common);
    BUILTIN_MENU_COMBOS.forEach((combo) => menu.append(createComboItem(combo, false)));
    const custom = readCustomCombos();
    if (custom.length) {
      const separator = document.createElement('div');
      separator.className = 'quick-more-sep';
      menu.append(separator);
      custom.forEach((combo) => menu.append(createComboItem(combo, true)));
    }
    const add = document.createElement('button');
    add.type = 'button';
    add.id = 'quick-combo-add';
    add.className = 'quick-more-action';
    add.setAttribute('role', 'menuitem');
    add.textContent = tr('+ 添加自定义组合');
    add.addEventListener('click', () => {
      closeQuickMore();
      openComboDialog();
    });
    menu.append(add);
  }
  function closeQuickMore() {
    $('#quick-more-menu')?.setAttribute('hidden', '');
    $('#quick-more-toggle')?.setAttribute('aria-expanded', 'false');
  }
  function fillComboKeyOptions() {
    const select = $('#combo-key');
    if (!select || select.options.length) return;
    const options = [];
    for (const letter of 'ABCDEFGHIJKLMNOPQRSTUVWXYZ') options.push([`Key${letter}`, letter]);
    for (let digit = 0; digit <= 9; digit += 1) options.push([`Digit${digit}`, String(digit)]);
    for (let fn = 1; fn <= 12; fn += 1) options.push([`F${fn}`, `F${fn}`]);
    options.push(
      ['Escape', 'Esc'],
      ['Tab', 'Tab'],
      ['Enter', 'Enter'],
      ['Space', 'Space'],
      ['Backspace', 'Backspace'],
      ['Delete', 'Delete'],
      ['Insert', 'Insert'],
      ['Home', 'Home'],
      ['End', 'End'],
      ['PageUp', 'PageUp'],
      ['PageDown', 'PageDown'],
      ['ArrowUp', '↑'],
      ['ArrowDown', '↓'],
      ['ArrowLeft', '←'],
      ['ArrowRight', '→'],
      ['PrintScreen', 'PrtSc'],
      ['Minus', '-'],
      ['Equal', '='],
      ['BracketLeft', '['],
      ['BracketRight', ']'],
      ['Backslash', '\\'],
      ['Semicolon', ';'],
      ['Quote', "'"],
      ['Comma', ','],
      ['Period', '.'],
      ['Slash', '/'],
    );
    options.forEach(([value, label]) => {
      const option = document.createElement('option');
      option.value = value;
      option.textContent = label;
      select.append(option);
    });
  }
  function openComboDialog() {
    const dialog = $('#combo-dialog');
    if (!dialog) return;
    fillComboKeyOptions();
    $('#combo-form')?.reset();
    try {
      if (typeof dialog.showModal === 'function') dialog.showModal();
      else dialog.show?.();
    } catch (_) {
      try {
        dialog.show?.();
      } catch (__) {}
    }
  }
  function closeComboDialog() {
    const dialog = $('#combo-dialog');
    try {
      dialog?.close?.();
    } catch (_) {}
  }
  function addCustomCombo() {
    const form = $('#combo-form'),
      key = form?.querySelector('[name="key"]')?.value;
    if (!form || !key) return;
    const combo = {
      ctrl: !!form.querySelector('[name="ctrl"]')?.checked,
      alt: !!form.querySelector('[name="alt"]')?.checked,
      shift: !!form.querySelector('[name="shift"]')?.checked,
      meta: !!form.querySelector('[name="meta"]')?.checked,
    };
    if (!combo.ctrl && !combo.alt && !combo.shift && !combo.meta) {
      toast(tr('请至少选择一个修饰键'), true);
      return;
    }
    const custom = readCustomCombos();
    if (custom.length >= 24) {
      toast(tr('自定义组合键最多 24 个'), true);
      return;
    }
    custom.push({
      id: `custom_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 6)}`,
      key,
      ...combo,
    });
    if (!writeCustomCombos(custom)) {
      toast(tr('无法保存自定义组合键'), true);
      return;
    }
    renderQuickCombos();
    closeComboDialog();
    toast(tr('已添加自定义组合键'));
  }
  const quickMoreToggle = $('#quick-more-toggle'),
    quickMoreMenu = $('#quick-more-menu');
  quickMoreToggle?.addEventListener('click', (event) => {
    event.stopPropagation();
    if (!quickMoreMenu) return;
    const open = !quickMoreMenu.hasAttribute('hidden');
    if (open) {
      closeQuickMore();
      return;
    }
    renderQuickCombos();
    quickMoreMenu.removeAttribute('hidden');
    quickMoreToggle.setAttribute('aria-expanded', 'true');
    $('#power-menu')?.setAttribute('hidden', '');
    $('#power-menu-toggle')?.setAttribute('aria-expanded', 'false');
    $('#lang-menu')?.setAttribute('hidden', '');
    $('#lang-toggle')?.setAttribute('aria-expanded', 'false');
    $('#diagnostics-popover')?.setAttribute('hidden', '');
    $('#diagnostics-toggle')?.setAttribute('aria-expanded', 'false');
  });
  document.addEventListener('click', (event) => {
    if (!event.target.closest('.quick-more')) closeQuickMore();
  });
  $$('[data-combo-close]').forEach((button) => button.addEventListener('click', closeComboDialog));
  $('#combo-dialog')?.addEventListener('click', (event) => {
    if (event.target === event.currentTarget) closeComboDialog();
  });
  $('#combo-form')?.addEventListener('submit', (event) => {
    event.preventDefault();
    addCustomCombo();
  });
  fillComboKeyOptions();
  renderQuickCombos();

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
