import { shared } from './state.js';
import { $, $$, viewport } from './00-core.js';
import { tr } from './01-i18n.js';
import { toast } from './03-session.js';
import {
  activeModifiers, closeQuickMore, inputEnabled, renderQuickCombos, resetVirtualModifiers, sendKey, setRemote,
} from './08-input.js';

// Keyboard macros: record keys that were actually forwarded to the target,
// replay them with their timing, and keep the list in localStorage. Only HID
// key names, modifier flags and delays are stored; no typed text is kept.
// This fragment loads before 08-input.js so its state is initialized before
// the initial renderQuickCombos() call there.
const KEYBOARD_MACROS_KEY = 'wingman_keyboard_macros';
const KEYBOARD_MACRO_SPEED_KEY = 'wingman_keyboard_macro_speed';
const MACRO_MAX_COUNT = 50;
const MACRO_MAX_STEPS = 2000;
const MACRO_MAX_DELAY_MS = 5000;
const MACRO_SPEEDS = [0.5, 1, 2];
const MACRO_NAMED_KEYS = new Set([
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
  'NumLock',
  'ScrollLock',
  'PrintScreen',
  'Pause',
  'ContextMenu',
]);
const MACRO_STEP_FIELDS = ['key', 'ctrl', 'shift', 'alt', 'meta', 'delay_ms'];
const MACRO_FIELDS = ['name', 'steps'];
const MACRO_FILE_FIELDS = ['version', 'macros'];
let macroRecording = null;
let macroPendingSteps = [];
let macroPlaybackSpeed = readMacroPlaybackSpeed();

function macroKeyNameValid(key) {
  return (
    typeof key === 'string' &&
    (/^Key[A-Z]$/.test(key) ||
      /^Digit[0-9]$/.test(key) ||
      /^F(?:[1-9]|1[0-2])$/.test(key) ||
      /^Numpad(?:[0-9]|Add|Subtract|Multiply|Divide|Decimal|Enter)$/.test(key) ||
      MACRO_NAMED_KEYS.has(key))
  );
}
function readMacroPlaybackSpeed() {
  try {
    const value = Number(localStorage.getItem(KEYBOARD_MACRO_SPEED_KEY));
    return MACRO_SPEEDS.includes(value) ? value : 1;
  } catch (_) {
    return 1;
  }
}
function writeMacroPlaybackSpeed(speed) {
  try {
    localStorage.setItem(KEYBOARD_MACRO_SPEED_KEY, String(speed));
  } catch (_) {}
}
function newMacroId() {
  return `macro_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 6)}`;
}
function macroStepFromStored(step) {
  if (!step || typeof step !== 'object' || !macroKeyNameValid(step.key)) return null;
  const delay = Number(step.delay_ms);
  return {
    key: step.key,
    ctrl: !!step.ctrl,
    shift: !!step.shift,
    alt: !!step.alt,
    meta: !!step.meta,
    delay_ms: Number.isFinite(delay) ? Math.max(0, Math.min(MACRO_MAX_DELAY_MS, Math.round(delay))) : 0,
  };
}
// Stored data belongs to this browser, so it is repaired where possible
// instead of being rejected; imported data is validated strictly below.
function readKeyboardMacros() {
  try {
    const raw = JSON.parse(localStorage.getItem(KEYBOARD_MACROS_KEY) || '[]');
    if (!Array.isArray(raw)) return [];
    return raw
      .slice(0, MACRO_MAX_COUNT)
      .map((macro, index) => {
        if (!macro || typeof macro !== 'object' || !Array.isArray(macro.steps)) return null;
        const steps = macro.steps
          .slice(0, MACRO_MAX_STEPS)
          .map(macroStepFromStored)
          .filter(Boolean);
        if (!steps.length) return null;
        const name = typeof macro.name === 'string' ? macro.name.trim().slice(0, 80) : '';
        return {
          id: typeof macro.id === 'string' && macro.id ? macro.id : `macro_restored_${index}`,
          name: name || `宏 ${index + 1}`,
          steps,
        };
      })
      .filter(Boolean);
  } catch (_) {
    return [];
  }
}
function writeKeyboardMacros(macros) {
  if (!Array.isArray(macros) || macros.length > MACRO_MAX_COUNT) return false;
  try {
    localStorage.setItem(KEYBOARD_MACROS_KEY, JSON.stringify(macros));
    return true;
  } catch (_) {
    return false;
  }
}
function macroModifiers(event) {
  return {
    ctrl: !!event.ctrlKey || activeModifiers.has('Control'),
    shift: !!event.shiftKey || activeModifiers.has('Shift'),
    alt: !!event.altKey || activeModifiers.has('Alt'),
    meta: !!event.metaKey || activeModifiers.has('Meta'),
  };
}
function updateMacroRecordingIndicator() {
  const indicator = $('#macro-recording');
  if (!indicator) return;
  indicator.classList.toggle('hidden', !macroRecording);
  if (!macroRecording) indicator.removeAttribute('title');
  else indicator.title = tr(`已录制 ${macroRecording.steps.length} 步`);
}
function startKeyboardMacroRecording() {
  if (macroRecording) return;
  if (shared.macroPlayback) {
    toast(tr('正在回放宏，无法开始录制'), true);
    return;
  }
  closeQuickMore();
  if (!inputEnabled()) setRemote(true);
  macroRecording = { steps: [], lastAt: 0, warnedFull: false };
  updateMacroRecordingIndicator();
  // Menu buttons keep focus when the menu closes; without moving focus to
  // the viewport the first physical key would count as local UI input.
  viewport.focus({ preventScroll: true });
  toast(tr('已开始录制宏，点击顶栏“停止”结束'));
}
// Called from the document keydown forwarding path, after local shortcuts
// and local input targets have already been excluded. Only keys that are
// actually sent to the target reach this point.
function recordMacroKey(key, event) {
  if (!macroRecording) return;
  if (event?.target?.closest?.('input,textarea,select,[contenteditable="true"]')) return;
  if (macroRecording.steps.length >= MACRO_MAX_STEPS) {
    if (!macroRecording.warnedFull) {
      macroRecording.warnedFull = true;
      toast(tr(`录制已达到 ${MACRO_MAX_STEPS} 步上限`), true);
    }
    return;
  }
  const now = performance.now();
  const delay = macroRecording.steps.length
    ? Math.min(MACRO_MAX_DELAY_MS, Math.max(0, Math.round(now - macroRecording.lastAt)))
    : 0;
  macroRecording.lastAt = now;
  macroRecording.steps.push({ key, ...macroModifiers(event), delay_ms: delay });
  updateMacroRecordingIndicator();
}
function stopKeyboardMacroRecording() {
  if (!macroRecording) return;
  const steps = macroRecording.steps;
  macroRecording = null;
  updateMacroRecordingIndicator();
  if (!steps.length) {
    toast(tr('没有录制到按键'), true);
    return;
  }
  openMacroSaveDialog(steps);
}
function openMacroSaveDialog(steps) {
  const dialog = $('#macro-save-dialog'),
    input = $('#macro-save-name'),
    summary = $('#macro-save-summary');
  macroPendingSteps = steps;
  if (summary) summary.textContent = tr(`已录制 ${steps.length} 个按键`);
  if (input) input.value = `${tr('宏')} ${readKeyboardMacros().length + 1}`;
  try {
    if (typeof dialog?.showModal === 'function') dialog.showModal();
    else dialog?.show?.();
  } catch (_) {
    try {
      dialog?.show?.();
    } catch (__) {}
  }
  setTimeout(() => {
    input?.focus();
    input?.select();
  }, 0);
}
function closeMacroSaveDialog() {
  const dialog = $('#macro-save-dialog');
  try {
    dialog?.close?.();
  } catch (_) {}
}
function saveRecordedMacro() {
  const steps = macroPendingSteps;
  if (!steps.length) return;
  const input = $('#macro-save-name');
  const name = (input?.value || '').trim().slice(0, 80) || `${tr('宏')} ${readKeyboardMacros().length + 1}`;
  const macros = readKeyboardMacros();
  if (macros.length >= MACRO_MAX_COUNT) {
    toast(tr(`键盘宏最多保存 ${MACRO_MAX_COUNT} 个`), true);
    return;
  }
  macros.push({
    id: newMacroId(),
    name,
    steps: steps.map((step) => ({ ...step })),
  });
  if (!writeKeyboardMacros(macros)) {
    toast(tr('无法保存键盘宏'), true);
    return;
  }
  macroPendingSteps = [];
  closeMacroSaveDialog();
  renderQuickCombos();
  renderMacroManager();
  toast(tr(`已保存宏「${name}」`));
}

function setMacroPlaybackSpeed(speed) {
  const value = Number(speed);
  if (!MACRO_SPEEDS.includes(value)) return;
  macroPlaybackSpeed = value;
  writeMacroPlaybackSpeed(value);
  if (shared.macroPlayback) shared.macroPlayback.speed = value;
  updateMacroSpeedButtons();
}
function updateMacroSpeedButtons() {
  $$('[data-macro-speed]').forEach((button) => {
    const active = Number(button.dataset.macroSpeed) === macroPlaybackSpeed;
    button.classList.toggle('active', active);
    button.setAttribute('aria-pressed', String(active));
  });
  const select = $('#macro-play-speed');
  if (select && select.value !== String(macroPlaybackSpeed)) select.value = String(macroPlaybackSpeed);
}
function macroMenuSection() {
  const fragment = document.createDocumentFragment();
  const separator = document.createElement('div');
  separator.className = 'quick-more-sep';
  fragment.append(separator);
  const title = document.createElement('div');
  title.className = 'quick-more-title';
  title.textContent = tr('键盘宏');
  fragment.append(title);

  const macros = readKeyboardMacros();
  macros.forEach((macro) => {
    const row = document.createElement('div');
    row.className = 'quick-more-row';
    const play = document.createElement('button');
    play.type = 'button';
    play.className = 'quick-combo-item';
    play.setAttribute('role', 'menuitem');
    play.dataset.macroPlay = macro.id;
    play.textContent = macro.name;
    play.title = tr(`${macro.steps.length} 步`);
    play.addEventListener('click', () => {
      closeQuickMore();
      startKeyboardMacro(macro);
    });
    const remove = document.createElement('button');
    remove.type = 'button';
    remove.className = 'quick-combo-delete';
    remove.dataset.removeMacro = macro.id;
    remove.title = tr('删除此宏');
    remove.setAttribute('aria-label', tr('删除此宏'));
    remove.textContent = '×';
    remove.addEventListener('click', (event) => {
      event.stopPropagation();
      deleteKeyboardMacro(macro.id);
    });
    row.append(play, remove);
    fragment.append(row);
  });

  const speed = document.createElement('div');
  speed.className = 'macro-speed-row';
  speed.setAttribute('role', 'group');
  speed.setAttribute('aria-label', tr('回放速度'));
  const speedLabel = document.createElement('span');
  speedLabel.className = 'macro-speed-label';
  speedLabel.textContent = tr('回放速度');
  speed.append(speedLabel);
  MACRO_SPEEDS.forEach((value) => {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'macro-speed-btn';
    button.dataset.macroSpeed = String(value);
    button.textContent = `${value}×`;
    button.setAttribute('aria-pressed', String(value === macroPlaybackSpeed));
    button.classList.toggle('active', value === macroPlaybackSpeed);
    button.addEventListener('click', (event) => {
      event.stopPropagation();
      setMacroPlaybackSpeed(value);
    });
    speed.append(button);
  });
  fragment.append(speed);

  const record = document.createElement('button');
  record.type = 'button';
  record.id = 'quick-macro-record';
  record.className = 'quick-more-action';
  record.setAttribute('role', 'menuitem');
  record.textContent = tr('录制宏…');
  record.addEventListener('click', (event) => {
    event.stopPropagation();
    startKeyboardMacroRecording();
  });
  fragment.append(record);

  const manage = document.createElement('button');
  manage.type = 'button';
  manage.id = 'quick-macro-manage';
  manage.className = 'quick-more-action';
  manage.setAttribute('role', 'menuitem');
  manage.textContent = tr('管理宏…');
  manage.addEventListener('click', (event) => {
    event.stopPropagation();
    openMacroManager();
  });
  fragment.append(manage);
  return fragment;
}
function renderKeyboardMacroMenu(menu) {
  if (menu) menu.append(macroMenuSection());
}

function deleteKeyboardMacro(id) {
  const macros = readKeyboardMacros();
  const remaining = macros.filter((macro) => macro.id !== id);
  if (remaining.length === macros.length) return;
  if (!writeKeyboardMacros(remaining)) {
    toast(tr('无法保存键盘宏'), true);
    return;
  }
  renderQuickCombos();
  renderMacroManager();
  toast(tr('已删除宏'));
}
function openMacroManager() {
  closeQuickMore();
  renderMacroManager();
  const dialog = $('#macro-manager-dialog');
  try {
    if (typeof dialog?.showModal === 'function') dialog.showModal();
    else dialog?.show?.();
  } catch (_) {
    try {
      dialog?.show?.();
    } catch (__) {}
  }
}
function closeMacroManager() {
  const dialog = $('#macro-manager-dialog');
  try {
    dialog?.close?.();
  } catch (_) {}
}
function macroManagerButton(label, action, id) {
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'secondary macro-row-action';
  button.textContent = tr(label);
  button.dataset[action] = id;
  return button;
}
function renderMacroManager() {
  const list = $('#macro-manager-list');
  if (!list) return;
  const macros = readKeyboardMacros();
  list.textContent = '';
  if (!macros.length) {
    const empty = document.createElement('p');
    empty.className = 'hint';
    empty.textContent = tr('还没有保存的宏');
    list.append(empty);
    return;
  }
  macros.forEach((macro) => {
    const row = document.createElement('div');
    row.className = 'macro-manager-row';
    row.dataset.macroRow = macro.id;
    const name = document.createElement('input');
    name.className = 'macro-name-input';
    name.value = macro.name;
    name.maxLength = 80;
    name.setAttribute('aria-label', tr('宏名称'));
    const meta = document.createElement('span');
    meta.className = 'macro-row-meta';
    meta.textContent = tr(`${macro.steps.length} 步`);
    const play = macroManagerButton('回放', 'macroPlay', macro.id);
    const rename = macroManagerButton('重命名', 'macroRename', macro.id);
    const exportButton = macroManagerButton('导出', 'macroExport', macro.id);
    const remove = macroManagerButton('删除', 'macroDelete', macro.id);
    remove.classList.add('danger');
    row.append(name, meta, play, rename, exportButton, remove);
    list.append(row);
  });
}
function renameKeyboardMacro(id) {
  const row = $$('#macro-manager-list [data-macro-row]').find((item) => item.dataset.macroRow === id);
  const input = row?.querySelector('.macro-name-input');
  const name = (input?.value || '').trim().slice(0, 80);
  if (!name) {
    toast(tr('宏名称不能为空'), true);
    return;
  }
  const macros = readKeyboardMacros();
  const macro = macros.find((item) => item.id === id);
  if (!macro) return;
  macro.name = name;
  if (!writeKeyboardMacros(macros)) {
    toast(tr('无法保存键盘宏'), true);
    return;
  }
  renderQuickCombos();
  renderMacroManager();
  toast(tr('已重命名宏'));
}
function exportMacroDocument(macros) {
  return {
    version: 1,
    macros: macros.map((macro) => ({
      name: macro.name,
      steps: macro.steps.map((step) => ({
        key: step.key,
        ctrl: !!step.ctrl,
        shift: !!step.shift,
        alt: !!step.alt,
        meta: !!step.meta,
        delay_ms: step.delay_ms,
      })),
    })),
  };
}
function macroFilename(name) {
  return (
    String(name)
      .replace(/[\\/:*?"<>|\u0000-\u001f]+/g, '_')
      .trim()
      .slice(0, 60) || 'macro'
  );
}
function downloadMacroJson(macros, filename) {
  const body = JSON.stringify(exportMacroDocument(macros), null, 2);
  const blob = new Blob([body], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = filename;
  document.body.append(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
function exportOneMacro(id) {
  const macro = readKeyboardMacros().find((item) => item.id === id);
  if (!macro) return;
  downloadMacroJson([macro], `${macroFilename(macro.name)}.wingman-macro.json`);
  toast(tr('已导出宏'));
}
function exportAllMacros() {
  const macros = readKeyboardMacros();
  if (!macros.length) {
    toast(tr('还没有保存的宏'), true);
    return;
  }
  downloadMacroJson(macros, 'wingman-keyboard-macros.json');
  toast(tr(`已导出 ${macros.length} 个宏`));
}
function macroImportError(message) {
  throw new Error(tr(`导入失败：${message}`));
}
function validateMacroFile(raw) {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) {
    macroImportError('宏文件格式无效');
  }
  for (const field of Object.keys(raw)) {
    if (!MACRO_FILE_FIELDS.includes(field)) macroImportError(`未知字段：${field}`);
  }
  if (raw.version !== 1) macroImportError('不支持的宏文件版本');
  if (!Array.isArray(raw.macros) || !raw.macros.length) macroImportError('宏文件不包含任何宏');
  if (raw.macros.length > MACRO_MAX_COUNT) {
    macroImportError(`一次最多导入 ${MACRO_MAX_COUNT} 个宏`);
  }
  return raw.macros.map((macro) => validateImportedMacro(macro));
}
function validateImportedMacro(raw) {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) {
    macroImportError('宏格式无效');
  }
  for (const field of Object.keys(raw)) {
    if (!MACRO_FIELDS.includes(field)) macroImportError(`未知字段：${field}`);
  }
  if (typeof raw.name !== 'string') macroImportError('宏名称必须是文本');
  const name = raw.name.trim();
  if (!name || name.length > 80) macroImportError('宏名称长度必须在 1 到 80 个字符之间');
  if (!Array.isArray(raw.steps) || !raw.steps.length) macroImportError('宏必须包含至少一个步骤');
  if (raw.steps.length > MACRO_MAX_STEPS) {
    macroImportError(`每个宏最多 ${MACRO_MAX_STEPS} 个步骤`);
  }
  return {
    id: newMacroId(),
    name,
    steps: raw.steps.map((step) => validateImportedStep(step)),
  };
}
function validateImportedStep(raw) {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) {
    macroImportError('步骤格式无效');
  }
  for (const field of Object.keys(raw)) {
    if (!MACRO_STEP_FIELDS.includes(field)) macroImportError(`未知字段：${field}`);
  }
  if (!macroKeyNameValid(raw.key)) macroImportError(`不支持的键名：${raw.key}`);
  for (const flag of ['ctrl', 'shift', 'alt', 'meta']) {
    if (typeof raw[flag] !== 'boolean') macroImportError(`修饰键 ${flag} 必须是布尔值`);
  }
  const delay = raw.delay_ms;
  if (
    typeof delay !== 'number' ||
    !Number.isFinite(delay) ||
    delay < 0 ||
    delay > MACRO_MAX_DELAY_MS ||
    Math.round(delay) !== delay
  ) {
    macroImportError(`延迟必须是 0 到 ${MACRO_MAX_DELAY_MS} 之间的整数毫秒`);
  }
  return {
    key: raw.key,
    ctrl: raw.ctrl,
    shift: raw.shift,
    alt: raw.alt,
    meta: raw.meta,
    delay_ms: delay,
  };
}
async function importMacroFile(input) {
  const file = input?.files?.[0];
  if (!file) return;
  try {
    const text = await file.text();
    let raw;
    try {
      raw = JSON.parse(text);
    } catch (_) {
      macroImportError('宏文件不是有效的 JSON');
    }
    const imported = validateMacroFile(raw);
    const existing = readKeyboardMacros();
    if (existing.length + imported.length > MACRO_MAX_COUNT) {
      macroImportError(`导入后会超过 ${MACRO_MAX_COUNT} 个宏`);
    }
    if (!writeKeyboardMacros([...existing, ...imported])) {
      macroImportError('无法写入本地存储');
    }
    renderQuickCombos();
    renderMacroManager();
    toast(tr(`已导入 ${imported.length} 个宏`));
  } catch (error) {
    toast(error.message, true);
  } finally {
    if (input) input.value = '';
  }
}

function openMacroPlayDialog(macro) {
  const dialog = $('#macro-play-dialog'),
    title = $('#macro-play-title');
  if (title) title.textContent = macro.name;
  updateMacroPlayProgress(0, macro.steps.length);
  setMacroPlayStatus('');
  updateMacroSpeedButtons();
  try {
    if (typeof dialog?.showModal === 'function') dialog.showModal();
    else dialog?.show?.();
  } catch (_) {
    try {
      dialog?.show?.();
    } catch (__) {}
  }
}
function closeMacroPlayDialog() {
  const dialog = $('#macro-play-dialog');
  try {
    dialog?.close?.();
  } catch (_) {}
}
function updateMacroPlayProgress(done, total) {
  const bar = $('#macro-play-bar'),
    progress = $('#macro-play-progress');
  if (bar) bar.style.width = total ? `${Math.round((done / total) * 100)}%` : '0%';
  if (progress) {
    progress.setAttribute('aria-valuemax', String(total));
    progress.setAttribute('aria-valuenow', String(done));
  }
}
function setMacroPlayStatus(text, tone = '') {
  const el = $('#macro-play-status');
  if (!el) return;
  el.textContent = text;
  el.classList.toggle('error', tone === 'error');
}
function macroWait(run, ms) {
  return new Promise((resolve) => {
    if (!(ms > 0) || run.canceled) return resolve();
    const deadline = performance.now() + ms;
    const tick = () => {
      run.wake = 0;
      if (run.canceled || shared.macroPlayback !== run || !inputEnabled() || performance.now() >= deadline)
        return resolve();
      run.wake = setTimeout(tick, Math.min(40, Math.max(1, deadline - performance.now())));
    };
    tick();
  });
}
function cancelMacroPlayback() {
  if (!shared.macroPlayback) return;
  // Leave the short wake timer alone: its next tick observes the flag and
  // resolves the pending delay instead of leaving the run suspended.
  shared.macroPlayback.canceled = true;
  setMacroPlayStatus(tr('正在取消…'));
}
function startKeyboardMacro(macro) {
  if (!macro || !Array.isArray(macro.steps) || !macro.steps.length) return;
  if (shared.macroPlayback) {
    toast(tr('已有宏正在回放'), true);
    return;
  }
  if (macroRecording) {
    toast(tr('请先停止录制再回放'), true);
    return;
  }
  // The latency tool toggles Caps Lock: replayed letters would change case.
  if (shared.latencyRun) {
    toast(tr('正在测量延迟，请结束后再回放'), true);
    return;
  }
  closeQuickMore();
  openMacroPlayDialog(macro);
  if (!inputEnabled()) setRemote(true);
  if (!inputEnabled()) {
    const message = tr('当前无法回放：请切换到远程画面并开启键鼠转发');
    setMacroPlayStatus(message, 'error');
    toast(message, true);
    return;
  }
  resetVirtualModifiers();
  const run = { macro, canceled: false, speed: macroPlaybackSpeed, wake: 0 };
  shared.macroPlayback = run;
  runKeyboardMacro(run).catch((error) => {
    setMacroPlayStatus(tr(`回放失败：${error.message}`), 'error');
    if (!error.reported) toast(error.message, true);
  });
}
async function runKeyboardMacro(run) {
  const total = run.macro.steps.length;
  let sent = 0,
    interrupted = false;
  updateMacroPlayProgress(0, total);
  setMacroPlayStatus(tr(`正在回放 0 / ${total}`));
  try {
    for (let index = 0; index < total; index += 1) {
      if (run.canceled) break;
      if (!inputEnabled()) {
        interrupted = true;
        break;
      }
      const step = run.macro.steps[index];
      if (index > 0) {
        await macroWait(run, step.delay_ms / run.speed);
        if (run.canceled) break;
        if (!inputEnabled()) {
          interrupted = true;
          break;
        }
      }
      await sendKey(step.key, {
        ctrlKey: !!step.ctrl,
        shiftKey: !!step.shift,
        altKey: !!step.alt,
        metaKey: !!step.meta,
      });
      sent += 1;
      updateMacroPlayProgress(sent, total);
      if (!run.canceled) setMacroPlayStatus(tr(`正在回放 ${sent} / ${total}`));
    }
    if (interrupted) {
      const message = tr(`已中断：键鼠转发已关闭，已发送 ${sent} / ${total}`);
      setMacroPlayStatus(message, 'error');
      toast(message, true);
    } else if (run.canceled) {
      setMacroPlayStatus(tr(`已取消，已发送 ${sent} / ${total}`));
    } else {
      setMacroPlayStatus(tr(`已回放完成（${sent} 个按键）`));
      toast(tr(`已回放 ${sent} 个按键`));
    }
  } finally {
    if (shared.macroPlayback === run) shared.macroPlayback = null;
  }
}

// DOM wiring; main.js calls this once, in the original fragment order.
export function init() {
  $('#macro-record-stop')?.addEventListener('click', stopKeyboardMacroRecording);
  $('#macro-save-form')?.addEventListener('submit', (event) => {
    event.preventDefault();
    saveRecordedMacro();
  });
  $$('[data-macro-save-close]').forEach((button) => button.addEventListener('click', closeMacroSaveDialog));
  $('#macro-save-dialog')?.addEventListener('click', (event) => {
    if (event.target === event.currentTarget) closeMacroSaveDialog();
  });
  $('#macro-save-dialog')?.addEventListener('close', () => {
    macroPendingSteps = [];
  });
  $$('[data-macro-manager-close]').forEach((button) => button.addEventListener('click', closeMacroManager));
  $('#macro-manager-dialog')?.addEventListener('click', (event) => {
    if (event.target === event.currentTarget) closeMacroManager();
  });
  $('#macro-manager-list')?.addEventListener('click', (event) => {
    const button = event.target.closest('button[data-macro-play],button[data-macro-rename],button[data-macro-export],button[data-macro-delete]');
    if (!button) return;
    if (button.dataset.macroPlay) startKeyboardMacro(readKeyboardMacros().find((m) => m.id === button.dataset.macroPlay));
    else if (button.dataset.macroRename) renameKeyboardMacro(button.dataset.macroRename);
    else if (button.dataset.macroExport) exportOneMacro(button.dataset.macroExport);
    else if (button.dataset.macroDelete) deleteKeyboardMacro(button.dataset.macroDelete);
  });
  $('#macro-import-btn')?.addEventListener('click', () => $('#macro-import-file')?.click());
  $('#macro-import-file')?.addEventListener('change', (event) => importMacroFile(event.target));
  $('#macro-export-all')?.addEventListener('click', exportAllMacros);
  $('#macro-play-speed')?.addEventListener('change', (event) => setMacroPlaybackSpeed(event.target.value));
  $('#macro-play-cancel')?.addEventListener('click', cancelMacroPlayback);
  $$('[data-macro-play-close]').forEach((button) => button.addEventListener('click', closeMacroPlayDialog));
  $('#macro-play-dialog')?.addEventListener('click', (event) => {
    if (event.target === event.currentTarget) closeMacroPlayDialog();
  });
  $('#macro-play-dialog')?.addEventListener('close', () => cancelMacroPlayback());
  updateMacroSpeedButtons();
  updateMacroRecordingIndicator();
  // Pure helpers used by the UI verification harness: no network or DOM.
  window.__wingmanTest = Object.assign(window.__wingmanTest || {}, {
    macroKeyNameValid,
    validateMacroFile,
    exportMacroDocument,
    macroStepFromStored,
    macroMaxCount: MACRO_MAX_COUNT,
    macroMaxSteps: MACRO_MAX_STEPS,
    macroMaxDelayMs: MACRO_MAX_DELAY_MS,
  });
}

export { recordMacroKey, renderKeyboardMacroMenu };
