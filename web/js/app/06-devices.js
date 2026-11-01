  function gpioPulseFrom(form) {
    const chip = value(form, 'reset_gpio_chip'),
      rawLine = value(form, 'reset_gpio_line');
    if (!chip && !rawLine) return null;
    if (!chip || !rawLine) throw new Error('RESET SW 需要芯片和线路');
    return {
      gpio_chip: chip,
      gpio_line: Number(rawLine),
      active_high: value(form, 'reset_active_high') !== 'false',
      pulse_ms: optionalNumber(value(form, 'reset_pulse_ms')) ?? 500,
    };
  }
  function gpioInputFrom(form) {
    const chip = value(form, 'power_led_gpio_chip'),
      rawLine = value(form, 'power_led_gpio_line');
    if (!chip && !rawLine) return null;
    if (!chip || !rawLine) throw new Error('PWR LED 需要芯片和线路');
    return {
      gpio_chip: chip,
      gpio_line: Number(rawLine),
      active_low: value(form, 'power_led_active_low') !== 'false',
      bias: value(form, 'power_led_bias') || 'pull_up',
      poll_interval_ms: optionalNumber(value(form, 'power_led_poll_interval_ms')) ?? 1000,
      debounce_ms: optionalNumber(value(form, 'power_led_debounce_ms')) ?? 50,
    };
  }
  $('#device-config').addEventListener('submit', (event) => {
    event.preventDefault();
    const f = event.currentTarget;
    submitOnce(f, async () => {
      try {
        const payload = {
          hid: {
            keyboard_device: value(f, 'keyboard_device') || null,
            mouse_device: value(f, 'mouse_device') || null,
            absolute_pointer_device: value(f, 'absolute_pointer_device') || null,
            pointer_mode: value(f, 'pointer_mode'),
          },
          power: {
            enabled: f.elements.power_enabled.checked,
            gpio_chip: value(f, 'gpio_chip') || null,
            gpio_line: optionalNumber(value(f, 'gpio_line')),
            active_high: value(f, 'active_high') !== 'false',
            reset_switch: gpioPulseFrom(f),
            power_led: gpioInputFrom(f),
          },
        };
        await request('/api/config', { method: 'POST', body: JSON.stringify(payload) });
        bootstrap = await request('/api/bootstrap');
        applyBootstrap();
        formStatus(f, true);
      } catch (e) {
        formStatus(f, false, e.message);
        toast(e.message, true);
      }
    });
  });
  $('#media-config').addEventListener('submit', (event) => {
    event.preventDefault();
    submitOnce(event.currentTarget, async () => {
      const f = event.currentTarget,
        payload = {
          media: {
            enabled: f.elements.enabled.checked,
            lun_path: value(f, 'lun_path') || null,
            image_directory: value(f, 'image_directory') || null,
          },
        };
      try {
        await request('/api/config', { method: 'POST', body: JSON.stringify(payload) });
        formStatus(f, true);
        await refreshMedia();
      } catch (e) {
        formStatus(f, false, e.message);
        toast(e.message, true);
      }
    });
  });
  const scanTicket = latestOnly();
  async function scanDevices(target = $('#device-results')) {
    const isCurrent = scanTicket();
    const setupTarget = target.id === 'setup-device-results',
      setupVisible = !setupForm.classList.contains('hidden');
    if (setupTarget) renderSetupDetection(null, true);
    else renderDeviceDetection(null, true);
    try {
      const data = await request(setupVisible ? '/api/setup/devices' : '/api/devices/scan', {
        method: 'POST',
      });
      if (!isCurrent()) return data;
      fillDetected(data);
      if (setupTarget) renderSetupDetection(data);
      else renderDeviceDetection(data);
      const diagnostics = $('#device-diagnostics');
      if (diagnostics) diagnostics.textContent = JSON.stringify(data, null, 2);
      return data;
    } catch (e) {
      if (!isCurrent()) return;
      if (setupTarget) renderSetupDetection({ error: e.message });
      else renderDeviceDetection({ error: e.message });
      toast(e.message, true);
    }
  }
  function firstPath(v) {
    if (typeof v === 'string') return v;
    if (Array.isArray(v)) return firstPath(v[0]);
    return v?.path || v?.device || '';
  }
  function firstLabel(v) {
    if (Array.isArray(v)) return firstLabel(v[0]);
    return v?.label || firstPath(v);
  }
  function asCandidates(v) {
    return (Array.isArray(v) ? v : v ? [v] : []).filter(Boolean);
  }
  function candidateReady(candidate, kind = 'gadget') {
    if (!firstPath(candidate)) return false;
    if (typeof candidate === 'string') return kind !== 'video';
    if (kind === 'video') return candidate.video_capture === true && candidate.supports_mjpeg === true;
    return (
      candidate.compatible === true && candidate.gadget_bound === true && candidate.function_linked === true
    );
  }
  function readyCandidates(v, kind) {
    return asCandidates(v).filter((candidate) => candidateReady(candidate, kind));
  }
  function candidateGroups(root) {
    return {
      video: readyCandidates(root.video || root.video_devices, 'video'),
      keyboard: readyCandidates(root.keyboard || root.keyboards),
      mouse: readyCandidates(root.mouse || root.mice),
      absolute: readyCandidates(root.absolute_pointer || root.absolute_pointers),
      luns: readyCandidates(root.mass_storage_luns),
      gpio: asCandidates(root.gpio_chip || root.gpio).filter((candidate) => !!firstPath(candidate)),
    };
  }
  function uniquePath(candidates) {
    return candidates.length === 1 ? firstPath(candidates[0]) : '';
  }
  function selectedPath(field, candidates) {
    const selected = value(setupForm, field);
    return candidates.some((candidate) => firstPath(candidate) === selected) ? selected : '';
  }
  function setIfEmpty(input, next) {
    if (input && next != null && next !== '' && !input.value) input.value = next;
  }
  function setAutoValue(input, next) {
    if (!input || input.dataset.userEdited) return;
    input.value = next || '';
    input.dataset.autoFilled = next ? 'true' : '';
  }
  let lastSetupDiscovery = null;
  function fillDetected(data) {
    const root = data?.devices || data || {},
      setup = !setupForm.classList.contains('hidden') ? setupForm : null,
      device = $('#device-config'),
      media = $('#media-config'),
      groups = candidateGroups(root);
    const detected = {
      video: uniquePath(groups.video),
      keyboard: uniquePath(groups.keyboard),
      mouse: uniquePath(groups.mouse),
      absolute: uniquePath(groups.absolute),
      lun: uniquePath(groups.luns),
      gpio: uniquePath(groups.gpio),
    };
    if (setup) {
      lastSetupDiscovery = root;
      setAutoValue(setup.elements.video_device, detected.video);
      setAutoValue(setup.elements.keyboard_device, detected.keyboard);
      setAutoValue(setup.elements.mouse_device, detected.mouse);
      setAutoValue(setup.elements.absolute_pointer_device, detected.absolute);
      setAutoValue(setup.elements.gpio_chip, detected.gpio);
      setAutoValue(setup.elements.lun_path, detected.lun);
      setAutoValue(setup.elements.image_directory, '/var/lib/wingmankvm/images');
      if (!setup.elements.pointer_mode.dataset.userEdited)
        setup.elements.pointer_mode.value = detected.absolute
          ? 'absolute'
          : detected.mouse
            ? 'relative'
            : 'absolute';
      if (!setup.elements.media_enabled.dataset.userEdited)
        setup.elements.media_enabled.checked = !!detected.lun;
      populateGpioOptions(groups.gpio);
    }
    setIfEmpty(device.elements.keyboard_device, detected.keyboard);
    setIfEmpty(device.elements.mouse_device, detected.mouse);
    setIfEmpty(device.elements.absolute_pointer_device, detected.absolute);
    setIfEmpty(media.elements.lun_path, detected.lun);
    setIfEmpty(media.elements.image_directory, '/var/lib/wingmankvm/images');
    if (Array.isArray(root.audio)) populateAudioDevices(root.audio);
    updateSetupCapabilities();
  }
  function populateGpioOptions(candidates) {
    const list = $('#setup-gpio-options');
    list.replaceChildren();
    for (const candidate of candidates) {
      const option = document.createElement('option');
      option.value = firstPath(candidate);
      option.label = firstLabel(candidate);
      list.append(option);
    }
  }
  function candidateSelect(label, field, candidates) {
    const select = document.createElement('select'),
      placeholder = document.createElement('option');
    placeholder.value = '';
    placeholder.textContent = `${label} · 请选择`;
    select.append(placeholder);
    for (const candidate of candidates) {
      const option = document.createElement('option');
      option.value = firstPath(candidate);
      option.textContent = firstLabel(candidate);
      option.dataset.noI18n = '';
      select.append(option);
    }
    select.value = value(setupForm, field);
    select.addEventListener('change', () => {
      const input = setupForm.elements[field];
      input.value = select.value;
      input.dataset.userEdited = 'true';
      renderSetupDetection(lastSetupDiscovery);
      updateSetupCapabilities();
    });
    return select;
  }
  function detectionCard(label, state, detail, selections = [], guide = '') {
    const card = document.createElement('div');
    card.className = 'capability';
    const dot = document.createElement('span');
    dot.className = `status-dot${state === 'online' ? ' online' : state === 'warning' ? ' warning' : ''}`;
    const heading = document.createElement('span');
    heading.className = 'capability-heading';
    const title = document.createElement('strong');
    title.textContent = label;
    heading.append(title);
    if (guide) {
      const help = document.createElement('button');
      help.type = 'button';
      help.className = 'guide-help';
      help.dataset.guide = guide;
      help.setAttribute('aria-label', `${label}说明`);
      help.setAttribute('aria-controls', 'auth-guide');
      help.setAttribute('aria-expanded', 'false');
      help.textContent = '?';
      heading.append(help);
    }
    const copy = document.createElement('p');
    copy.textContent = detail;
    copy.title = detail;
    card.append(dot, heading, copy);
    for (const selection of selections) {
      if (selection.candidates.length > 1)
        card.append(candidateSelect(selection.label, selection.field, selection.candidates));
    }
    return card;
  }
  function renderDeviceDetection(data, loading = false) {
    const box = $('#device-results');
    box.replaceChildren();
    if (loading) {
      box.append(detectionCard('正在检测', 'warning', '请稍候'));
      return;
    }
    if (data?.error) {
      box.append(detectionCard('检测失败', '', data.error));
      return;
    }
    const root = data?.devices || data || {},
      groups = candidateGroups(root),
      caps = bootstrap.capabilities || {},
      videoFound = groups.video.length > 0,
      videoLabel =
        groups.video.length === 1 ? firstLabel(groups.video[0]) : `${groups.video.length} 个可用设备`,
      inputFound = groups.keyboard.length > 0 && (groups.mouse.length > 0 || groups.absolute.length > 0),
      mediaFound = groups.luns.length > 0,
      powerFound = groups.gpio.length > 0,
      audioCards = asCandidates(root.audio).filter((candidate) => !!candidate.device),
      audioFound = audioCards.length > 0,
      audioLabel =
        audioCards.length === 1 ? audioCards[0].label || audioCards[0].device : `${audioCards.length} 个可用设备`;
    box.append(
      detectionCard(
        '视频',
        caps.video ? (videoFound ? 'online' : 'warning') : videoFound ? 'warning' : '',
        caps.video ? (videoFound ? videoLabel : '已配置，当前未检测到') : videoFound ? '可选择' : '未配置',
      ),
      detectionCard(
        '键盘与鼠标',
        caps.keyboard && caps.mouse ? (inputFound ? 'online' : 'warning') : inputFound ? 'warning' : '',
        caps.keyboard && caps.mouse
          ? inputFound
            ? caps.pointer_mode === 'absolute'
              ? '绝对指针'
              : '相对鼠标'
            : '已配置，当前未检测到'
          : inputFound
            ? '可选择'
            : '未配置',
      ),
      detectionCard(
        '虚拟介质',
        caps.mass_storage ? (mediaFound ? 'online' : 'warning') : mediaFound ? 'warning' : '',
        caps.mass_storage
          ? mediaFound
            ? '已就绪'
            : '已配置，当前未检测到'
          : mediaFound
            ? '可启用'
            : '未配置',
      ),
      detectionCard(
        '电源',
        caps.gpio_power ? (powerFound ? 'online' : 'warning') : powerFound ? 'warning' : '',
        caps.gpio_power
          ? powerFound
            ? '已配置'
            : '已配置，当前未检测到'
          : powerFound
            ? '线路待确认'
            : '未配置',
      ),
      detectionCard(
        'HDMI 音频',
        caps.audio ? (audioFound ? 'online' : 'warning') : '',
        caps.audio ? (audioFound ? audioLabel : '已启用，当前未检测到') : audioFound ? '可启用' : '未启用',
      ),
    );
  }
  function renderSetupDetection(data, loading = false) {
    const box = $('#setup-device-results');
    box.replaceChildren();
    if (loading) {
      box.append(detectionCard('正在检测', 'warning', '请稍候'));
      return;
    }
    if (data?.error) {
      box.append(detectionCard('检测失败', '', data.error));
      return;
    }
    const root = data?.devices || data || lastSetupDiscovery || {},
      groups = candidateGroups(root);
    lastSetupDiscovery = root;
    const video = selectedPath('video_device', groups.video),
      keyboard = selectedPath('keyboard_device', groups.keyboard),
      mouse = selectedPath('mouse_device', groups.mouse),
      absolute = selectedPath('absolute_pointer_device', groups.absolute),
      pointerMode = value(setupForm, 'pointer_mode') === 'relative' ? 'relative' : 'absolute',
      pointer = pointerMode === 'relative' ? mouse : absolute,
      lun = selectedPath('lun_path', groups.luns),
      gpio = value(setupForm, 'gpio_chip');
    const videoState = video ? 'online' : groups.video.length ? 'warning' : '';
    const inputState =
      keyboard && pointer
        ? 'online'
        : groups.keyboard.length && (groups.mouse.length || groups.absolute.length)
          ? 'warning'
          : '';
    const mediaState = lun ? 'online' : groups.luns.length ? 'warning' : '';
    const gpioKnown = groups.gpio.some((candidate) => firstPath(candidate) === gpio);
    box.append(
      detectionCard(
        '视频',
        videoState,
        video
          ? firstLabel(groups.video.find((item) => firstPath(item) === video))
          : groups.video.length
            ? '请选择采集卡'
            : '未发现可用采集卡',
        [{ label: '视频', field: 'video_device', candidates: groups.video }],
        'video',
      ),
      detectionCard(
        'USB 控制',
        inputState,
        inputState === 'online'
          ? pointerMode === 'absolute'
            ? '绝对指针已就绪'
            : '相对鼠标已就绪'
          : groups.keyboard.length && (groups.mouse.length || groups.absolute.length)
            ? '请选择与指针模式匹配的设备'
            : '未发现完整键鼠',
        [
          { label: '键盘', field: 'keyboard_device', candidates: groups.keyboard },
          { label: '相对鼠标', field: 'mouse_device', candidates: groups.mouse },
          { label: '绝对指针', field: 'absolute_pointer_device', candidates: groups.absolute },
        ],
        'input',
      ),
      detectionCard(
        '虚拟介质',
        mediaState,
        lun ? '已就绪' : groups.luns.length ? '请选择接口' : '未配置',
        [{ label: '虚拟介质', field: 'lun_path', candidates: groups.luns }],
        'media',
      ),
      detectionCard(
        '电源',
        gpioKnown ? 'warning' : '',
        gpioKnown ? '检测到控制器，仍需选择线路' : groups.gpio.length ? '请选择控制器' : '稍后配置',
        [{ label: 'GPIO', field: 'gpio_chip', candidates: groups.gpio }],
        'power',
      ),
    );
  }
  function updateSetupCapabilities() {
    const groups = candidateGroups(lastSetupDiscovery || {}),
      video = selectedPath('video_device', groups.video),
      keyboard = selectedPath('keyboard_device', groups.keyboard),
      pointerMode = value(setupForm, 'pointer_mode') === 'relative' ? 'relative' : 'absolute',
      pointer =
        pointerMode === 'relative'
          ? selectedPath('mouse_device', groups.mouse)
          : selectedPath('absolute_pointer_device', groups.absolute),
      lun = selectedPath('lun_path', groups.luns),
      gpio = value(setupForm, 'gpio_chip'),
      line = value(setupForm, 'gpio_line'),
      powerReady = !!gpio && line !== '' && groups.gpio.some((candidate) => firstPath(candidate) === gpio),
      mediaReady = !!lun;
    const powerToggle = setupForm.elements.power_enabled,
      mediaToggle = setupForm.elements.media_enabled;
    if (!powerReady) powerToggle.checked = false;
    if (!mediaReady) mediaToggle.checked = false;
    powerToggle.disabled = !powerReady;
    mediaToggle.disabled = !mediaReady;
    const missing = [];
    if (!video) missing.push('视频');
    if (!keyboard || !pointer) missing.push('键鼠');
    const readiness = $('#setup-readiness'),
      submit = $('#setup-submit');
    readiness.textContent = missing.length ? `${missing.join('、')}可稍后配置` : '推荐配置已就绪';
    submit.textContent = missing.length ? '仍然进入控制台' : '进入控制台';
  }
  [
    'video_device',
    'keyboard_device',
    'mouse_device',
    'absolute_pointer_device',
    'pointer_mode',
    'gpio_chip',
    'gpio_line',
    'lun_path',
    'image_directory',
  ].forEach((name) =>
    setupForm.elements[name].addEventListener('input', (event) => {
      if (event.isTrusted) event.target.dataset.userEdited = 'true';
      if (name === 'pointer_mode') renderSetupDetection(lastSetupDiscovery);
      if (name === 'pointer_mode' || name === 'gpio_chip' || name === 'gpio_line') updateSetupCapabilities();
    }),
  );
  setupForm.elements.media_enabled.addEventListener('change', (event) => {
    event.target.dataset.userEdited = 'true';
    updateSetupCapabilities();
  });
  setupForm.elements.power_enabled.addEventListener('change', (event) => {
    event.target.dataset.userEdited = 'true';
    updateSetupCapabilities();
  });
  $('#scan-devices').addEventListener('click', () => scanDevices());
  $('#setup-scan').addEventListener('click', () => scanDevices($('#setup-device-results')));
  $('#media-scan').addEventListener('click', async () => {
    const button = $('#media-scan'),
      label = button.textContent;
    button.disabled = true;
    button.textContent = '检测中…';
    try {
      const data = await scanDevices(),
        luns = readyCandidates((data?.devices || data || {}).mass_storage_luns);
      toast(
        value($('#media-config'), 'lun_path')
          ? '已找到虚拟介质'
          : luns.length > 1
            ? '发现多个接口，请选择路径'
            : '未发现可用 LUN',
        !value($('#media-config'), 'lun_path'),
      );
    } finally {
      button.disabled = false;
      button.textContent = label;
    }
  });

