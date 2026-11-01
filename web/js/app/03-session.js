import { shared } from './state.js';
import { $, $$, app, authView, loginForm, setupForm, viewport } from './00-core.js';
import { getSetting } from './00-settings.js';
import { t } from './01-i18n.js';
import { fitConsoleToVideo } from './02-theme-layout.js';
import { applyBootstrap } from './04-video-settings.js';
import { scanDevices, updateSetupCapabilities } from './06-devices.js';
import { startVideo, stopVideo } from './07-video.js';
import { stopInput } from './08-input.js';
import {
  connectInputSocket, disconnectInputSocket, HID_SOCKET_ROUTES, inputSocketReady, sendOverInputSocket,
} from './08a-input-socket.js';
import { terminalConnect, terminalDisconnect, terminalReset } from './09-terminal.js';
import { refreshStatus, setInspectorOpen } from './10-workspace-status.js';
import { refreshMedia } from './11-media.js';
import { refreshPasskeys, updateLoginPasskeyUI } from './12-passkey.js';

let toastTimer = 0;
const setupGuides = {
  account: {
    kicker: '01 / 管理员',
    title: '账号只设置一次',
    items: [
      ['网页与终端', '管理员密码也会同步给本机终端用户 wingman。'],
      ['强密码', '至少 12 位，并包含大小写字母、数字和符号。'],
      ['一次性链接', '初始化令牌会从地址中自动读取并立即清除。'],
    ],
  },
  devices: {
    kicker: '02 / 连接',
    title: '先自动检测',
    items: [
      ['唯一候选', '验证通过且只有一个设备时直接采用。'],
      ['多个设备', '只在存在多个可用候选时让你选择。'],
      ['手动配置', '设备路径只留给自定义 Gadget 和特殊板卡。'],
    ],
  },
  video: {
    kicker: '视频采集',
    title: '认准视频节点',
    items: [
      ['插在 Host 口', '采集卡是输入设备，不要接到 OTG Device 口。'],
      ['检查能力', '选择同时支持 Video Capture 和 MJPG 的 USB 节点。'],
      ['排除 metadata', '只有 Metadata Capture 的 /dev/videoN 不会输出画面。'],
    ],
  },
  input: {
    kicker: 'USB 控制',
    title: '优先精准同步',
    items: [
      ['OTG Device 口', '键盘、鼠标和虚拟 U 盘从这里连接被控机。'],
      ['绝对指针', '网页位置会直接映射到远端位置，默认优先使用。'],
      ['相对鼠标', '保留给只支持 Boot Mouse 的 BIOS 或 UEFI。'],
    ],
  },
  media: {
    kicker: '虚拟介质',
    title: '镜像就是一只 U 盘',
    items: [
      ['ISO', '始终按只读光驱挂载。'],
      ['IMG', '可作为读写 U 盘，但弹出前先在被控机卸载。'],
      ['避免双写', '同一镜像不要同时被 RK3399 和被控机写入。'],
    ],
  },
  power: {
    kicker: 'GPIO 电源',
    title: '接线必须人工确认',
    items: [
      ['使用继电器', '用干接点并联被控机电源按钮，不要让 GPIO 直接承受外部电压。'],
      ['线路与极性', '软件能发现 gpiochip，但不能猜出 line 和高低电平。'],
      ['可以跳过', '先完成 KVM，确认原理图和接线后再启用电源控制。'],
    ],
  },
};

let setupGuideTrigger = null;
function openSetupGuide(key, trigger) {
  const guide = setupGuides[key];
  if (!guide) return;
  if (setupGuideTrigger) setupGuideTrigger.setAttribute('aria-expanded', 'false');
  setupGuideTrigger = trigger || null;
  if (setupGuideTrigger) setupGuideTrigger.setAttribute('aria-expanded', 'true');
  $('#auth-guide-kicker').textContent = guide.kicker;
  $('#auth-guide-title').textContent = guide.title;
  const body = $('#auth-guide-body');
  body.replaceChildren();
  for (const [title, copy] of guide.items) {
    const item = document.createElement('div');
    item.className = 'auth-guide-item';
    const strong = document.createElement('strong');
    strong.textContent = title;
    const text = document.createElement('span');
    text.textContent = copy;
    item.append(strong, text);
    body.append(item);
  }
  $('#auth-story-home').classList.add('hidden');
  const panel = $('#auth-guide');
  panel.classList.remove('hidden');
  requestAnimationFrame(() => {
    if (matchMedia('(max-width:720px)').matches)
      panel.scrollIntoView({
        block: 'start',
        behavior: matchMedia('(prefers-reduced-motion:reduce)').matches ? 'auto' : 'smooth',
      });
    $('#auth-guide-back').focus({ preventScroll: true });
  });
}
function closeSetupGuide(restoreFocus = true) {
  $('#auth-guide').classList.add('hidden');
  $('#auth-story-home').classList.remove('hidden');
  const trigger = setupGuideTrigger;
  if (trigger) trigger.setAttribute('aria-expanded', 'false');
  setupGuideTrigger = null;
  if (restoreFocus && trigger?.isConnected) requestAnimationFrame(() => trigger.focus());
}

function toast(message, bad = false) {
  const el = $('#toast');
  el.textContent = message;
  el.style.borderColor = bad ? '#ee0000' : '';
  el.style.color = bad ? '#c50000' : '';
  el.classList.add('show');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => el.classList.remove('show'), 2600);
}
async function request(url, options = {}) {
  // HID endpoints prefer the input WebSocket when it is connected.
  // keepalive requests (sent while the page unloads) stay on HTTP: the
  // browser only guarantees delivery for those, not for socket messages.
  const socketRoute =
    (options.method || 'GET') === 'POST' && !options.keepalive && !options.viaHttp && HID_SOCKET_ROUTES[url];
  if (socketRoute && inputSocketReady())
    return sendOverInputSocket(socketRoute, options.body, () => request(url, { ...options, viaHttp: true }));
  const headers = new Headers(options.headers || {});
  if (options.body && !(options.body instanceof FormData) && !headers.has('content-type'))
    headers.set('content-type', 'application/json');
  const response = await fetch(url, { ...options, headers, cache: 'no-store', credentials: 'same-origin' });
  const type = response.headers.get('content-type') || '';
  const data = type.includes('json') ? await response.json() : await response.text();
  if (response.status === 401 && url !== '/api/login' && !app.classList.contains('hidden')) {
    clearAuthSecrets({ token: true });
    shared.bootstrap.authenticated = false;
    stopInput();
    showState('login');
    $('#login-error').textContent = '会话已过期，请重新登录。';
  }
  if (!response.ok) {
    const error = new Error(
      (data && (data.error || data.message)) || data || `请求失败 (${response.status})`,
    );
    error.status = response.status;
    throw error;
  }
  return data;
}
// Returns a ticket factory: each call starts a new request generation and
// yields a check that is true only while no newer request has started, so a
// slow response cannot overwrite fresher state.
function latestOnly() {
  let generation = 0;
  return () => {
    const mine = ++generation;
    return () => mine === generation;
  };
}
// Serializes a form submission: ignores re-entry while one is in flight and
// exposes the busy state to assistive technology.
async function submitOnce(form, run) {
  const button = $('button[type=submit]', form);
  if (form.getAttribute('aria-busy') === 'true') return;
  form.setAttribute('aria-busy', 'true');
  if (button) button.disabled = true;
  try {
    await run();
  } finally {
    form.setAttribute('aria-busy', 'false');
    if (button) button.disabled = false;
  }
}
// Inline save feedback next to a form's submit button, so the result stays
// visible where the user acted instead of only in a transient toast.
function formStatus(form, ok, message = '') {
  const submit = $('button[type=submit]', form);
  if (!submit) return;
  let status = $('.form-status', submit.parentElement);
  if (!status) {
    status = document.createElement('span');
    status.className = 'form-status';
    status.setAttribute('role', 'status');
    submit.before(status);
  }
  const time = new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  status.classList.toggle('error', !ok);
  status.textContent = ok ? `已保存 · ${time}` : message || '保存失败';
}
const rawValue = (form, name) => new FormData(form).get(name)?.toString() || '';
const value = (form, name) => rawValue(form, name).trim();
function clearAuthSecrets(options = {}) {
  if (options.login !== false) loginForm.elements.password.value = '';
  if (options.setup !== false) {
    setupForm.elements.password.value = '';
    setupForm.elements.confirm_password.value = '';
  }
  if (options.token) {
    setupForm.elements.setup_token.value = '';
    if (shared.bootstrap?.setup_required && shared.bootstrap?.token_required !== false)
      setupForm.elements.setup_token.closest('.field')?.classList.remove('hidden');
  }
}
const optionalNumber = (v) => (v === '' ? null : Number(v));
function hardwareFrom(form) {
  const mouse = value(form, 'mouse_device') || null,
    absolute = value(form, 'absolute_pointer_device') || null,
    selected = value(form, 'pointer_mode') === 'relative' ? 'relative' : 'absolute';
  return {
    video_device: value(form, 'video_device') || null,
    keyboard_device: value(form, 'keyboard_device') || null,
    mouse_device: mouse,
    absolute_pointer_device: absolute,
    pointer_mode: selected,
    power_enabled: form.elements.power_enabled.checked,
    gpio_chip: value(form, 'gpio_chip') || null,
    gpio_line: optionalNumber(value(form, 'gpio_line')),
    active_high: value(form, 'active_high') !== 'false',
    media_enabled: form.elements.media_enabled.checked,
    lun_path: value(form, 'lun_path') || null,
    image_directory: value(form, 'image_directory') || null,
  };
}
function showState(state) {
  const wasMain = !app.classList.contains('hidden');
  if (state === 'main') clearAuthSecrets({ token: true });
  else if (state === 'login' && wasMain) clearAuthSecrets({ token: true });
  $('#boot-panel').classList.add('hidden');
  setupForm.classList.toggle('hidden', state !== 'setup');
  loginForm.classList.toggle('hidden', state !== 'login');
  authView.classList.toggle('hidden', state === 'main');
  app.classList.toggle('hidden', state !== 'main');
  if (state !== 'setup') closeSetupGuide(false);
  if (state !== 'main') disconnectInputSocket();
  if (state === 'main') {
    shared.pageActive = !document.hidden;
    connectInputSocket();
    applyBootstrap();
    startVideo();
    refreshStatus();
    scanDevices();
    // "Expand by default on wide screens" decides the initial state each
    // time the console opens; manual toggles apply only to this visit.
    setInspectorOpen(!matchMedia('(max-width:900px)').matches && getSetting('auto_expand_inspector', true));
    if (shared.bootstrap.capabilities?.mass_storage) refreshMedia();
    else $('#media-list').textContent = '虚拟介质尚未配置';
    refreshPasskeys();
    requestAnimationFrame(() => {
      fitConsoleToVideo(true);
      viewport.focus({ preventScroll: true });
    });
    if (shared.videoWorkspace === 'terminal') {
      terminalReset();
      terminalConnect();
    }
  } else {
    terminalDisconnect();
    stopInput();
    stopVideo();
    if (state === 'login') updateLoginPasskeyUI();
    requestAnimationFrame(() => $(state === 'setup' ? '#setup-form h2' : '#login-form h2')?.focus());
  }
}
function consumeSetupToken() {
  const params = new URLSearchParams(location.hash.slice(1)),
    token = params.get('setup');
  if (!token) return;
  setupForm.elements.setup_token.value = token;
  $('#setup-token-field').classList.add('hidden');
  history.replaceState(null, '', `${location.pathname}${location.search}`);
}
async function start() {
  try {
    shared.bootstrap = await request('/api/bootstrap');
    const token = setupForm.elements.setup_token;
    token.required = shared.bootstrap.token_required !== false;
    token
      .closest('.field')
      .classList.toggle(
        'hidden',
        shared.bootstrap.setup_required && (shared.bootstrap.token_required === false || !!token.value),
      );
    const state = shared.bootstrap.setup_required ? 'setup' : shared.bootstrap.authenticated ? 'main' : 'login';
    showState(state);
    if (state === 'setup' && !setupScanStarted) {
      setupScanStarted = true;
      scanDevices($('#setup-device-results'));
    }
  } catch (error) {
    $('#boot-panel h2').textContent = '无法连接 WingmanKVM';
    $('#boot-panel p').textContent = error.message;
  }
}
let setupStep = 0,
  setupScanStarted = false;
function setSetupStep(next) {
  setupStep = Math.max(0, Math.min(1, next));
  $$('.setup-step', setupForm).forEach((step, index) =>
    step.classList.toggle('active', index === setupStep),
  );
  $$('.progress-step', setupForm).forEach((step, index) => {
    step.classList.toggle('active', index === setupStep);
    step.classList.toggle('done', index < setupStep);
  });
  $('.auth-panel').scrollTop = 0;
  if (setupStep === 1) {
    if (!setupScanStarted) {
      setupScanStarted = true;
      scanDevices($('#setup-device-results'));
    }
    updateSetupCapabilities();
  }
}
function validateSetupAccount() {
  const username = value(setupForm, 'username'),
    password = rawValue(setupForm, 'password'),
    confirm = rawValue(setupForm, 'confirm_password'),
    usernameInput = setupForm.elements.username,
    passwordInput = setupForm.elements.password,
    confirmInput = setupForm.elements.confirm_password,
    strong =
      [...password].length >= 12 &&
      /\p{Lu}/u.test(password) &&
      /\p{Ll}/u.test(password) &&
      /\p{N}/u.test(password) &&
      /[^\p{L}\p{N}\s]/u.test(password);
  usernameInput.setCustomValidity(
    /^[A-Za-z0-9._-]{3,64}$/.test(username) ? '' : '账号使用 3–64 位字母、数字、点、下划线或短横线',
  );
  passwordInput.setCustomValidity(strong ? '' : '密码至少 12 位，并包含大写字母、小写字母、数字和符号');
  confirmInput.setCustomValidity(password === confirm ? '' : '两次输入的密码不一致');
  const valid = [setupForm.elements.setup_token, usernameInput, passwordInput, confirmInput].every(
    (input) => input.reportValidity(),
  );
  return valid;
}

// DOM wiring; main.js calls this once, in the original fragment order.
export function init() {
  document.addEventListener('click', (event) => {
    const button = event.target.closest?.('[data-guide]');
    if (!button) return;
    event.preventDefault();
    event.stopPropagation();
    openSetupGuide(button.dataset.guide, button);
  });
  $('#auth-guide-back').addEventListener('click', closeSetupGuide);
  $$('[data-setup-next]', setupForm).forEach((button) =>
    button.addEventListener('click', () => {
      if (setupStep === 0 && !validateSetupAccount()) return;
      setSetupStep(setupStep + 1);
    }),
  );
  $$('[data-setup-prev]', setupForm).forEach((button) =>
    button.addEventListener('click', () => setSetupStep(setupStep - 1)),
  );
  setupForm.elements.password.addEventListener('input', () =>
    setupForm.elements.password.setCustomValidity(''),
  );
  setupForm.elements.confirm_password.addEventListener('input', () =>
    setupForm.elements.confirm_password.setCustomValidity(''),
  );
  setupForm.addEventListener('submit', async (event) => {
    event.preventDefault();
    if (!validateSetupAccount()) {
      setSetupStep(0);
      return;
    }
    const button = $('button[type=submit]', setupForm),
      error = $('#setup-error'),
      label = button.textContent;
    button.disabled = true;
    button.textContent = '正在初始化…';
    setupForm.setAttribute('aria-busy', 'true');
    error.textContent = '';
    try {
      const payload = {
        username: value(setupForm, 'username'),
        password: rawValue(setupForm, 'password'),
        setup_token: rawValue(setupForm, 'setup_token'),
        ...hardwareFrom(setupForm),
      };
      await request('/api/setup', { method: 'POST', body: JSON.stringify(payload) });
      clearAuthSecrets({ token: true });
      toast('初始化完成，正在建立控制台');
      shared.bootstrap = await request('/api/bootstrap');
      showState(shared.bootstrap.authenticated ? 'main' : 'login');
    } catch (e) {
      error.textContent = e.message;
      clearAuthSecrets({ login: false });
      if (e.message.includes('令牌')) $('#setup-token-field').classList.remove('hidden');
    } finally {
      button.disabled = false;
      button.textContent = label;
      setupForm.setAttribute('aria-busy', 'false');
    }
  });
  loginForm.addEventListener('submit', async (event) => {
    event.preventDefault();
    const button = $('button[type=submit]', loginForm),
      error = $('#login-error'),
      label = button.textContent;
    button.disabled = true;
    button.textContent = '正在连接…';
    loginForm.setAttribute('aria-busy', 'true');
    error.textContent = '';
    try {
      await request('/api/login', {
        method: 'POST',
        body: JSON.stringify({
          username: value(loginForm, 'username'),
          password: rawValue(loginForm, 'password'),
        }),
      });
      clearAuthSecrets({ token: true });
      shared.bootstrap = await request('/api/bootstrap');
      showState(shared.bootstrap.authenticated ? 'main' : 'login');
    } catch (e) {
      error.textContent = e.message;
      clearAuthSecrets({ setup: false });
    } finally {
      button.disabled = false;
      button.textContent = label;
      loginForm.setAttribute('aria-busy', 'false');
    }
  });
  // Only show the login screen once the server confirms the session is gone;
  // the HttpOnly cookie cannot be cleared from script, so a failed logout
  // must not pretend to have signed out.
  $('#logout').addEventListener('click', async () => {
    const button = $('#logout');
    if (button.disabled) return;
    button.disabled = true;
    try {
      try {
        await request('/api/logout', { method: 'POST' });
      } catch (error) {
        if (error.status !== 401) {
          toast(t('logout_failed', '退出失败，会话仍然有效，请重试') + `：${error.message}`, true);
          return;
        }
      }
      clearAuthSecrets({ token: true });
      await stopInput();
      shared.bootstrap.authenticated = false;
      try {
        shared.bootstrap = await request('/api/bootstrap');
      } catch (_) {}
      showState('login');
    } finally {
      button.disabled = false;
    }
  });
}

export {
  clearAuthSecrets, consumeSetupToken, formStatus, latestOnly, optionalNumber, request, showState, start,
  submitOnce, toast, value,
};
