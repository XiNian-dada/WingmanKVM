import { $ } from './00-core.js';
import { tr } from './01-i18n.js';
import { latestOnly, request } from './03-session.js';

const form = $('#tls-config');
const ticket = latestOnly();
let customConfigured = false;
let applying = false;

function syncMode() {
  const custom = form.elements.mode.value === 'custom';
  $('#tls-custom-fields').classList.toggle('hidden', !custom);
  $('#tls-self-signed-hint').classList.toggle('hidden', custom);
  $('#tls-regenerate-field').classList.toggle('hidden', custom);
  const encrypted = location.protocol === 'https:';
  for (const name of ['certificate', 'private_key']) form.elements[name].disabled = !custom || !encrypted;
  $('#tls-custom-hint').textContent = tr(!encrypted
    ? '请先保存自动自签模式，再打开 HTTPS 地址导入证书和私钥。'
    : customConfigured ? '已保存自定义证书；两个文件都留空可继续使用。私钥不会回显。'
      : '证书与私钥必须配套。私钥不会回显，也不会随配置导出。');
}

function display(status) {
  customConfigured = !!status.custom_configured;
  form.elements.mode.value = status.mode === 'custom' ? 'custom' : 'self_signed';
  form.elements.public_origin.value = status.public_origin || `https://${location.host}`;
  const certificate = status.certificate;
  const message = !status.enabled ? tr('当前为 HTTP 兼容模式；保存后将启用 HTTPS。')
    : certificate ? `${tr(status.mode === 'custom' ? '自定义证书' : '自动自签证书')} · ${certificate.subject} · ${tr('到期时间')} ${certificate.not_after}`
      : tr('HTTPS 已启用');
  $('#tls-status').textContent = message;
  $('#tls-fingerprint').textContent = certificate ? `SHA-256: ${certificate.fingerprint_sha256}` : '';
  $('#tls-download').classList.toggle('hidden', !certificate);
  form.elements.regenerate.checked = false;
  syncMode();
}

export async function refreshTls() {
  if (!form || applying) return;
  const current = ticket();
  try {
    const status = await request('/api/tls');
    if (current()) display(status);
  } catch (error) {
    if (current()) $('#tls-status').textContent = error.message || tr('无法读取证书状态');
  }
}

export function init() {
  if (!form) return;
  form.elements.mode.addEventListener('change', () => {
    form.elements.certificate.value = '';
    form.elements.private_key.value = '';
    syncMode();
  });
  form.addEventListener('submit', async (event) => {
    event.preventDefault();
    const button = $('button[type="submit"]', form), result = $('#tls-result'), link = $('#tls-open');
    if (applying) return;
    applying = true;
    ticket();
    button.disabled = true;
    link.classList.add('hidden');
    result.textContent = tr('正在校验并应用证书…');
    try {
      const origin = new URL(form.elements.public_origin.value.trim());
      if (origin.protocol !== 'https:' || origin.username || origin.password || origin.search || origin.hash
        || (origin.pathname !== '/' && origin.pathname !== '')) throw new Error(tr('请填写 HTTPS 地址，不含路径、参数或账户信息。'));
      const body = { mode: form.elements.mode.value, public_origin: origin.origin, regenerate: form.elements.mode.value === 'self_signed' && form.elements.regenerate.checked };
      const certificate = form.elements.certificate.files[0], key = form.elements.private_key.files[0];
      if (body.mode === 'custom') {
        if (location.protocol !== 'https:') throw new Error(tr('请先保存自动自签模式，再打开 HTTPS 地址导入证书和私钥。'));
        if (!!certificate !== !!key || (!certificate && !customConfigured)) throw new Error(tr('请选择配套的证书文件和私钥文件。'));
        if (certificate) {
          if (certificate.size > 128 * 1024 || key.size > 64 * 1024) throw new Error(tr('证书或私钥文件过大。'));
          body.certificate_pem = await certificate.text();
          body.private_key_pem = await key.text();
        }
      }
      const status = await request('/api/tls', { method: 'POST', body: JSON.stringify(body) });
      display(status);
      result.textContent = tr(status.reauthenticate ? 'HTTPS 已启用，旧会话已退出。请打开 HTTPS 地址重新登录。' : 'HTTPS 已应用，新连接使用当前证书。');
      link.href = origin.origin + '/';
      link.classList.remove('hidden');
    } catch (error) {
      result.textContent = error.message || tr('证书应用失败');
    } finally {
      form.elements.certificate.value = '';
      form.elements.private_key.value = '';
      button.disabled = false;
      applying = false;
    }
  });
}
