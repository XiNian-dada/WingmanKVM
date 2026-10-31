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
    $$('#media-upload input,#media-upload button').forEach((control) => (control.disabled = mediaBusy));
    $('#media-refresh').disabled = mediaBusy;
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
    const detail = document.createElement('span');
    if (mounted) {
      const parts = [mediaTypeLabel(status?.media_type), status?.read_only === false ? '读写' : '只读'];
      const stateText = mediaStateLabel(state);
      if (stateText && stateText !== '已挂载') parts.unshift(stateText);
      detail.textContent = parts.join(' · ');
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
          !confirm('请先在目标机中弹出此 U 盘。确认已经弹出并继续吗？')
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
          if (!confirm('强制弹出可能损坏镜像文件系统，确定继续吗？')) return;
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
            : (saved.read_only ?? bootstrap?.config?.media?.read_only_by_default ?? false);
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
  $('#media-refresh').addEventListener('click', refreshMedia);
  $('#media-upload').addEventListener('submit', (event) => {
    event.preventDefault();
    if (mediaBusy) return;
    const form = event.currentTarget,
      bar = $('#upload-bar'),
      progress = bar.parentElement,
      xhr = new XMLHttpRequest();
    mediaBusy = true;
    syncMediaBusy();
    xhr.open('POST', '/api/media/upload');
    xhr.upload.onprogress = (e) => {
      if (!e.lengthComputable) return;
      const value = Math.round((e.loaded / e.total) * 100);
      bar.style.width = `${value}%`;
      progress.setAttribute('aria-valuenow', String(value));
    };
    const finish = async (success, message) => {
      bar.style.width = '0';
      progress.setAttribute('aria-valuenow', '0');
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
    xhr.send(new FormData(form));
  });

