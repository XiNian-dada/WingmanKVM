  // HDMI audio (issue #71). Audio only exists on the WebRTC transport: the
  // Opus track joins the video's MediaStream, so the <video> element plays
  // both and muting is just its `muted` flag. Playback always starts muted to
  // satisfy browser autoplay rules. MJPEG carries no audio, and the control
  // says so instead of silently disappearing. Nothing here can end or restart
  // the video: a missing or failing audio track only changes this control.
  const AUDIO_STATE_TEXT = {
    disabled: '音频未启用',
    no_device: '未检测到音频设备',
    idle: '音频就绪（无人收听）',
    starting: '音频正在启动',
    running: '音频运行中',
    error: '音频采集异常',
  };
  function audioConfigured() {
    return bootstrap.capabilities?.audio === true;
  }
  function audioStatusText(status = latestAudioStatus) {
    if (!audioConfigured() && (!status || status.state === 'disabled')) return tr('音频未启用');
    if (!status) return tr('读取中…');
    let text = tr(AUDIO_STATE_TEXT[status.state] || '音频状态未知');
    if (status.device) text += ` · ${status.device}`;
    if (status.message && status.state !== 'no_device') text += ` · ${tr(status.message)}`;
    return text;
  }
  // [state, reason]: state is one of hidden, unavailable, connecting, muted,
  // unmuted; the reason is the tooltip and the toast for an unusable control.
  function audioControlState() {
    if (!audioConfigured()) return ['hidden', ''];
    if (activeVideoTransport === 'mjpeg') return ['unavailable', 'MJPEG 模式不支持音频'];
    if (activeVideoTransport !== 'webrtc_h264') return ['unavailable', '音频仅在 H.264 (WebRTC) 模式可用'];
    if (webrtcAudio === null) return ['connecting', '正在连接音频…'];
    if (!webrtcAudio)
      return ['unavailable', `音频不可用：${latestAudioStatus?.message || '未检测到音频采集设备'}`];
    return [audioMuted ? 'muted' : 'unmuted', ''];
  }
  function syncAudioUi() {
    const button = $('#audio-toggle');
    if (!button) return;
    const [state, reason] = audioControlState(),
      active = state === 'muted' || state === 'unmuted',
      failing = active && latestAudioStatus?.state === 'error';
    button.classList.toggle('hidden', state === 'hidden');
    button.dataset.audioState = state === 'hidden' ? 'unavailable' : state;
    button.setAttribute('aria-disabled', String(!active));
    button.setAttribute('aria-pressed', String(state === 'muted'));
    button.classList.toggle('audio-error', failing);
    const title = active
      ? failing
        ? `音频采集异常：${latestAudioStatus?.message || '正在重试'}`
        : audioMuted
          ? '已静音，点击取消静音'
          : '声音已开启，点击静音'
      : reason;
    button.title = title;
    button.setAttribute('aria-label', active ? '静音' : reason);
    // The element only ever plays sound for a negotiated audio track.
    webrtcFeed.muted = !active || audioMuted;
  }
  function renderAudioStatus() {
    const text = audioStatusText(),
      line = $('#audio-config-status'),
      diag = $('#diag-audio');
    if (line) line.textContent = text;
    if (diag) diag.textContent = text;
  }
  // Keep the configured device selectable even when it is not detected now.
  function populateAudioDevices(candidates) {
    const select = $('#video-config')?.elements.audio_device;
    if (!select || !Array.isArray(candidates)) return;
    audioCandidates = candidates;
    const wanted = select.dataset.configured ?? select.value;
    while (select.options.length > 1) select.remove(1);
    const seen = new Set();
    for (const candidate of candidates) {
      const device = candidate?.device;
      if (!device || seen.has(device)) continue;
      seen.add(device);
      const option = document.createElement('option');
      option.value = device;
      option.textContent = candidate.capture_card
        ? `${candidate.label || device} (${candidate.capture_card})`
        : candidate.label || device;
      option.dataset.noI18n = '';
      select.append(option);
    }
    if (wanted && !seen.has(wanted)) {
      const option = document.createElement('option');
      option.value = wanted;
      option.textContent = wanted;
      option.dataset.noI18n = '';
      select.append(option);
    }
    select.value = wanted || '';
  }
  function applyAudioConfig(audio = {}) {
    const form = $('#video-config');
    if (!form?.elements.audio_enabled) return;
    form.elements.audio_enabled.checked = !!audio.enabled;
    form.elements.audio_device.dataset.configured = audio.device || '';
    populateAudioDevices(audioCandidates);
    renderAudioStatus();
    syncAudioUi();
  }
  async function scanAudioDevices() {
    const button = $('#audio-scan');
    if (!button || button.disabled) return;
    const label = button.textContent;
    button.disabled = true;
    button.textContent = '检测中…';
    try {
      const data = await request('/api/devices/scan', { method: 'POST' }),
        candidates = (data?.devices || data || {}).audio || [];
      populateAudioDevices(candidates);
      toast(candidates.length ? `检测到 ${candidates.length} 个音频采集设备` : '未检测到音频采集设备');
    } catch (e) {
      toast(e.message, true);
    } finally {
      button.disabled = false;
      button.textContent = label;
    }
  }
  $('#audio-scan')?.addEventListener('click', scanAudioDevices);
  $('#video-config')?.elements.audio_device?.addEventListener('change', (event) => {
    event.currentTarget.dataset.configured = event.currentTarget.value;
  });
  $('#audio-toggle')?.addEventListener('click', () => {
    const [state, reason] = audioControlState();
    if (state !== 'muted' && state !== 'unmuted') {
      toast(reason || '音频不可用', true);
      return;
    }
    audioMuted = !audioMuted;
    syncAudioUi();
    toast(audioMuted ? '已静音' : '已取消静音');
    if (!audioMuted)
      webrtcFeed.play().catch(() => {
        audioMuted = true;
        syncAudioUi();
        toast('浏览器阻止了声音播放，请再次点击', true);
      });
  });
