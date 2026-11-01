import { shared } from './state.js';
import { $, $$ } from './00-core.js';
import { tr } from './01-i18n.js';
import { formStatus, latestOnly, request } from './03-session.js';
import { unwrapConfig } from './04-video-settings.js';

// Event notification settings (#84). Secrets are write-only: the server
// only reports whether one is configured, and an empty field keeps it.
const notifyForm = $('#notify-config');
const NOTIFY_EVENTS = ['login_succeeded', 'login_failed', 'power_led_changed', 'hid_failing', 'video_lost'];
const NOTIFY_NUMBERS = ['video_lost_seconds', 'rate_limit_seconds', 'hid_failure_threshold', 'hid_failure_window_seconds'];
const NOTIFY_SECRET_INPUT = { webhook: 'webhook_url', ntfy: 'ntfy_token', telegram: 'telegram_bot_token' };
function notifySecretLabel(configured, hint) {
  if (!configured) return tr('未配置');
  return hint ? tr(`已配置（${hint}）`) : tr('已配置');
}
function fillNotifyForm(notifications) {
  if (!notifyForm || !notifications) return;
  const n = notifications;
  const el = notifyForm.elements;
  el.webhook_enabled.checked = !!n.webhook?.enabled;
  el.ntfy_enabled.checked = !!n.ntfy?.enabled;
  el.telegram_enabled.checked = !!n.telegram?.enabled;
  el.ntfy_server.value = n.ntfy?.server || '';
  el.ntfy_topic.value = n.ntfy?.topic || '';
  el.telegram_chat_id.value = n.telegram?.chat_id || '';
  // Secret inputs are always cleared; their state is shown separately.
  Object.values(NOTIFY_SECRET_INPUT).forEach((name) => (el[name].value = ''));
  const states = {
    webhook: [n.webhook?.url_configured, n.webhook?.url_hint],
    ntfy: [n.ntfy?.token_configured],
    telegram: [n.telegram?.bot_token_configured],
  };
  Object.entries(states).forEach(([channel, [configured, hint]]) => {
    const label = $(`[data-notify-secret="${channel}"]`, notifyForm);
    if (label) label.textContent = notifySecretLabel(!!configured, hint);
    const clear = $(`[data-notify-clear="${channel}"]`, notifyForm);
    if (clear) clear.disabled = !configured;
    el[NOTIFY_SECRET_INPUT[channel]].placeholder = configured
      ? tr('已保存，留空保持不变')
      : channel === 'webhook'
        ? 'https://example.com/hook'
        : '';
  });
  NOTIFY_EVENTS.forEach((name) => (el[`event_${name}`].checked = n.events?.[name] !== false));
  NOTIFY_NUMBERS.forEach((name) => (el[name].value = n[name] ?? ''));
}
function notifyPatchFromForm() {
  const el = notifyForm.elements;
  const text = (name) => el[name].value.trim();
  const patch = {
    webhook: { enabled: el.webhook_enabled.checked },
    ntfy: { enabled: el.ntfy_enabled.checked, server: text('ntfy_server'), topic: text('ntfy_topic') || null },
    telegram: { enabled: el.telegram_enabled.checked, chat_id: text('telegram_chat_id') || null },
    events: {},
  };
  if (text('webhook_url')) patch.webhook.url = text('webhook_url');
  if (text('ntfy_token')) patch.ntfy.token = text('ntfy_token');
  if (text('telegram_bot_token')) patch.telegram.bot_token = text('telegram_bot_token');
  NOTIFY_EVENTS.forEach((name) => (patch.events[name] = el[`event_${name}`].checked));
  NOTIFY_NUMBERS.forEach((name) => {
    if (el[name].value !== '') patch[name] = Number(el[name].value);
  });
  return patch;
}
async function saveNotifications(patch) {
  const saved = await request('/api/config', { method: 'POST', body: JSON.stringify({ notifications: patch }) });
  if (saved?.notifications) {
    const config = unwrapConfig(shared.bootstrap);
    if (shared.bootstrap && typeof shared.bootstrap === 'object') config.notifications = saved.notifications;
    fillNotifyForm(saved.notifications);
  }
  return saved;
}
const notifyTicket = latestOnly();
async function refreshNotifications() {
  if (!notifyForm) return;
  const isCurrent = notifyTicket();
  try {
    const config = await request('/api/config');
    if (isCurrent()) fillNotifyForm(config?.notifications);
  } catch (_) {
    if (isCurrent()) fillNotifyForm(unwrapConfig(shared.bootstrap).notifications);
  }
}
function setNotifyResult(channel, ok, message) {
  const box = $(`[data-notify-result="${channel}"]`, notifyForm);
  if (!box) return;
  box.classList.toggle('ok', ok);
  box.classList.toggle('error', !ok);
  box.textContent = message;
}

// DOM wiring; main.js calls this once, in the original fragment order.
export function init() {
  notifyForm?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const button = $('button[type=submit]', notifyForm);
    if (button) button.disabled = true;
    try {
      await saveNotifications(notifyPatchFromForm());
      formStatus(notifyForm, true);
    } catch (err) {
      formStatus(notifyForm, false, err.message || '保存失败');
    } finally {
      if (button) button.disabled = false;
    }
  });
  $$('[data-notify-clear]', notifyForm || document).forEach((button) =>
    button.addEventListener('click', async () => {
      const channel = button.dataset.notifyClear;
      if (!confirm(tr('确定要清除已保存的密钥吗？该通道将停止推送。'))) return;
      const field = { webhook: 'clear_url', ntfy: 'clear_token', telegram: 'clear_bot_token' }[channel];
      button.disabled = true;
      try {
        await saveNotifications({ [channel]: { [field]: true } });
        setNotifyResult(channel, true, tr('已清除'));
      } catch (err) {
        button.disabled = false;
        setNotifyResult(channel, false, err.message || '清除失败');
      }
    }),
  );
  $$('[data-notify-test]', notifyForm || document).forEach((button) =>
    button.addEventListener('click', async () => {
      const channel = button.dataset.notifyTest;
      button.disabled = true;
      setNotifyResult(channel, true, tr('正在发送测试消息…'));
      try {
        // Test what the form shows: save first, then send through the server.
        await saveNotifications(notifyPatchFromForm());
        await request('/api/notifications/test', { method: 'POST', body: JSON.stringify({ channel }) });
        const time = new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
        setNotifyResult(channel, true, tr(`测试消息已发送 · ${time}`));
      } catch (err) {
        setNotifyResult(channel, false, err.message || '发送失败');
      } finally {
        button.disabled = false;
      }
    }),
  );
}

export { refreshNotifications };
