  function unwrapConfig(source) {
    return source?.config || source || {};
  }
  const resolutionPresets = new Set(['3840x2160', '2560x1440', '1920x1080', '1280x720', '720x480']);
  const fpsPresets = new Set(['60', '30', '25', '24', '15']);
  const h264BitratePresets = new Set(['2000', '4000', '8000', '12000']);
  const displayModes = [
    ['fit', 'mode-fit', '适应'],
    ['native', 'mode-native', '1:1'],
    ['fill', 'mode-fill', '填满'],
  ];
  let modeIndex = 0;
  function readVideoUiPreference(name, fallback) {
    try {
      return localStorage.getItem(`wingmankvm.video.${name}`) || fallback;
    } catch {
      return fallback;
    }
  }
  function saveVideoUiPreference(name, value) {
    try {
      localStorage.setItem(`wingmankvm.video.${name}`, value);
    } catch {}
  }
  videoPreference = readVideoUiPreference('transport', 'auto');
  if (!['auto', 'webrtc_h264', 'mjpeg'].includes(videoPreference)) videoPreference = 'auto';
  function browserSupportsH264() {
    if (typeof RTCPeerConnection !== 'function') return false;
    const codecs = globalThis.RTCRtpReceiver?.getCapabilities?.('video')?.codecs;
    return !codecs || codecs.some((codec) => String(codec.mimeType || '').toLowerCase() === 'video/h264');
  }
  function h264Available() {
    return bootstrap.capabilities?.video_webrtc_h264 === true && browserSupportsH264();
  }
  function syncTransportControl() {
    const select = $('#video-config').elements.transport,
      option = select.querySelector('option[value="webrtc_h264"]');
    option.disabled = bootstrap.capabilities?.video_webrtc_h264 === false;
    select.value = videoPreference;
  }
  function setVideoPreference(value) {
    videoPreference = ['auto', 'webrtc_h264', 'mjpeg'].includes(value) ? value : 'auto';
    saveVideoUiPreference('transport', videoPreference);
    syncTransportControl();
    restartVideo();
  }
  function setDisplayScale(mode, remember = false) {
    const nextIndex = Math.max(
        0,
        displayModes.findIndex((item) => item[0] === mode),
      ),
      next = displayModes[nextIndex];
    displayModes.forEach((item) => consoleBox.classList.remove(item[1]));
    modeIndex = nextIndex;
    consoleBox.classList.add(next[1]);
    const modeText = $('#mode-button .mode-text');
    if (modeText) modeText.textContent = next[2];
    else $('#mode-button').textContent = next[2];
    $('#mode-button').title = `切换显示方式：${next[2]} (适应窗口 / 1:1 像素 / 填满窗口)`;
    $('#video-config').elements.display_scale.value = next[0];
    if (remember) saveVideoUiPreference('scale', next[0]);
    const panToggle = $('#pan-toggle');
    if (next[0] === 'fit') {
      panToggle?.classList.add('hidden');
      setPanMode(false);
      fitConsoleToVideo();
    } else if (next[0] === 'native') {
      panToggle?.classList.remove('hidden');
      consoleBox.style.width = '';
      consoleBox.style.height = '';
      if (!consoleUserMoved) {
        consoleBox.style.left = '';
        consoleBox.style.top = '';
      }
      setTimeout(centerNativeViewport, 60);
    } else {
      panToggle?.classList.add('hidden');
      setPanMode(false);
      consoleBox.style.width = '';
      consoleBox.style.height = '';
      if (!consoleUserMoved) {
        consoleBox.style.left = '';
        consoleBox.style.top = '';
      }
    }
    invalidateSurfaceBox();
  }
  $('#pan-toggle')?.addEventListener('click', () => setPanMode(!isPanActive));
  $('#pan-toggle')?.addEventListener('dblclick', () => {
    centerNativeViewport();
    toast('平移: 已居中');
  });
  // Single source of truth for interpolation: the video panel select and the
  // settings dialog toggle both read and write the `render_pixelated` setting.
  function setVideoRendering(rendering, remember = false) {
    const next = rendering === 'smooth' ? 'smooth' : 'pixelated';
    consoleBox.classList.toggle('render-pixelated', next === 'pixelated');
    $('#video-config').elements.rendering.value = next;
    if (remember) setSetting('render_pixelated', next === 'pixelated');
  }
  function syncVideoPresets() {
    const form = $('#video-config'),
      resolution = value(form, 'resolution_preset'),
      fps = value(form, 'fps_preset');
    $$('.resolution-custom', form).forEach((field) =>
      field.classList.toggle('hidden', resolution !== 'custom'),
    );
    $('.fps-custom', form).classList.toggle('hidden', fps !== 'custom');
    if (resolutionPresets.has(resolution)) {
      const [width, height] = resolution.split('x');
      form.elements.width.value = width;
      form.elements.height.value = height;
    } else if (resolution === 'auto') {
      form.elements.width.value = '';
      form.elements.height.value = '';
    }
    if (fpsPresets.has(fps)) form.elements.frames_per_second.value = fps;
    else if (fps === 'auto') form.elements.frames_per_second.value = '';
  }
  for (const name of ['resolution_preset', 'fps_preset'])
    $('#video-config').elements[name].addEventListener('change', syncVideoPresets);
  function syncH264Preset() {
    const form = $('#video-config'),
      preset = value(form, 'h264_bitrate_preset');
    $('.h264-bitrate-custom', form).classList.toggle('hidden', preset !== 'custom');
    if (h264BitratePresets.has(preset)) form.elements.h264_bitrate_kbps.value = preset;
  }
  {
    const form = $('#video-config');
    form.elements.h264_bitrate_preset.addEventListener('change', syncH264Preset);
    form.elements.display_scale.addEventListener('change', (e) => setDisplayScale(e.target.value, true));
    form.elements.rendering.addEventListener('change', (e) => setVideoRendering(e.target.value, true));
    form.elements.jpeg_quality.addEventListener('input', (e) => {
      $('#quality-value').textContent = e.target.value;
    });
  }
  function selectH264Preset(h264) {
    const form = $('#video-config'),
      bitrate = Number(h264?.bitrate_kbps) || 4000;
    form.elements.h264_bitrate_preset.value = h264BitratePresets.has(String(bitrate))
      ? String(bitrate)
      : 'custom';
    form.elements.h264_bitrate_kbps.value = String(bitrate);
    syncH264Preset();
    form.elements.h264_encoder.value = h264?.encoder || 'auto';
    form.elements.h264_max_sessions.value = String(h264?.max_sessions || 1);
    form.elements.h264_allow_software.checked = !!h264?.allow_software;
  }
  function selectVideoPresets(video) {
    const form = $('#video-config'),
      resolution = video.width && video.height ? `${video.width}x${video.height}` : 'auto',
      fps = video.frames_per_second == null ? 'auto' : String(video.frames_per_second);
    form.elements.resolution_preset.value = resolutionPresets.has(resolution)
      ? resolution
      : resolution === 'auto'
        ? 'auto'
        : 'custom';
    form.elements.fps_preset.value = fpsPresets.has(fps) ? fps : fps === 'auto' ? 'auto' : 'custom';
    syncVideoPresets();
  }
  function renderClientInfo(client) {
    if (!client) return;
    const ip = client.ip || client;
    if (!ip) return;
    const port = client.port ? `:${client.port}` : '';
    const isProxy = client.proxy_protocol && client.proxy_protocol !== 'disabled';
    const proxySuffix = isProxy ? ' (PPv2)' : '';
    const diagEl = $('#diag-client-ip');
    if (diagEl) diagEl.textContent = `${ip}${port}${proxySuffix}`;
    const sessionIpEl = $('#session-ip');
    if (sessionIpEl) sessionIpEl.textContent = `(${ip}${proxySuffix})`;
  }
  function applyBootstrap() {
    const config = unwrapConfig(bootstrap),
      video = config.video || {},
      h264 = video.h264 || {},
      hid = config.hid || {},
      power = config.power || {},
      media = config.media || {};
    const vf = $('#video-config'),
      df = $('#device-config'),
      mf = $('#media-config');
    for (const name of ['device', 'width', 'height', 'frames_per_second', 'encoding', 'jpeg_quality'])
      if (video[name] != null && vf.elements[name]) vf.elements[name].value = video[name];
    selectVideoPresets(video);
    selectH264Preset(h264);
    applyAudioConfig(config.audio || {});
    for (const name of ['keyboard_device', 'mouse_device', 'absolute_pointer_device'])
      df.elements[name].value = hid[name] || '';
    df.elements.pointer_mode.value =
      bootstrap.capabilities?.pointer_mode === 'relative' ? 'relative' : 'absolute';
    df.elements.power_enabled.checked = !!power.enabled;
    df.elements.gpio_chip.value = power.gpio_chip ?? '';
    df.elements.gpio_line.value = power.gpio_line ?? '';
    df.elements.active_high.value = power.active_high === false ? 'false' : 'true';
    const reset = power.reset_switch || {},
      led = power.power_led || {};
    df.elements.reset_gpio_chip.value = reset.gpio_chip ?? '';
    df.elements.reset_gpio_line.value = reset.gpio_line ?? '';
    df.elements.reset_active_high.value = reset.active_high === false ? 'false' : 'true';
    df.elements.reset_pulse_ms.value = reset.pulse_ms ?? '';
    df.elements.power_led_gpio_chip.value = led.gpio_chip ?? '';
    df.elements.power_led_gpio_line.value = led.gpio_line ?? '';
    df.elements.power_led_active_low.value = led.active_low === false ? 'false' : 'true';
    df.elements.power_led_bias.value = led.bias || 'pull_up';
    df.elements.power_led_poll_interval_ms.value = led.poll_interval_ms ?? '';
    df.elements.power_led_debounce_ms.value = led.debounce_ms ?? '';
    mf.elements.enabled.checked = !!media.enabled;
    if (media.lun_path != null) mf.elements.lun_path.value = media.lun_path;
    if (media.lun_file != null && !media.lun_path) mf.elements.lun_path.value = media.lun_file;
    if (media.image_directory != null) mf.elements.image_directory.value = media.image_directory;
    $('#quality-value').textContent = vf.elements.jpeg_quality.value;
    $('#session-user').textContent = bootstrap.username || bootstrap.user?.username || '管理员';
    if (bootstrap.client_ip)
      renderClientInfo({
        ip: bootstrap.client_ip,
        proxy_protocol: unwrapConfig(bootstrap).server?.proxy_protocol,
      });
    if (bootstrap.capabilities?.pointer_mode === 'absolute' && relativeCaptured()) document.exitPointerLock();
    syncInputUi();
    syncTransportControl();
    syncGpioControls();
    renderPowerLed(latestPowerStatus);
  }
