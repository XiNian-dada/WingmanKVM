import { $ } from './00-core.js';
import { tr } from './01-i18n.js';
import { latestOnly, request } from './03-session.js';

// Online upgrade (#91). The browser only confirms the version it was shown;
// the root unit resolves, downloads and verifies the release itself. While
// the installer restarts the service the status poll keeps retrying.
const updateCheckBtn = $('#update-check-btn');
const updatePanel = $('#update-panel');
const updateApplyBtn = $('#update-apply-btn');
const UPDATE_POLL_MS = 2000;
const UPDATE_IDLE_LIMIT_MS = 60 * 1000;
const UPDATE_RELOAD_MS = 3000;
const UPDATE_PHASES = {
  resolving: '正在获取版本信息…',
  downloading: '正在下载 ${}…',
  verifying: '正在校验 SHA256…',
  installing: '正在安装并重启服务…',
};
const UPDATE_ERRORS = {
  network: '网络错误',
  checksum: 'SHA256 校验失败',
  install_failed: '安装失败',
  legacy_policy: '需要主机管理员先选择维护模式',
  unsupported_arch: '不支持的 CPU 架构',
  invalid_release: '发布包无效',
  missing_tool: '缺少系统工具',
  io: '文件系统错误',
  stale: '升级任务异常中断',
  interrupted: '升级任务被中断',
};
let updateLatest = null;
let updateTimer = null;
let updateWatch = null;
const updateTicket = latestOnly();

const plainVersion = (value) => String(value || '').replace(/^v/, '');
function setUpdateText(selector, text, state = '') {
  const el = $(selector);
  if (!el) return;
  el.textContent = text;
  el.classList.toggle('ok', state === 'ok');
  el.classList.toggle('error', state === 'error');
}
function updateProgress(text, state = '') {
  updatePanel?.classList.remove('hidden');
  setUpdateText('#update-progress', text, state);
}
function updateErrorText(code) {
  return tr(UPDATE_ERRORS[code] || '未知错误');
}
function stopUpdateWatch() {
  clearTimeout(updateTimer);
  updateTimer = null;
  updateWatch = null;
}
function setUpdateBusy(busy) {
  if (updateCheckBtn) updateCheckBtn.disabled = busy;
  if (updateApplyBtn) updateApplyBtn.disabled = busy;
}

// Raw fetch: a 401 here means the restarted service dropped the session,
// which must not tear down the page mid-upgrade like request() would.
async function fetchUpdateStatus() {
  const response = await fetch('/api/update/status', { cache: 'no-store', credentials: 'same-origin' });
  if (response.status === 401) return { unauthorized: true };
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  return response.json();
}

function renderUpdateOutcome(data) {
  const status = data.status || {};
  if (status.state === 'running') {
    const phase = UPDATE_PHASES[status.phase] || '升级任务已启动…';
    updateProgress(tr(phase.replace('${}', status.version || '')));
    return 'running';
  }
  if (status.state === 'succeeded') {
    updateProgress(tr(`升级成功：已更新到 ${status.version}`), 'ok');
    return 'succeeded';
  }
  if (status.state === 'failed') {
    // Only the installer rolls back; every other failure happens before it
    // runs (or, when interrupted, at a point this status cannot tell).
    const reason = updateErrorText(status.error);
    updateProgress(
      status.error === 'install_failed'
        ? tr(`升级失败：${reason}（已自动回滚，详见 journalctl -u wingmankvm-online-upgrade）`)
        : tr(`升级失败：${reason}（详见 journalctl -u wingmankvm-online-upgrade）`),
      'error',
    );
    return 'failed';
  }
  return 'idle';
}

async function watchUpdate() {
  const watch = updateWatch;
  if (!watch) return;
  let data;
  try {
    data = await fetchUpdateStatus();
  } catch (_) {
    if (updateWatch !== watch) return;
    watch.restarting = true;
    updateProgress(tr('服务正在重启，自动重连中…'));
    updateTimer = setTimeout(watchUpdate, UPDATE_POLL_MS);
    return;
  }
  if (updateWatch !== watch) return;
  if (data.unauthorized) {
    stopUpdateWatch();
    updateProgress(tr('服务已重启，请重新登录以查看升级结果'), 'ok');
    setTimeout(() => location.reload(), UPDATE_RELOAD_MS);
    return;
  }
  const state = renderUpdateOutcome(data);
  if (state === 'succeeded') {
    stopUpdateWatch();
    if (plainVersion(data.current_version) === plainVersion(data.status.version)) {
      updateProgress(tr(`升级成功：已更新到 ${data.status.version}，即将刷新页面…`), 'ok');
      setTimeout(() => location.reload(), UPDATE_RELOAD_MS);
    }
    setUpdateBusy(false);
    return;
  }
  if (state === 'failed') {
    stopUpdateWatch();
    setUpdateBusy(false);
    return;
  }
  if (state === 'idle' && Date.now() - watch.started > UPDATE_IDLE_LIMIT_MS) {
    stopUpdateWatch();
    setUpdateBusy(false);
    updateProgress(tr('升级任务未能启动，请查看 journalctl -u wingmankvm-online-upgrade'), 'error');
    return;
  }
  if (state === 'idle' && !watch.restarting) updateProgress(tr('升级任务已启动…'));
  updateTimer = setTimeout(watchUpdate, UPDATE_POLL_MS);
}
function startUpdateWatch() {
  stopUpdateWatch();
  updateWatch = { started: Date.now(), restarting: false };
  setUpdateBusy(true);
  updateTimer = setTimeout(watchUpdate, 0);
}

function renderRelease(data) {
  const latest = data.latest;
  updateLatest = data.update_available && data.supported ? latest.tag : null;
  updatePanel?.classList.remove('hidden');
  setUpdateText('#update-current', tr(`当前版本：${data.current_version}`));
  setUpdateText(
    '#update-title',
    data.update_available ? tr(`发现新版本 ${latest.tag}`) : tr(`已是最新版本（${data.current_version}）`),
  );
  const notes = $('#update-notes');
  if (notes) {
    const text = data.update_available ? latest.notes || '' : '';
    notes.textContent = latest.notes_truncated ? `${text}\n…` : text;
    notes.classList.toggle('hidden', !text);
  }
  const link = $('#update-link');
  if (link) {
    link.classList.toggle('hidden', !data.update_available);
    // The server builds this URL from the fixed repository and tag.
    if (/^https:\/\/github\.com\//.test(latest.url || '')) link.href = latest.url;
  }
  if (updateApplyBtn) {
    updateApplyBtn.classList.toggle('hidden', !updateLatest);
    updateApplyBtn.textContent = updateLatest ? tr(`升级到 ${updateLatest}`) : '';
  }
  if (data.update_available && !data.supported)
    updateProgress(tr('此安装不支持在线升级，请先用安装脚本升级一次'), 'error');
}

async function checkForUpdate() {
  if (!updateCheckBtn || updateWatch) return;
  const isCurrent = updateTicket();
  updateCheckBtn.disabled = true;
  updateProgress(tr('正在检查更新…'));
  try {
    const data = await request('/api/update/check', { method: 'POST', body: '{}' });
    if (!isCurrent()) return;
    setUpdateText('#update-progress', '');
    renderRelease(data);
  } catch (err) {
    if (isCurrent()) updateProgress(tr(`检查更新失败：${err.message || '未知错误'}`), 'error');
  } finally {
    if (!updateWatch) updateCheckBtn.disabled = false;
  }
}

async function applyUpdate() {
  const version = updateLatest;
  if (!version || updateWatch) return;
  if (!confirm(tr(`确定升级到 ${version} 吗？升级期间服务会重启，页面将自动重连。`))) return;
  setUpdateBusy(true);
  updateProgress(tr('正在启动升级任务…'));
  try {
    await request('/api/update/apply', { method: 'POST', body: JSON.stringify({ version }) });
    updateApplyBtn?.classList.add('hidden');
    startUpdateWatch();
  } catch (err) {
    setUpdateBusy(false);
    updateProgress(tr(`无法启动升级：${err.message || '未知错误'}`), 'error');
  }
}

// Opening the settings resumes the progress view of a run in flight (for
// example after a reload) and shows the outcome of the last one.
async function refreshOnlineUpdate() {
  if (!updatePanel || updateWatch) return;
  try {
    const data = await fetchUpdateStatus();
    if (data.unauthorized || updateWatch) return;
    setUpdateText('#update-current', tr(`当前版本：${data.current_version}`));
    if (data.status?.state === 'running') startUpdateWatch();
    else if (data.status?.state !== 'idle') renderUpdateOutcome(data);
  } catch (_) {}
}

// DOM wiring; main.js calls this once, in the original fragment order.
export function init() {
  updateCheckBtn?.addEventListener('click', checkForUpdate);
  updateApplyBtn?.addEventListener('click', applyUpdate);
}

export { refreshOnlineUpdate };
