  function syncGpioControls() {
    const caps = bootstrap.capabilities || {};
    $$('[data-power-action="press"],[data-power-action="force-off"]').forEach(
      (b) => (b.disabled = caps.gpio_power !== true),
    );
    $$('[data-power-action="reset"]').forEach((b) => (b.disabled = caps.gpio_reset !== true));
    $$('[data-gpio-test="power"]').forEach(
      (button) => (button.disabled = gpioTestBusy || caps.gpio_power !== true),
    );
    $$('[data-gpio-test="reset"]').forEach(
      (button) => (button.disabled = gpioTestBusy || caps.gpio_reset !== true),
    );
    $$('[data-power-led-refresh]').forEach((button) => (button.disabled = caps.gpio_power_led !== true));
  }
  function renderPowerLed(powerStatus) {
    const led = powerStatus?.power_led || {},
      dots = $$('[data-power-led-dot]'),
      states = $$('[data-power-led-state]');
    if (!dots.length || !states.length) return;
    dots.forEach((dot) => dot.classList.remove('online', 'warning', 'error'));
    const show = (label, tone = '') => {
      states.forEach((state) => (state.textContent = label));
      if (tone) dots.forEach((dot) => dot.classList.add(tone));
    };
    if (led.configured === false) {
      show('未配置');
      updateDiagnosticsCard();
      return;
    }
    if (led.sense_error) {
      show('读取失败', 'error');
      updateDiagnosticsCard();
      return;
    }
    const ledState = led.state || (led.active === true ? 'on' : led.active === false ? 'off' : 'unknown');
    if (ledState === 'on') {
      show('亮', 'online');
      updateDiagnosticsCard();
      return;
    }
    if (ledState === 'off') {
      show('灭');
      updateDiagnosticsCard();
      return;
    }
    show('未知', 'warning');
    updateDiagnosticsCard();
  }
  const powerMenuToggle = $('#power-menu-toggle'),
    powerMenu = $('#power-menu');
  powerMenuToggle?.addEventListener('click', (event) => {
    event.stopPropagation();
    const open = !powerMenu.hasAttribute('hidden');
    if (open) {
      powerMenu.setAttribute('hidden', '');
      powerMenuToggle.setAttribute('aria-expanded', 'false');
    } else {
      powerMenu.removeAttribute('hidden');
      powerMenuToggle.setAttribute('aria-expanded', 'true');
      $('#diagnostics-popover')?.setAttribute('hidden', '');
      $('#diagnostics-toggle')?.setAttribute('aria-expanded', 'false');
      $('#lang-menu')?.setAttribute('hidden', '');
      $('#lang-toggle')?.setAttribute('aria-expanded', 'false');
    }
  });
  const langToggle = $('#lang-toggle'),
    langMenu = $('#lang-menu');
  langToggle?.addEventListener('click', (event) => {
    event.stopPropagation();
    const open = !langMenu.hasAttribute('hidden');
    if (open) {
      langMenu.setAttribute('hidden', '');
      langToggle.setAttribute('aria-expanded', 'false');
    } else {
      langMenu.removeAttribute('hidden');
      langToggle.setAttribute('aria-expanded', 'true');
      powerMenu?.setAttribute('hidden', '');
      powerMenuToggle?.setAttribute('aria-expanded', 'false');
      $('#diagnostics-popover')?.setAttribute('hidden', '');
      $('#diagnostics-toggle')?.setAttribute('aria-expanded', 'false');
    }
  });
  $$('[data-lang]').forEach((btn) =>
    btn.addEventListener('click', () => {
      setLanguage(btn.dataset.lang);
      langMenu?.setAttribute('hidden', '');
      langToggle?.setAttribute('aria-expanded', 'false');
    }),
  );
  $('#theme-toggle')?.addEventListener('click', toggleTheme);
  const diagToggle = $('#diagnostics-toggle'),
    diagPopover = $('#diagnostics-popover');
  function updateDiagnosticsCard() {
    if (!diagPopover || diagPopover.hasAttribute('hidden')) return;
    $('#diag-transport').textContent =
      activeVideoTransport === 'webrtc_h264'
        ? 'WebRTC (H.264)'
        : activeVideoTransport === 'mjpeg'
          ? 'MJPEG'
          : '未连接';
    $('#diag-h264-failure-card').classList.toggle('hidden', !videoFallbackReason);
    $('#diag-h264-failure').textContent = videoFallbackReason || '--';
    const w = latestVideoStatus?.width,
      h = latestVideoStatus?.height,
      fps = latestVideoStatus?.frames_per_second;
    $('#diag-resolution').textContent =
      w && h ? `${w} × ${h}${fps ? ` @ ${Math.round(fps)} FPS` : ''}` : '--';
    $('#diag-input-mode').textContent = !inputEnabled()
      ? '已暂停'
      : absoluteMode()
        ? '绝对指针 (USB Tablet)'
        : relativeCaptured()
          ? '相对鼠标 (已捕获)'
          : '相对鼠标 (未捕获)';
    const led = latestPowerStatus?.power_led;
    const ledOn = led?.state === 'on' || led?.active === true;
    const ledOff = led?.state === 'off' || led?.active === false;
    $('#diag-power-led').textContent = ledOn
      ? '已开机 (LED 亮)'
      : ledOff
        ? '已关机 (LED 灭)'
        : led?.configured === false
          ? '未配置'
          : '未知';
    if (latestSystemStatus) renderSystemMetrics(latestSystemStatus);
  }
  diagToggle?.addEventListener('click', (event) => {
    event.stopPropagation();
    const open = !diagPopover.hasAttribute('hidden');
    if (open) {
      diagPopover.setAttribute('hidden', '');
      diagToggle.setAttribute('aria-expanded', 'false');
    } else {
      diagPopover.removeAttribute('hidden');
      diagToggle.setAttribute('aria-expanded', 'true');
      powerMenu?.setAttribute('hidden', '');
      powerMenuToggle?.setAttribute('aria-expanded', 'false');
      langMenu?.setAttribute('hidden', '');
      langToggle?.setAttribute('aria-expanded', 'false');
      updateDiagnosticsCard();
    }
  });
  $('#diagnostics-close')?.addEventListener('click', () => {
    diagPopover?.setAttribute('hidden', '');
    diagToggle?.setAttribute('aria-expanded', 'false');
  });
  $('#diagnostics-reconnect')?.addEventListener('click', () => {
    diagPopover?.setAttribute('hidden', '');
    diagToggle?.setAttribute('aria-expanded', 'false');
    restartVideo();
    toast('已重新连接视频');
  });
  $('#diagnostics-scan')?.addEventListener('click', () => {
    diagPopover?.setAttribute('hidden', '');
    diagToggle?.setAttribute('aria-expanded', 'false');
    scanDevices();
    toast('已触发设备扫描');
  });
  document.addEventListener('click', (event) => {
    if (!event.target.closest('.power-menu-container')) {
      powerMenu?.setAttribute('hidden', '');
      powerMenuToggle?.setAttribute('aria-expanded', 'false');
    }
    if (!event.target.closest('.diagnostics-popover') && !event.target.closest('#diagnostics-toggle')) {
      diagPopover?.setAttribute('hidden', '');
      diagToggle?.setAttribute('aria-expanded', 'false');
    }
    if (!event.target.closest('.lang-menu-container')) {
      langMenu?.setAttribute('hidden', '');
      langToggle?.setAttribute('aria-expanded', 'false');
    }
    if (!event.target.closest('#topbar-perf-btn') && !event.target.closest('#topbar-perf-popover')) {
      closeTopbarPerf();
    }
  });
  // In-app confirmation sized to the risk: short press and reset need one
  // explicit click; force-off must be held for HOLD_TO_CONFIRM_MS so a stray
  // click or Enter can never cut power to the target.
  const HOLD_TO_CONFIRM_MS = 1000;
  const POWER_CONFIRM = {
    press: {
      title: '短按电源',
      desc: '向被控机发送一次电源按钮短按。被控机运行中时，这通常会触发关机。',
      ok: '确认短按',
      hold: false,
    },
    reset: { title: '复位', desc: '立即复位被控机，未保存的数据会丢失。', ok: '确认复位', hold: false },
    'force-off': {
      title: '强制关机',
      desc: '长按电源 5 秒强制断电，可能导致数据丢失或文件系统损坏。',
      ok: '按住 1 秒强制关机',
      hold: true,
    },
  };
  function confirmPowerAction(action) {
    const spec = POWER_CONFIRM[action],
      dialog = $('#power-confirm'),
      ok = $('#power-confirm-ok');
    if (!spec || !dialog?.showModal) return Promise.resolve(confirm(tr(spec?.title || '')));
    $('#power-confirm-title').textContent = spec.title;
    $('#power-confirm-desc').textContent = spec.desc;
    $('.hold-label', ok).textContent = spec.ok;
    ok.classList.toggle('danger', spec.hold);
    ok.classList.toggle('primary', !spec.hold);
    return new Promise((resolve) => {
      let timer = 0,
        done = false;
      const finish = (result) => {
        if (done) return;
        done = true;
        cancelHold();
        ok.removeEventListener('click', onClick);
        ok.removeEventListener('pointerdown', startHold);
        ok.removeEventListener('keydown', onKeyDown);
        ['pointerup', 'pointerleave', 'pointercancel', 'keyup', 'blur'].forEach((type) =>
          ok.removeEventListener(type, cancelHold),
        );
        dialog.removeEventListener('close', onClose);
        if (dialog.open) dialog.close();
        resolve(result);
      };
      const startHold = (event) => {
        if (timer || (event.type === 'pointerdown' && event.button !== 0)) return;
        ok.classList.add('holding');
        timer = setTimeout(() => finish(true), HOLD_TO_CONFIRM_MS);
      };
      const cancelHold = () => {
        clearTimeout(timer);
        timer = 0;
        ok.classList.remove('holding');
      };
      const onKeyDown = (event) => {
        if ((event.key === 'Enter' || event.key === ' ') && !event.repeat) {
          event.preventDefault();
          startHold(event);
        }
      };
      const onClick = () => {
        if (!spec.hold) finish(true);
      };
      const onClose = () => finish(false);
      ok.addEventListener('click', onClick);
      if (spec.hold) {
        ok.addEventListener('pointerdown', startHold);
        ok.addEventListener('keydown', onKeyDown);
        ['pointerup', 'pointerleave', 'pointercancel', 'keyup', 'blur'].forEach((type) =>
          ok.addEventListener(type, cancelHold),
        );
      }
      dialog.addEventListener('close', onClose);
      dialog.showModal();
      // Focus Cancel first: a reflexive Enter must never confirm.
      $('button[value="cancel"]', dialog)?.focus();
    });
  }
  async function executePowerAction(action) {
    const caps = bootstrap.capabilities || {};
    if (action === 'reset' && caps.gpio_reset !== true) {
      toast('复位引脚未配置', true);
      return;
    }
    if ((action === 'press' || action === 'force-off') && caps.gpio_power !== true) {
      toast('电源引脚未配置', true);
      return;
    }
    let url = '/power',
      duration = '0.5',
      label = '电源';
    if (!(await confirmPowerAction(action))) return;
    if (action === 'reset') {
      url = '/reset';
      duration = '0.5';
      label = '复位';
    } else if (action === 'force-off') {
      url = '/power';
      duration = '5';
      label = '强制关机';
    } else {
      label = '短按电源';
    }
    toast(`${label}操作已发送…`);
    try {
      const response = await fetch(url, {
        method: 'POST',
        body: new URLSearchParams({ duration }),
        credentials: 'same-origin',
      });
      const type = response.headers.get('content-type') || '';
      const data = type.includes('json') ? await response.json() : await response.text();
      if (!response.ok) throw new Error((data && data.error) || data || `${label}操作失败`);
      toast(`${label}操作已执行`);
      setTimeout(refreshStatus, 1000);
    } catch (error) {
      toast(error.message || `${label}操作失败`, true);
    }
  }
  $$('[data-power-action]').forEach((button) =>
    button.addEventListener('click', () => {
      powerMenu?.setAttribute('hidden', '');
      powerMenuToggle?.setAttribute('aria-expanded', 'false');
      executePowerAction(button.dataset.powerAction);
    }),
  );
  async function runGpioTest(target, button) {
    if (gpioTestBusy || button.disabled) return;
    gpioTestBusy = true;
    const label = button.textContent;
    button.disabled = true;
    button.textContent = '…';
    syncGpioControls();
    try {
      await request('/api/gpio/test', { method: 'POST', body: JSON.stringify({ target }) });
      toast(`${target === 'reset' ? 'RESET' : 'POWER'} 已执行`);
      await refreshStatus();
    } catch (error) {
      toast(error.message || 'GPIO 操作失败', true);
    } finally {
      button.textContent = label;
      gpioTestBusy = false;
      syncGpioControls();
    }
  }
  $$('[data-gpio-test]').forEach((button) =>
    button.addEventListener('click', () => runGpioTest(button.dataset.gpioTest, button)),
  );
  $$('[data-power-led-refresh]').forEach((button) =>
    button.addEventListener('click', async (event) => {
      const current = event.currentTarget;
      if (current.disabled) return;
      const label = current.textContent;
      current.disabled = true;
      current.textContent = '…';
      try {
        await refreshStatus();
      } finally {
        current.textContent = label;
        syncGpioControls();
      }
    }),
  );
  $('#video-config').addEventListener('submit', async (event) => {
    event.preventDefault();
    const f = event.currentTarget,
      button = $('button[type=submit]', f);
    if (button.disabled) return;
    syncVideoPresets();
    syncH264Preset();
    const selectedTransport = value(f, 'transport'),
      requestId = ++videoConfigRequestId,
      label = button.textContent,
      payload = {
        video: {
          device: value(f, 'device') || null,
          width: optionalNumber(value(f, 'width')),
          height: optionalNumber(value(f, 'height')),
          frames_per_second: optionalNumber(value(f, 'frames_per_second')),
          encoding: value(f, 'encoding'),
          jpeg_quality: Number(value(f, 'jpeg_quality')),
          h264: {
            bitrate_kbps: Number(value(f, 'h264_bitrate_kbps')),
            encoder: value(f, 'h264_encoder'),
            allow_software: f.elements.h264_allow_software.checked,
            max_sessions: Number(value(f, 'h264_max_sessions')),
          },
        },
      };
    button.disabled = true;
    button.textContent = '应用中…';
    stopVideo();
    try {
      await request('/api/config', { method: 'POST', body: JSON.stringify(payload) });
      if (requestId !== videoConfigRequestId) return;
      videoPreference = ['auto', 'webrtc_h264', 'mjpeg'].includes(selectedTransport)
        ? selectedTransport
        : 'auto';
      saveVideoUiPreference('transport', videoPreference);
      bootstrap = await request('/api/bootstrap');
      applyBootstrap();
      toast('视频设置已应用');
    } catch (e) {
      if (requestId === videoConfigRequestId) toast(e.message, true);
    } finally {
      if (requestId === videoConfigRequestId) {
        button.disabled = false;
        button.textContent = label;
        if (shouldVideoRun()) startVideo(true);
      }
    }
  });
  $$('[data-video-preset]').forEach((chip) =>
    chip.addEventListener('click', () => {
      $$('.preset-chip').forEach((c) => c.classList.toggle('active', c === chip));
      const preset = chip.dataset.videoPreset,
        form = $('#video-config');
      if (preset === 'desktop') {
        form.elements.transport.value = 'auto';
        setVideoPreference('auto');
        form.elements.resolution_preset.value = '1920x1080';
        form.elements.fps_preset.value = '60';
        form.elements.h264_bitrate_preset.value = '8000';
        setDisplayScale('fit', true);
        setVideoRendering('smooth', true);
        toast(t('preset_desktop_applied', '已加载桌面优化预设 (1080p 60fps 8M)'));
      } else if (preset === 'bios') {
        form.elements.transport.value = 'mjpeg';
        setVideoPreference('mjpeg');
        form.elements.resolution_preset.value = '1280x720';
        form.elements.fps_preset.value = '30';
        form.elements.h264_bitrate_preset.value = '4000';
        setDisplayScale('native', true);
        setVideoRendering('pixelated', true);
        toast(t('preset_bios_applied', '已加载 BIOS/UEFI 预设 (720p 30fps MJPEG 像素锐利)'));
      } else if (preset === 'bandwidth') {
        form.elements.transport.value = 'auto';
        setVideoPreference('auto');
        form.elements.resolution_preset.value = '1280x720';
        form.elements.fps_preset.value = '24';
        form.elements.h264_bitrate_preset.value = '2000';
        setDisplayScale('fit', true);
        setVideoRendering('smooth', true);
        toast(t('preset_bandwidth_applied', '已加载低带宽省流预设 (720p 24fps 2M)'));
      }
      syncVideoPresets();
      syncH264Preset();
    }),
  );
