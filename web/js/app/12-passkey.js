import { shared } from './state.js';
import { $ } from './00-core.js';
import { t } from './01-i18n.js';
import { latestOnly, request, showState, toast } from './03-session.js';

function base64UrlToBuffer(base64url) {
  if (!base64url) return new ArrayBuffer(0);
  if (base64url instanceof ArrayBuffer) return base64url;
  if (ArrayBuffer.isView(base64url)) return base64url.buffer;
  const padding = '='.repeat((4 - (base64url.length % 4)) % 4);
  const base64 = (base64url + padding).replace(/-/g, '+').replace(/_/g, '/');
  const rawData = atob(base64);
  const outputArray = new Uint8Array(rawData.length);
  for (let i = 0; i < rawData.length; ++i) {
    outputArray[i] = rawData.charCodeAt(i);
  }
  return outputArray.buffer;
}
function bufferToBase64Url(buffer) {
  if (!buffer) return '';
  const bytes = buffer instanceof Uint8Array ? buffer : new Uint8Array(buffer);
  let binary = '';
  for (let i = 0; i < bytes.byteLength; i++) {
    binary += String.fromCharCode(bytes[i]);
  }
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}
function isIpAddress(hostname) {
  if (!hostname || hostname === 'localhost') return false;
  return /^(\d{1,3}\.){3}\d{1,3}$/.test(hostname) || hostname.startsWith('[') || hostname.includes(':');
}
function isPasskeySupported() {
  return !!(window.isSecureContext && window.PublicKeyCredential);
}
async function registerPasskey() {
  if (!isPasskeySupported()) {
    toast(t('passkey_insecure'), true);
    return;
  }
  if (isIpAddress(window.location.hostname)) {
    toast(t('passkey_ip_warn'), true);
    return;
  }
  const defaultName = navigator.userAgent.includes('Macintosh')
    ? 'Mac (Touch ID)'
    : navigator.userAgent.includes('iPhone')
      ? 'iPhone (Face ID)'
      : navigator.userAgent.includes('iPad')
        ? 'iPad (Touch/Face ID)'
        : navigator.userAgent.includes('Windows')
          ? 'Windows Hello'
          : 'Passkey Device';
  const name = prompt(t('passkey_prompt_name'), defaultName);
  if (name === null) return;
  const passkeyName = name.trim() || defaultName;

  const btn = $('#passkey-add-btn');
  const oldText = btn?.textContent || '';
  if (btn) {
    btn.disabled = true;
    btn.textContent = '...';
  }

  try {
    const startRes = await request('/api/passkey/register/start', {
      method: 'POST',
      body: JSON.stringify({ name: passkeyName }),
    });
    const { challenge_id, options } = startRes;
    const pk = { ...options.publicKey };
    pk.challenge = base64UrlToBuffer(pk.challenge);
    pk.user = { ...pk.user, id: base64UrlToBuffer(pk.user.id) };
    if (Array.isArray(pk.excludeCredentials) && pk.excludeCredentials.length > 0) {
      pk.excludeCredentials = pk.excludeCredentials.map((c) => ({
        ...c,
        id: base64UrlToBuffer(c.id),
      }));
    } else {
      delete pk.excludeCredentials;
    }
    if (pk.authenticatorSelection) {
      pk.authenticatorSelection = {
        ...pk.authenticatorSelection,
        residentKey: 'preferred',
      };
    }

    const cred = await navigator.credentials.create({ publicKey: pk });
    if (!cred) throw new Error('未能建立通行密鑰');

    const credentialPayload = {
      id: cred.id,
      rawId: bufferToBase64Url(cred.rawId),
      response: {
        clientDataJSON: bufferToBase64Url(cred.response.clientDataJSON),
        attestationObject: bufferToBase64Url(cred.response.attestationObject),
      },
      type: cred.type,
      extensions: cred.getClientExtensionResults ? cred.getClientExtensionResults() : {},
    };

    await request('/api/passkey/register/finish', {
      method: 'POST',
      body: JSON.stringify({
        challenge_id,
        name: passkeyName,
        credential: credentialPayload,
      }),
    });

    toast(t('passkey_added_success'));
    await refreshPasskeys();
  } catch (err) {
    if (err.name !== 'NotAllowedError') {
      toast(err.message || 'Passkey 绑定失败', true);
    }
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.textContent = oldText;
    }
  }
}

let conditionalAbortController = null;
let explicitPasskeyBusy = false;
let conditionalUiStarted = false;

async function loginWithPasskey(isConditional = false) {
  if (!isPasskeySupported()) {
    if (!isConditional) toast(t('passkey_insecure'), true);
    return;
  }
  if (isIpAddress(window.location.hostname)) {
    if (!isConditional) toast(t('passkey_ip_warn'), true);
    return;
  }

  const btn = $('#passkey-login-btn');
  const errorBox = $('#login-error');
  if (errorBox && !isConditional) errorBox.textContent = '';

  if (isConditional) {
    if (explicitPasskeyBusy) return;
    if (conditionalAbortController) {
      try {
        conditionalAbortController.abort();
      } catch (_) {}
      conditionalAbortController = null;
    }
    conditionalAbortController = new AbortController();
  } else {
    if (explicitPasskeyBusy) return;
    if (conditionalAbortController) {
      try {
        conditionalAbortController.abort();
      } catch (_) {}
      conditionalAbortController = null;
    }
    explicitPasskeyBusy = true;
    if (btn) btn.disabled = true;
  }

  try {
    const startRes = await request('/api/passkey/login/start', { method: 'POST' });
    const { challenge_id, options } = startRes;
    const pk = { ...options.publicKey };
    pk.challenge = base64UrlToBuffer(pk.challenge);
    if (Array.isArray(pk.allowCredentials) && pk.allowCredentials.length > 0) {
      pk.allowCredentials = pk.allowCredentials.map((c) => ({
        ...c,
        id: base64UrlToBuffer(c.id),
      }));
    } else {
      delete pk.allowCredentials;
    }

    const getOptions = { publicKey: pk };
    if (isConditional) {
      getOptions.mediation = 'conditional';
      if (conditionalAbortController) {
        getOptions.signal = conditionalAbortController.signal;
      }
    }

    const cred = await navigator.credentials.get(getOptions);
    if (!cred) return;

    const credentialPayload = {
      id: cred.id,
      rawId: bufferToBase64Url(cred.rawId),
      response: {
        clientDataJSON: bufferToBase64Url(cred.response.clientDataJSON),
        authenticatorData: bufferToBase64Url(cred.response.authenticatorData),
        signature: bufferToBase64Url(cred.response.signature),
        userHandle: cred.response.userHandle ? bufferToBase64Url(cred.response.userHandle) : null,
      },
      type: cred.type,
      extensions: cred.getClientExtensionResults ? cred.getClientExtensionResults() : {},
    };

    await request('/api/passkey/login/finish', {
      method: 'POST',
      body: JSON.stringify({
        challenge_id,
        credential: credentialPayload,
      }),
    });

    shared.bootstrap = await request('/api/bootstrap');
    showState(shared.bootstrap.authenticated ? 'main' : 'login');
  } catch (err) {
    if (err.name !== 'AbortError' && err.name !== 'NotAllowedError') {
      if (!isConditional) {
        if (errorBox) errorBox.textContent = err.message || 'Passkey 登录失败';
        else toast(err.message || 'Passkey 登录失败', true);
      }
    }
  } finally {
    if (isConditional) {
      conditionalAbortController = null;
    } else {
      explicitPasskeyBusy = false;
      if (btn) btn.disabled = false;
    }
  }
}

const passkeyTicket = latestOnly();
async function refreshPasskeys() {
  const isCurrent = passkeyTicket();
  const container = $('#passkey-items');
  const emptyHint = $('#passkey-empty-hint');
  const inspectorHint = $('#passkey-inspector-hint');
  const addBtn = $('#passkey-add-btn');

  if (isIpAddress(window.location.hostname)) {
    if (inspectorHint) {
      inspectorHint.textContent = t('passkey_ip_warn');
      inspectorHint.classList.remove('hidden');
    }
    if (addBtn) addBtn.disabled = true;
  } else if (!isPasskeySupported()) {
    if (inspectorHint) {
      inspectorHint.textContent = t('passkey_insecure');
      inspectorHint.classList.remove('hidden');
    }
    if (addBtn) addBtn.disabled = true;
  } else {
    if (inspectorHint) inspectorHint.classList.add('hidden');
    if (addBtn) addBtn.disabled = false;
  }

  try {
    const res = await request('/api/passkey/list');
    if (!isCurrent()) return;
    const list = res.passkeys || [];
    if (!container) return;
    container.replaceChildren();

    if (list.length === 0) {
      emptyHint?.classList.remove('hidden');
      container?.classList.add('hidden');
    } else {
      emptyHint?.classList.add('hidden');
      container?.classList.remove('hidden');

      list.forEach((pk) => {
        const item = document.createElement('div');
        item.className = 'media-item';
        item.style.padding = '8px 10px';
        item.style.marginBottom = '6px';

        const head = document.createElement('div');
        head.className = 'media-item-head';

        const nameBox = document.createElement('span');
        nameBox.className = 'media-item-name';

        const title = document.createElement('strong');
        title.textContent = pk.name || 'Passkey';

        const meta = document.createElement('span');
        meta.className = 'media-item-meta';
        const dateStr = pk.created_at_unix_seconds
          ? new Date(pk.created_at_unix_seconds * 1000).toLocaleDateString()
          : '';
        meta.textContent = dateStr;

        nameBox.append(title, meta);
        head.append(nameBox);

        const controls = document.createElement('div');
        controls.className = 'media-item-controls';

        const delBtn = document.createElement('button');
        delBtn.type = 'button';
        delBtn.className = 'danger';
        delBtn.style.minHeight = '28px';
        delBtn.style.padding = '0 8px';
        delBtn.style.fontSize = '12px';
        delBtn.textContent = t('passkey_delete');
        delBtn.addEventListener('click', async () => {
          if (!confirm(t('passkey_delete_confirm'))) return;
          try {
            await request(`/api/passkey/${encodeURIComponent(pk.id)}`, { method: 'DELETE' });
            toast(t('passkey_delete') + ' OK');
            await refreshPasskeys();
          } catch (e) {
            toast(e.message, true);
          }
        });

        controls.append(delBtn);
        item.append(head, controls);
        container.append(item);
      });
    }
  } catch (err) {
    // Silently ignore if not authorized
  }
}

function updateLoginPasskeyUI() {
  const container = $('#passkey-login-container');
  const insecureHint = $('#passkey-insecure-hint');
  const hasPasskeys = !!shared.bootstrap.has_passkeys;

  if (isIpAddress(window.location.hostname)) {
    if (insecureHint) {
      insecureHint.textContent = t('passkey_ip_warn');
      insecureHint.classList.remove('hidden');
    }
    if (container) container.classList.add('hidden');
    return;
  }

  if (!isPasskeySupported()) {
    if (hasPasskeys && insecureHint) {
      insecureHint.textContent = t('passkey_insecure');
      insecureHint.classList.remove('hidden');
    }
    if (container) container.classList.add('hidden');
    return;
  }

  if (insecureHint) insecureHint.classList.add('hidden');

  if (hasPasskeys) {
    if (container) container.classList.remove('hidden');

    // Trigger Conditional UI (Autofill) once if available
    if (
      !conditionalUiStarted &&
      window.PublicKeyCredential &&
      PublicKeyCredential.isConditionalMediationAvailable
    ) {
      PublicKeyCredential.isConditionalMediationAvailable()
        .then((available) => {
          if (available && !conditionalUiStarted) {
            conditionalUiStarted = true;
            loginWithPasskey(true);
          }
        })
        .catch(() => {});
    }
  } else {
    if (container) container.classList.add('hidden');
  }
}

// DOM wiring; main.js calls this once, in the original fragment order.
export function init() {
  $('#passkey-login-btn')?.addEventListener('click', () => loginWithPasskey(false));
  $('#passkey-add-btn')?.addEventListener('click', registerPasskey);
}

export { refreshPasskeys, updateLoginPasskeyUI };
