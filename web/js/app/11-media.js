import { shared } from './state.js';
import { $, $$ } from './00-core.js';
import { tr } from './01-i18n.js';
import { latestOnly, request, toast } from './03-session.js';
import {
  formatBytes, mediaChoices, mediaItems, mediaStateLabel, mediaTypeLabel,
} from './10-workspace-status.js';

let mediaBusy = false;
let mediaServerBusy = false;
let mediaDownloadBusy = false;
let mediaStatus = {};

function mountedMediaName(status, items) {
  return (
    status?.mounted_name ||
    items.find((item) => typeof item !== 'string' && (item.mounted || item.attached))?.name ||
    ''
  );
}
function writableMountedDisk(status, name) {
  return !!name && status?.read_only === false && status?.media_type === 'disk';
}
function syncMediaBusy() {
  const locked = mediaBusy || mediaServerBusy;
  $$('.media-operation').forEach(
    (control) => (control.disabled = locked || control.dataset.mediaLocked === 'true'),
  );
  $$(
    '#media-upload input,#media-upload button:not(#upload-cancel),#media-download input,#media-download button:not(#media-download-cancel)',
  ).forEach((control) => (control.disabled = mediaBusy || mediaDownloadBusy));
  $('#media-refresh').disabled = mediaBusy || mediaDownloadBusy;
  $('#media-list').setAttribute('aria-busy', String(locked));
}
async function runMediaAction(action, success) {
  if (mediaBusy || mediaServerBusy) return;
  mediaBusy = true;
  syncMediaBusy();
  try {
    await action();
    toast(success);
    await refreshMedia();
  } catch (error) {
    toast(error.message, true);
    await refreshMedia();
  } finally {
    mediaBusy = false;
    syncMediaBusy();
  }
}
function renderMediaStatus(status, items) {
  const box = $('#media-status'),
    name = mountedMediaName(status, items),
    state = status?.state || (name ? 'attached' : 'detached'),
    mounted = !!name;
  mediaServerBusy = ['attaching', 'detaching', 'ejecting', 'busy'].includes(state);
  box.className = `media-status${mounted ? ' mounted' : ''}${mediaServerBusy ? ' busy' : ''}${state === 'error' ? ' error' : ''}`;
  box.replaceChildren();
  const dot = document.createElement('span');
  dot.className = 'status-dot';
  const copy = document.createElement('span');
  copy.className = 'media-status-copy';
  const title = document.createElement('strong');
  title.textContent = name || mediaStateLabel(state) || '未挂载';
  if (name) title.dataset.noI18n = '';
  const detail = document.createElement('span');
  if (mounted) {
    const parts = [mediaTypeLabel(status?.media_type), status?.read_only === false ? '读写' : '只读'];
    const stateText = mediaStateLabel(state);
    if (stateText && stateText !== '已挂载') parts.unshift(stateText);
    // One text node per part, so each is translated as its own phrase.
    detail.append(...parts.flatMap((part, index) => (index ? [' · ', part] : [part])));
  } else detail.textContent = state === 'error' ? '请检查 LUN 配置' : '选择镜像即可挂载';
  copy.append(title, detail);
  box.append(dot, copy);
  if (mounted) {
    const actions = document.createElement('span');
    actions.className = 'media-status-actions';
    const detach = document.createElement('button');
    detach.type = 'button';
    detach.className = 'secondary media-operation';
    detach.textContent = '弹出';
    detach.addEventListener('click', () => {
      if (
        writableMountedDisk(status, name) &&
        !confirm(tr('请先在目标机中弹出此 U 盘。确认已经弹出并继续吗？'))
      )
        return;
      runMediaAction(
        () => request('/api/media/detach', { method: 'POST', body: JSON.stringify({ force: false }) }),
        '介质已弹出',
      );
    });
    actions.append(detach);
    if (status?.forced_eject_supported) {
      const force = document.createElement('button');
      force.type = 'button';
      force.className = 'danger media-operation';
      force.textContent = '强制弹出';
      force.title = '强制弹出可能损坏镜像文件系统';
      force.addEventListener('click', () => {
        if (!confirm(tr('强制弹出可能损坏镜像文件系统，确定继续吗？'))) return;
        runMediaAction(
          () => request('/api/media/detach', { method: 'POST', body: JSON.stringify({ force: true }) }),
          '介质已强制弹出',
        );
      });
      actions.append(force);
    }
    box.append(actions);
  }
  syncMediaBusy();
}
function renderMediaItem(item, mountedName) {
  const data = typeof item === 'string' ? { name: item, size: null, mounted: false } : item,
    name = data.name || data.path || '',
    isIso = name.toLowerCase().endsWith('.iso'),
    mounted = !!(data.mounted || data.attached || name === mountedName),
    anotherMounted = !!mountedName && !mounted,
    saved = mediaChoices.get(name) || {},
    defaultType = isIso ? 'cdrom' : 'disk',
    type = mounted
      ? mediaStatus.media_type || saved.media_type || defaultType
      : saved.media_type || defaultType,
    readOnly =
      isIso || type === 'cdrom'
        ? true
        : mounted
          ? mediaStatus.read_only !== false
          : (saved.read_only ?? shared.bootstrap?.config?.media?.read_only_by_default ?? false);
  const row = document.createElement('article');
  row.className = 'media-item';
  const head = document.createElement('div');
  head.className = 'media-item-head';
  const nameBox = document.createElement('span');
  nameBox.className = 'media-item-name';
  const title = document.createElement('strong');
  title.textContent = name;
  title.title = name;
  const meta = document.createElement('span');
  meta.className = 'media-item-meta';
  const size = formatBytes(data.size);
  if (size) meta.append(document.createTextNode(size));
  if (mounted) {
    const badge = document.createElement('span');
    badge.className = 'media-badge';
    badge.textContent = '已挂载';
    meta.append(badge);
  }
  nameBox.append(title, meta);
  head.append(nameBox);
  const controls = document.createElement('div');
  controls.className = 'media-item-controls';
  const select = document.createElement('select');
  select.className = 'media-type-select media-operation';
  select.setAttribute('aria-label', `${name} 的介质模式`);
  [
    ['auto', '自动'],
    ['cdrom', '光驱'],
    ['disk', 'U 盘'],
  ].forEach(([value, label]) => {
    const option = document.createElement('option');
    option.value = value;
    option.textContent = label;
    select.append(option);
  });
  select.value = type;
  select.dataset.mediaLocked = String(mounted);
  const readonlyLabel = document.createElement('label');
  readonlyLabel.className = 'media-readonly';
  const readonly = document.createElement('input');
  readonly.type = 'checkbox';
  readonly.checked = readOnly;
  readonly.dataset.mediaLocked = String(isIso || mounted || type === 'cdrom');
  readonly.className = 'media-operation';
  readonly.setAttribute('aria-label', `${name} 只读`);
  readonlyLabel.append(readonly, document.createTextNode('只读'));
  if (isIso) readonlyLabel.title = 'ISO 始终以只读方式挂载';
  select.addEventListener('change', () => {
    if (select.value === 'cdrom') {
      readonly.checked = true;
      readonly.dataset.mediaLocked = 'true';
    } else readonly.dataset.mediaLocked = String(isIso || mounted);
    mediaChoices.set(name, { media_type: select.value, read_only: readonly.checked });
    syncMediaBusy();
  });
  readonly.addEventListener('change', () =>
    mediaChoices.set(name, { media_type: select.value, read_only: readonly.checked }),
  );
  controls.append(select, readonlyLabel);
  if (!mounted) {
    const attach = document.createElement('button');
    attach.type = 'button';
    attach.className = 'primary media-attach media-operation';
    attach.textContent = '挂载';
    attach.dataset.mediaLocked = String(anotherMounted);
    if (anotherMounted) attach.title = '请先弹出当前介质';
    attach.addEventListener('click', () => {
      const payload = {
        name,
        media_type: select.value,
        read_only: isIso || select.value === 'cdrom' ? true : readonly.checked,
      };
      mediaChoices.set(name, { media_type: payload.media_type, read_only: payload.read_only });
      runMediaAction(
        () => request('/api/media/attach', { method: 'POST', body: JSON.stringify(payload) }),
        '镜像已挂载',
      );
    });
    controls.append(attach);
  }
  row.append(head, controls);
  return row;
}
function renderMedia(data) {
  const box = $('#media-list'),
    items = mediaItems(data),
    status = data?.status || {};
  mediaStatus = status;
  const mountedName = mountedMediaName(status, items);
  renderMediaStatus(status, items);
  box.replaceChildren();
  if (!items.length) {
    box.textContent = '暂无镜像';
    syncMediaBusy();
    return;
  }
  items.forEach((item) => box.append(renderMediaItem(item, mountedName)));
  syncMediaBusy();
}
const mediaTicket = latestOnly();
async function refreshMedia() {
  const isCurrent = mediaTicket();
  const ownsBusy = !mediaBusy;
  if (ownsBusy) {
    mediaBusy = true;
    syncMediaBusy();
  }
  try {
    const media = await request('/api/media');
    if (isCurrent()) renderMedia(media);
  } catch (error) {
    if (!isCurrent()) return;
    mediaServerBusy = false;
    $('#media-list').textContent = error.message;
    const status = $('#media-status');
    status.className = 'media-status error';
    status.innerHTML =
      '<span class="status-dot"></span><span class="media-status-copy"><strong>状态不可用</strong></span>';
  } finally {
    if (ownsBusy) {
      mediaBusy = false;
      syncMediaBusy();
    }
  }
}

// URL downloads run as a server-side background job. Polling continues even
// if the user closes the inspector, so the image still lands on the device.
let mediaDownloadId = '',
  mediaDownloadTimer = 0,
  mediaDownloadRate = 0,
  mediaDownloadLastAt = 0,
  mediaDownloadLastReceived = 0;
function setMediaDownloadProgress(value, text) {
  const bar = $('#media-download-bar'),
    progress = bar.parentElement;
  const percent = Number.isFinite(value) ? Math.max(0, Math.min(100, value)) : 0;
  bar.style.width = `${percent}%`;
  progress.setAttribute('aria-valuenow', String(Math.round(percent)));
  if (text !== undefined) $('#media-download-stats').textContent = text;
}
function stopMediaDownloadPolling() {
  clearTimeout(mediaDownloadTimer);
  mediaDownloadTimer = 0;
}
function finishMediaDownload(state, error) {
  stopMediaDownloadPolling();
  mediaDownloadId = '';
  mediaDownloadBusy = false;
  const cancel = $('#media-download-cancel');
  cancel.classList.add('hidden');
  cancel.disabled = false;
  if (state === 'done') {
    setMediaDownloadProgress(100, '');
    toast('镜像下载完成');
    $('#media-download-form').reset();
    refreshMedia();
  } else if (state === 'cancelled') {
    setMediaDownloadProgress(0, '');
    toast('下载已取消');
  } else {
    const message = error || '下载失败';
    setMediaDownloadProgress(0, message);
    toast(message, true);
  }
  syncMediaBusy();
}
function renderMediaDownloadProgress(status) {
  const received = Number(status?.received) || 0,
    total = Number(status?.total) || 0,
    now = performance.now(),
    elapsed = (now - mediaDownloadLastAt) / 1000;
  if (elapsed >= 0.5) {
    const instant = (received - mediaDownloadLastReceived) / elapsed;
    if (instant >= 0) mediaDownloadRate = mediaDownloadRate ? mediaDownloadRate * 0.7 + instant * 0.3 : instant;
    mediaDownloadLastAt = now;
    mediaDownloadLastReceived = received;
  }
  const receivedText = formatBytes(received),
    speed = mediaDownloadRate > 0 ? `${formatBytes(mediaDownloadRate)}/s` : '';
  let text;
  if (total > 0) {
    const percent = Math.round((received / total) * 100);
    text = speed
      ? tr(`已下载 ${receivedText} / ${formatBytes(total)} · 速度 ${speed}`)
      : tr(`已下载 ${receivedText} / ${formatBytes(total)} · ${percent}%`);
    setMediaDownloadProgress(percent, text);
  } else {
    text = speed ? tr(`已下载 ${receivedText} · 速度 ${speed}`) : tr(`已下载 ${receivedText}`);
    setMediaDownloadProgress(0, text);
  }
}
async function pollMediaDownload() {
  if (!mediaDownloadId) return;
  try {
    const status = await request(`/api/media/download/${encodeURIComponent(mediaDownloadId)}`);
    renderMediaDownloadProgress(status);
    if (status.state === 'running') {
      mediaDownloadTimer = setTimeout(pollMediaDownload, 700);
      return;
    }
    finishMediaDownload(status.state, status.error);
  } catch (error) {
    finishMediaDownload('failed', error.message);
  }
}

// DOM wiring; main.js calls this once, in the original fragment order.
export function init() {
  $('#media-refresh').addEventListener('click', refreshMedia);
  $('#media-upload').addEventListener('submit', (event) => {
    event.preventDefault();
    if (mediaBusy || mediaDownloadBusy) return;
    const form = event.currentTarget,
      bar = $('#upload-bar'),
      progress = bar.parentElement,
      xhr = new XMLHttpRequest(),
      // Capture the fields before syncMediaBusy() disables them: disabled
      // controls are omitted from FormData, which would upload nothing.
      body = new FormData(form);
    mediaBusy = true;
    syncMediaBusy();
    xhr.open('POST', '/api/media/upload');
    // Smoothed throughput so the speed and remaining time do not jitter.
    const stats = $('#upload-stats'),
      cancel = $('#upload-cancel');
    let lastAt = performance.now(),
      lastLoaded = 0,
      rate = 0;
    cancel.classList.remove('hidden');
    cancel.onclick = () => xhr.abort();
    xhr.upload.onprogress = (e) => {
      if (!e.lengthComputable) return;
      const value = Math.round((e.loaded / e.total) * 100);
      bar.style.width = `${value}%`;
      progress.setAttribute('aria-valuenow', String(value));
      const now = performance.now(),
        elapsed = (now - lastAt) / 1000;
      if (elapsed >= 0.5) {
        const instant = (e.loaded - lastLoaded) / elapsed;
        rate = rate ? rate * 0.7 + instant * 0.3 : instant;
        lastAt = now;
        lastLoaded = e.loaded;
      }
      if (rate > 0) {
        const left = Math.max(0, Math.round((e.total - e.loaded) / rate)),
          clock = `${String(Math.floor(left / 60)).padStart(2, '0')}:${String(left % 60).padStart(2, '0')}`;
        stats.textContent = `${value}% · ${formatBytes(rate)}/s · 剩余 ${clock}`;
      } else stats.textContent = `${value}%`;
    };
    const finish = async (success, message) => {
      bar.style.width = '0';
      progress.setAttribute('aria-valuenow', '0');
      stats.textContent = '';
      cancel.classList.add('hidden');
      cancel.onclick = null;
      if (success) {
        toast('镜像上传完成');
        form.reset();
        await refreshMedia();
      } else toast(message || '上传失败', true);
      mediaBusy = false;
      syncMediaBusy();
    };
    xhr.onload = () => {
      let message = xhr.responseText;
      try {
        const data = JSON.parse(message);
        message = data.error || data.message || message;
      } catch {}
      finish(xhr.status >= 200 && xhr.status < 300, message);
    };
    xhr.onerror = () => finish(false, '上传连接失败');
    xhr.onabort = () => finish(false, '上传已取消');
    xhr.send(body);
  });
  $('#media-download-form').addEventListener('submit', async (event) => {
    event.preventDefault();
    if (mediaBusy || mediaDownloadBusy) return;
    const form = event.currentTarget,
      url = form.elements.url.value.trim(),
      sha256 = form.elements.sha256.value.trim();
    if (!/^https?:\/\//i.test(url)) {
      toast('只支持 http 或 https 下载地址', true);
      return;
    }
    if (sha256 && !/^[0-9a-f]{64}$/i.test(sha256)) {
      toast('SHA256 必须是 64 位十六进制字符串', true);
      return;
    }
    mediaDownloadBusy = true;
    mediaDownloadRate = 0;
    mediaDownloadLastAt = performance.now();
    mediaDownloadLastReceived = 0;
    setMediaDownloadProgress(0, tr('正在下载…'));
    syncMediaBusy();
    try {
      const payload = { url };
      if (sha256) payload.sha256 = sha256;
      const status = await request('/api/media/download', {
        method: 'POST',
        body: JSON.stringify(payload),
      });
      mediaDownloadId = status.id;
      const cancel = $('#media-download-cancel');
      cancel.classList.remove('hidden');
      cancel.disabled = false;
      renderMediaDownloadProgress(status);
      pollMediaDownload();
    } catch (error) {
      finishMediaDownload('failed', error.message);
    }
  });
  $('#media-download-cancel').addEventListener('click', async (event) => {
    if (!mediaDownloadId) return;
    const button = event.currentTarget;
    button.disabled = true;
    try {
      await request('/api/media/download/cancel', { method: 'POST' });
      $('#media-download-stats').textContent = tr('正在取消…');
    } catch (error) {
      button.disabled = false;
      toast(error.message, true);
    }
  });

  // Drag an image anywhere onto the media panel to upload it.
  {
    const panel = $('#panel-media'),
      form = $('#media-upload');
    const isFileDrag = (event) => [...(event.dataTransfer?.types || [])].includes('Files');
    panel?.addEventListener('dragover', (event) => {
      if (!isFileDrag(event)) return;
      event.preventDefault();
      panel.classList.add('drop-target');
    });
    panel?.addEventListener('dragleave', (event) => {
      if (!panel.contains(event.relatedTarget)) panel.classList.remove('drop-target');
    });
    panel?.addEventListener('drop', (event) => {
      if (!isFileDrag(event)) return;
      event.preventDefault();
      panel.classList.remove('drop-target');
      const file = event.dataTransfer.files[0];
      if (!file) return;
      if (!/\.(iso|img)$/i.test(file.name)) {
        toast('只支持 ISO 或 IMG 镜像', true);
        return;
      }
      const transfer = new DataTransfer();
      transfer.items.add(file);
      form.elements.file.files = transfer.files;
      form.requestSubmit();
    });
  }
}

export { refreshMedia };
