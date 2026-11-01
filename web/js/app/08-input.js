import { shared } from './state.js';
import { $, $$, app, consoleBox, feed, viewport, webrtcFeed } from './00-core.js';
import { getSetting } from './00-settings.js';
import { t, tr } from './01-i18n.js';
import { invalidateSurfaceBox } from './02-theme-layout.js';
import { request, toast } from './03-session.js';
import { updateDiagnosticsCard } from './05-power.js';
import { shouldVideoRun, startVideo, stopVideo } from './07-video.js';
import { isPipActive } from './07a-pip.js';
import { recordMacroKey, renderKeyboardMacroMenu } from './07z-keyboard-macros.js';
import { refreshStatus } from './10-workspace-status.js';

let panStartX = 0;
let panStartY = 0;
let panScrollX = 0;
let panScrollY = 0;
let remoteWanted = false;
let mouseBusy = false;
let mouseTimer = 0;
let lastMouseSend = 0;

let lastHidToast = 0;
// Only a write timeout (HTTP 504, e.g. target powered off) is noisy enough to
// optionally silence; "not configured"/"not ready" failures always surface.
function handleHidError(error) {
  // A dropped input socket already toasted once for its whole burst.
  if (error?.reported) return;
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
  return remoteWanted && shared.pageActive && shared.videoWorkspace === 'video' && !app.classList.contains('hidden');
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
    if (!e.reported) toast(e.message, true);
  }
}

// Paste-as-typing: turn arbitrary text into physical HID keystrokes for the
// target machine's keyboard layout. The target OS decides which character a
// physical key produces, so each layout needs its own character -> key map.
// Kept pure so the mapping can be exercised without touching the network or
// DOM (and exposed to the UI verification harness at the end of this block).
const TARGET_LAYOUTS = ['us', 'uk', 'de', 'fr', 'jp'];
const TARGET_LAYOUT_LABELS = {
  us: '美式（US QWERTY）',
  uk: '英式（UK）',
  de: '德语（DE QWERTZ）',
  fr: '法语（FR AZERTY）',
  jp: '日语（JP JIS）',
};
const QWERTY_LETTERS = {
  a: 'KeyA',
  b: 'KeyB',
  c: 'KeyC',
  d: 'KeyD',
  e: 'KeyE',
  f: 'KeyF',
  g: 'KeyG',
  h: 'KeyH',
  i: 'KeyI',
  j: 'KeyJ',
  k: 'KeyK',
  l: 'KeyL',
  m: 'KeyM',
  n: 'KeyN',
  o: 'KeyO',
  p: 'KeyP',
  q: 'KeyQ',
  r: 'KeyR',
  s: 'KeyS',
  t: 'KeyT',
  u: 'KeyU',
  v: 'KeyV',
  w: 'KeyW',
  x: 'KeyX',
  y: 'KeyY',
  z: 'KeyZ',
};
// QWERTZ swaps the physical Y/Z keys; AZERTY moves the letter rows around.
const DE_LETTERS = { ...QWERTY_LETTERS, y: 'KeyZ', z: 'KeyY' };
const FR_LETTERS = {
  ...QWERTY_LETTERS,
  a: 'KeyQ',
  q: 'KeyA',
  w: 'KeyZ',
  z: 'KeyW',
  m: 'Semicolon',
};
const plainKey = (key) => ({ key });
const shiftedKey = (key) => ({ key, shift: true });
// AltGr is sent as Ctrl+Alt. Windows treats that as AltGr for layout input,
// and Linux does too; a macOS target uses Option, so AltGr-only symbols (for
// example @ on a German layout) may not come out right there.
const altGrKey = (key) => ({ key, altgr: true });
const US_SYMBOLS = {
  '`': plainKey('Backquote'),
  '-': plainKey('Minus'),
  '=': plainKey('Equal'),
  '[': plainKey('BracketLeft'),
  ']': plainKey('BracketRight'),
  '\\': plainKey('Backslash'),
  ';': plainKey('Semicolon'),
  "'": plainKey('Quote'),
  ',': plainKey('Comma'),
  '.': plainKey('Period'),
  '/': plainKey('Slash'),
  '~': shiftedKey('Backquote'),
  '!': shiftedKey('Digit1'),
  '@': shiftedKey('Digit2'),
  '#': shiftedKey('Digit3'),
  $: shiftedKey('Digit4'),
  '%': shiftedKey('Digit5'),
  '^': shiftedKey('Digit6'),
  '&': shiftedKey('Digit7'),
  '*': shiftedKey('Digit8'),
  '(': shiftedKey('Digit9'),
  ')': shiftedKey('Digit0'),
  _: shiftedKey('Minus'),
  '+': shiftedKey('Equal'),
  '{': shiftedKey('BracketLeft'),
  '}': shiftedKey('BracketRight'),
  '|': shiftedKey('Backslash'),
  ':': shiftedKey('Semicolon'),
  '"': shiftedKey('Quote'),
  '<': shiftedKey('Comma'),
  '>': shiftedKey('Period'),
  '?': shiftedKey('Slash'),
};
const UK_SYMBOLS = {
  '`': plainKey('Backquote'),
  '-': plainKey('Minus'),
  '=': plainKey('Equal'),
  '[': plainKey('BracketLeft'),
  ']': plainKey('BracketRight'),
  '#': plainKey('Backslash'),
  ';': plainKey('Semicolon'),
  "'": plainKey('Quote'),
  ',': plainKey('Comma'),
  '.': plainKey('Period'),
  '/': plainKey('Slash'),
  '\\': plainKey('IntlBackslash'),
  '!': shiftedKey('Digit1'),
  '"': shiftedKey('Digit2'),
  '£': shiftedKey('Digit3'),
  $: shiftedKey('Digit4'),
  '%': shiftedKey('Digit5'),
  '^': shiftedKey('Digit6'),
  '&': shiftedKey('Digit7'),
  '*': shiftedKey('Digit8'),
  '(': shiftedKey('Digit9'),
  ')': shiftedKey('Digit0'),
  _: shiftedKey('Minus'),
  '+': shiftedKey('Equal'),
  '{': shiftedKey('BracketLeft'),
  '}': shiftedKey('BracketRight'),
  '~': shiftedKey('Backslash'),
  ':': shiftedKey('Semicolon'),
  '@': shiftedKey('Quote'),
  '<': shiftedKey('Comma'),
  '>': shiftedKey('Period'),
  '?': shiftedKey('Slash'),
  '|': shiftedKey('IntlBackslash'),
};
const DE_SYMBOLS = {
  'ß': plainKey('Minus'),
  ü: plainKey('BracketLeft'),
  '+': plainKey('BracketRight'),
  ö: plainKey('Semicolon'),
  ä: plainKey('Quote'),
  '#': plainKey('Backslash'),
  '<': plainKey('IntlBackslash'),
  ',': plainKey('Comma'),
  '.': plainKey('Period'),
  '-': plainKey('Slash'),
  '°': shiftedKey('Backquote'),
  '?': shiftedKey('Minus'),
  '!': shiftedKey('Digit1'),
  '"': shiftedKey('Digit2'),
  '§': shiftedKey('Digit3'),
  $: shiftedKey('Digit4'),
  '%': shiftedKey('Digit5'),
  '&': shiftedKey('Digit6'),
  '/': shiftedKey('Digit7'),
  '(': shiftedKey('Digit8'),
  ')': shiftedKey('Digit9'),
  '=': shiftedKey('Digit0'),
  'Ü': shiftedKey('BracketLeft'),
  '*': shiftedKey('BracketRight'),
  'Ö': shiftedKey('Semicolon'),
  'Ä': shiftedKey('Quote'),
  "'": shiftedKey('Backslash'),
  '>': shiftedKey('IntlBackslash'),
  ';': shiftedKey('Comma'),
  ':': shiftedKey('Period'),
  _: shiftedKey('Slash'),
  '@': altGrKey('KeyQ'),
  '€': altGrKey('KeyE'),
  '²': altGrKey('Digit2'),
  '³': altGrKey('Digit3'),
  '{': altGrKey('Digit7'),
  '[': altGrKey('Digit8'),
  ']': altGrKey('Digit9'),
  '}': altGrKey('Digit0'),
  '~': altGrKey('BracketRight'),
  '\\': altGrKey('Minus'),
  '|': altGrKey('IntlBackslash'),
  µ: altGrKey('KeyM'),
};
// "~" (AltGr+2) and "`" (AltGr+7) are dead keys on Windows French layouts:
// typed mid-text they would merge with the next character, so they are
// deliberately unsupported here and reported before sending.
const FR_SYMBOLS = {
  '²': plainKey('Backquote'),
  '&': plainKey('Digit1'),
  é: plainKey('Digit2'),
  '"': plainKey('Digit3'),
  "'": plainKey('Digit4'),
  '(': plainKey('Digit5'),
  '-': plainKey('Digit6'),
  è: plainKey('Digit7'),
  _: plainKey('Digit8'),
  ç: plainKey('Digit9'),
  à: plainKey('Digit0'),
  ')': plainKey('Minus'),
  '=': plainKey('Equal'),
  $: plainKey('BracketRight'),
  '*': plainKey('Backslash'),
  'ù': plainKey('Quote'),
  '<': plainKey('IntlBackslash'),
  ',': plainKey('KeyM'),
  ';': plainKey('Comma'),
  ':': plainKey('Period'),
  '!': plainKey('Slash'),
  '£': shiftedKey('BracketRight'),
  'µ': shiftedKey('Backslash'),
  '%': shiftedKey('Quote'),
  '>': shiftedKey('IntlBackslash'),
  '?': shiftedKey('KeyM'),
  '.': shiftedKey('Comma'),
  '/': shiftedKey('Period'),
  '§': shiftedKey('Slash'),
  '+': shiftedKey('Equal'),
  '@': altGrKey('Digit0'),
  '€': altGrKey('KeyE'),
  '#': altGrKey('Digit3'),
  '{': altGrKey('Digit4'),
  '[': altGrKey('Digit5'),
  '|': altGrKey('Digit6'),
  '\\': altGrKey('Digit8'),
  '^': altGrKey('Digit9'),
  ']': altGrKey('Minus'),
  '}': altGrKey('Equal'),
  '¤': altGrKey('BracketRight'),
};
const JP_SYMBOLS = {
  '@': plainKey('BracketLeft'),
  '[': plainKey('BracketRight'),
  ';': plainKey('Semicolon'),
  ':': plainKey('Quote'),
  ']': plainKey('Backslash'),
  '\\': plainKey('IntlRo'),
  '¥': plainKey('IntlYen'),
  '-': plainKey('Minus'),
  '^': plainKey('Equal'),
  ',': plainKey('Comma'),
  '.': plainKey('Period'),
  '/': plainKey('Slash'),
  '`': shiftedKey('BracketLeft'),
  '{': shiftedKey('BracketRight'),
  '+': shiftedKey('Semicolon'),
  '*': shiftedKey('Quote'),
  '}': shiftedKey('Backslash'),
  _: shiftedKey('IntlRo'),
  '|': shiftedKey('IntlYen'),
  '=': shiftedKey('Minus'),
  '~': shiftedKey('Equal'),
  '!': shiftedKey('Digit1'),
  '"': shiftedKey('Digit2'),
  '#': shiftedKey('Digit3'),
  $: shiftedKey('Digit4'),
  '%': shiftedKey('Digit5'),
  '&': shiftedKey('Digit6'),
  "'": shiftedKey('Digit7'),
  '(': shiftedKey('Digit8'),
  ')': shiftedKey('Digit9'),
  '<': shiftedKey('Comma'),
  '>': shiftedKey('Period'),
  '?': shiftedKey('Slash'),
};
function buildLayout({ letters, symbols, digitsShifted = false }) {
  const map = Object.create(null);
  const put = (ch, key, extra = {}) => {
    map[ch] = { key, shift: false, altgr: false, letter: false, ...extra };
  };
  for (const [ch, key] of Object.entries(letters)) {
    put(ch, key, { letter: true });
    put(ch.toUpperCase(), key, { shift: true, letter: true });
  }
  for (let digit = 0; digit <= 9; digit += 1)
    put(String(digit), `Digit${digit}`, { shift: digitsShifted });
  put(' ', 'Space');
  put('\n', 'Enter');
  put('\t', 'Tab');
  for (const [ch, spec] of Object.entries(symbols)) put(ch, spec.key, spec);
  return map;
}
const LAYOUT_MAPS = {
  us: buildLayout({ letters: QWERTY_LETTERS, symbols: US_SYMBOLS }),
  uk: buildLayout({ letters: QWERTY_LETTERS, symbols: UK_SYMBOLS }),
  de: buildLayout({ letters: DE_LETTERS, symbols: DE_SYMBOLS }),
  fr: buildLayout({ letters: FR_LETTERS, symbols: FR_SYMBOLS, digitsShifted: true }),
  jp: buildLayout({ letters: QWERTY_LETTERS, symbols: JP_SYMBOLS }),
};
function targetLayout() {
  const value = getSetting('target_layout', 'us');
  return TARGET_LAYOUTS.includes(value) ? value : 'us';
}
function targetLayoutLabel(layout = targetLayout()) {
  return tr(TARGET_LAYOUT_LABELS[layout] || TARGET_LAYOUT_LABELS.us);
}
function textToKeystrokes(text, layout = targetLayout()) {
  const map = LAYOUT_MAPS[TARGET_LAYOUTS.includes(layout) ? layout : 'us'],
    strokes = [],
    unsupported = [],
    seenUnsupported = new Set(),
    normalized = String(text ?? '').replace(/\r\n?/g, '\n');
  for (const ch of Array.from(normalized)) {
    const entry = map[ch];
    if (!entry) {
      if (!seenUnsupported.has(ch)) {
        seenUnsupported.add(ch);
        unsupported.push(ch);
      }
      continue;
    }
    strokes.push({
      key: entry.key,
      shift: !!entry.shift,
      ctrl: !!entry.altgr,
      alt: !!entry.altgr,
      meta: false,
      letter: !!entry.letter,
    });
  }
  return { strokes, unsupported };
}
function updatePasteLayoutLabel() {
  const el = $('#paste-key-layout');
  if (el) el.textContent = targetLayoutLabel();
}
function displayUnsupportedChar(ch) {
  return /[\u0000-\u001f\u007f]/.test(ch)
    ? `U+${ch.codePointAt(0).toString(16).toUpperCase().padStart(4, '0')}`
    : ch;
}
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
  updatePasteLayoutLabel();
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
  // The latency tool toggles Caps Lock: typed letters would change case.
  if (shared.latencyRun) {
    const message = tr('正在测量延迟，请结束后再发送');
    setPasteStatus(message, 'error');
    toast(message, true);
    return;
  }
  const input = $('#paste-key-text'),
    unsupportedEl = $('#paste-key-unsupported'),
    sendButton = $('#paste-key-send'),
    cancelButton = $('#paste-key-cancel');
  const { strokes, unsupported } = textToKeystrokes(input ? input.value : '', targetLayout());
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
  shared.pasteRun = {
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
        shiftKey: stroke.shift !== (capsLock && stroke.letter),
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
    if (!error.reported) toast(error.message, true);
  } finally {
    shared.pasteRun = null;
    if (sendButton) sendButton.disabled = false;
    if (cancelButton) cancelButton.textContent = tr('取消');
  }
}

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
    if (!error.reported) toast(error.message, true);
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
  renderKeyboardMacroMenu(menu);
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
      'IntlBackslash',
      'IntlRo',
      'IntlYen',
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
// Background releases (blur, logout, pause) are best effort; the explicit
// "release keys" button passes `strict` so the user learns if it failed.
function releaseAll({ strict = false } = {}) {
  shared.mouseX = shared.mouseY = 0;
  shared.absolutePending = null;
  resetVirtualModifiers();
  if (app.classList.contains('hidden') || shared.bootstrap.authenticated === false) return Promise.resolve();
  const released = request('/api/input/release-all', { method: 'POST', keepalive: true });
  return strict ? released : released.catch(() => {});
}
function stopInput() {
  shared.pageActive = false;
  remoteWanted = false;
  resetVirtualModifiers();
  if (relativeCaptured()) document.exitPointerLock();
  syncInputUi();
  return releaseAll();
}
function suspendInput() {
  shared.pageActive = false;
  if (relativeCaptured()) document.exitPointerLock();
  syncInputUi();
  return releaseAll();
}
function resumeInput() {
  shared.pageActive = !document.hidden && document.hasFocus();
  syncInputUi();
  if (inputEnabled()) scheduleMouse();
}
function handleVideoVisibility() {
  if (shouldVideoRun()) {
    startVideo();
    refreshStatus();
  } else if (document.hidden || shared.videoWorkspace !== 'video' || app.classList.contains('hidden')) stopVideo();
}

function absoluteMode() {
  return shared.bootstrap.capabilities?.pointer_mode === 'absolute';
}
function activeVideoSurface() {
  return shared.activeVideoTransport === 'webrtc_h264' ? webrtcFeed : feed;
}
function mapAbsolute(clientX, clientY) {
  const surface = activeVideoSurface(),
    sourceWidth = surface instanceof HTMLVideoElement ? surface.videoWidth : surface.naturalWidth,
    sourceHeight = surface instanceof HTMLVideoElement ? surface.videoHeight : surface.naturalHeight;
  if (!sourceWidth || !sourceHeight) return null;
  if (!shared.cachedSurfaceBox) shared.cachedSurfaceBox = surface.getBoundingClientRect();
  const box = shared.cachedSurfaceBox;
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
function stopPanning(event) {
  if (shared.isDraggingPan) {
    shared.isDraggingPan = false;
    viewport.classList.remove('panning-active');
    try {
      if (event?.pointerId) viewport.releasePointerCapture?.(event.pointerId);
    } catch (_) {}
    invalidateSurfaceBox();
  }
}
function scheduleMouse() {
  const pending = absoluteMode() ? shared.absolutePending : shared.mouseX || shared.mouseY;
  if (mouseBusy || mouseTimer || !pending) return;
  const wait = Math.max(0, 16 - (performance.now() - lastMouseSend));
  mouseTimer = setTimeout(flushMouse, wait);
}
async function flushMouse() {
  mouseTimer = 0;
  if (mouseBusy || !inputEnabled()) return;
  let url, body;
  if (absoluteMode()) {
    if (!shared.absolutePending) return;
    const point = shared.absolutePending;
    shared.absolutePending = null;
    url = '/api/mouse/absolute';
    body = { action: 'move', ...point };
  } else {
    const dx = Math.max(-127, Math.min(127, shared.mouseX)),
      dy = Math.max(-127, Math.min(127, shared.mouseY));
    if (!dx && !dy) return;
    shared.mouseX -= dx;
    shared.mouseY -= dy;
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

// DOM wiring; main.js calls this once, in the original fragment order.
export function init() {
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
  $$('[data-quick-key]').forEach((button) =>
    button.addEventListener('click', () => sendQuickKey(button.dataset.quickKey)),
  );
  // The verification harness calls the pure mapper directly to assert every
  // layout table without a network round-trip. Only exposed when the harness
  // set window.__wingmanTestHooks before the page loaded.
  if (window.__wingmanTestHooks === true)
    window.__wingmanTest = Object.assign(window.__wingmanTest || {}, {
      textToKeystrokes,
      targetLayout,
      targetLayouts: TARGET_LAYOUTS.slice(),
    });
  $('#paste-typing-btn')?.addEventListener('click', openPasteDialog);
  $$('[data-paste-close]').forEach((button) => button.addEventListener('click', closePasteDialog));
  $('#paste-key-send')?.addEventListener('click', startPasteTyping);
  $('#paste-key-cancel')?.addEventListener('click', () => {
    if (shared.pasteRun) {
      shared.pasteRun.cancel();
      setPasteStatus(tr('正在取消…'));
      return;
    }
    closePasteDialog();
  });
  $('#paste-key-dialog')?.addEventListener('click', (event) => {
    if (event.target === event.currentTarget) closePasteDialog();
  });
  $('#paste-key-dialog')?.addEventListener('close', () => {
    shared.pasteRun?.cancel();
    shared.pasteRun = null;
  });
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
    // Hook the forwarding path itself: local shortcuts and keys typed into
    // local controls have already returned above, so only keys that really
    // reach the target are recorded.
    recordMacroKey(key, event);
    sendKey(key, event).catch(handleHidError);
  });
  $$('.key-strip [data-key]').forEach((button) =>
    button.addEventListener('click', () => sendKey(button.dataset.key).catch(handleHidError)),
  );
  document.addEventListener('visibilitychange', () => {
    if (document.hidden) {
      suspendInput();
      if (!isPipActive()) stopVideo();
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
  document.addEventListener('pointerlockchange', () => {
    if (!relativeCaptured()) shared.mouseX = shared.mouseY = 0;
    syncInputUi();
  });
  document.addEventListener('pointerlockerror', () => toast('浏览器未允许捕获鼠标', true));
  viewport.addEventListener('pointermove', (event) => {
    if (shared.isDraggingPan) {
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
      shared.absolutePending = point;
      if (!point) return;
    } else {
      if (!relativeCaptured()) return;
      shared.mouseX += event.movementX;
      shared.mouseY += event.movementY;
    }
    scheduleMouse();
  });
  viewport.addEventListener('pointerdown', (event) => {
    if (event.button > 2) return;
    if (
      consoleBox.classList.contains('mode-native') &&
      (shared.isPanActive || event.button === 1 || event.altKey || !inputEnabled())
    ) {
      event.preventDefault();
      event.stopPropagation();
      shared.isDraggingPan = true;
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
      shared.absolutePending = point;
      if (!point) return;
      request('/api/mouse/absolute', {
        method: 'POST',
        body: JSON.stringify({ action: 'click', ...point, button }),
      }).catch(handleHidError);
    } else
      request('/api/mouse/click', { method: 'POST', body: JSON.stringify({ button }) }).catch(handleHidError);
  });
  viewport.addEventListener('pointerup', stopPanning);
  viewport.addEventListener('pointercancel', stopPanning);
  viewport.addEventListener('contextmenu', (event) => {
    if (inputEnabled() || shared.isPanActive) event.preventDefault();
  });
  viewport.addEventListener(
    'wheel',
    (event) => {
      if (consoleBox.classList.contains('mode-native') && (shared.isPanActive || event.altKey || !inputEnabled())) {
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
        shared.absolutePending = point;
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
}

export {
  absoluteMode, activeModifiers, activeVideoSurface, closeQuickMore, handleHidError, inputEnabled,
  interactiveTarget, mapAbsolute, relativeCaptured, renderKeyboardLeds, renderQuickCombos,
  resetVirtualModifiers, scheduleMouse, sendKey, setRemote, stopInput, syncInputUi, targetLayout,
  updatePasteLayoutLabel,
};
