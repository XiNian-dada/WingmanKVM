  function setWorkspaceMode(mode) {
    const next = mode === 'terminal' ? 'terminal' : 'video',
      terminalMode = next === 'terminal',
      changed = videoWorkspace !== next;
    videoWorkspace = next;
    syncTerminalTabIndicator();
    invalidateSurfaceBox();
    $('#terminal-window').hidden = !terminalMode;
    $('#video-viewport').hidden = terminalMode;
    $$('[data-workspace]').forEach((tab) => tab.classList.toggle('active', tab.dataset.workspace === next));
    $('#terminal-clear')?.classList.toggle('hidden', !terminalMode);
    if (!terminalMode) $('#terminal-reconnect')?.classList.add('hidden');
    else if (!terminalSocket || terminalSocket.readyState > WebSocket.OPEN)
      $('#terminal-reconnect')?.classList.remove('hidden');
    $('.quick-keys')?.classList.toggle('hidden', terminalMode);
    $('#input-state')?.classList.toggle('hidden', terminalMode);
    $('#mode-button')?.classList.toggle('hidden', terminalMode);
    $('#video-transport-label')?.classList.toggle('hidden', terminalMode);
    syncInputUi();
    if (terminalMode) {
      if (changed) stopVideo();
      terminalConnect();
      scheduleTerminalFit();
      requestAnimationFrame(() => terminal?.focus());
    } else {
      if (changed) startVideo();
      fitConsoleToVideo();
    }
  }
  $$('[data-workspace]').forEach((tab) =>
    tab.addEventListener('click', () => setWorkspaceMode(tab.dataset.workspace)),
  );
  $$('.tab-button').forEach((button) =>
    button.addEventListener('click', () => {
      const mode = button.dataset.panelTarget;
      $$('.tab-button').forEach((item) => {
        item.classList.toggle('active', item === button);
        item.setAttribute('aria-selected', String(item === button));
      });
      $$('[data-panel]').forEach((panel) => (panel.hidden = panel.dataset.panel !== mode));
    }),
  );
  const inspectorToggle = $('#inspector-toggle');
  function setInspectorOpen(open) {
    invalidateSurfaceBox();
    app.classList.toggle('inspector-open', open);
    inspectorToggle.setAttribute('aria-expanded', String(open));
    inspectorToggle.classList.toggle('active', open);
    inspectorToggle.textContent = open
      ? t('collapse_inspector', '收起面板')
      : t('open_inspector', '控制面板');
    requestAnimationFrame(() => {
      invalidateSurfaceBox();
      if (videoWorkspace === 'terminal') scheduleTerminalFit();
      else fitConsoleToVideo();
    });
  }
  inspectorToggle.addEventListener('click', () =>
    setInspectorOpen(!app.classList.contains('inspector-open')),
  );
  $('#inspector-close')?.addEventListener('click', () => setInspectorOpen(false));
  $('#inspector-backdrop')?.addEventListener('click', () => setInspectorOpen(false));
  $('#inspector-float-open')?.addEventListener('click', () => setInspectorOpen(true));
  function openShortcutSheet() {
    const dialog = $('#shortcut-dialog');
    if (dialog && !dialog.open) dialog.showModal?.();
  }
  $('#shortcut-open')?.addEventListener('click', openShortcutSheet);
  window.addEventListener('keydown', (event) => {
    // "?" opens the cheat sheet only while input stays local, so it never
    // competes with typing on the target or in a form field.
    if (
      event.key === '?' &&
      !inputEnabled() &&
      !event.ctrlKey &&
      !event.altKey &&
      !event.metaKey &&
      !interactiveTarget(event.target) &&
      !app.classList.contains('hidden')
    ) {
      event.preventDefault();
      openShortcutSheet();
      return;
    }
    if (event.key === 'Escape' && !inputEnabled()) {
      if (topbarPerfPopover && !topbarPerfPopover.hasAttribute('hidden')) {
        closeTopbarPerf();
        return;
      }
      // Esc belongs to an open dialog first; only collapse the inspector when
      // no modal (power confirm, cheat sheet, paste, ...) is consuming it.
      if (app.classList.contains('inspector-open') && !document.querySelector('dialog[open]')) {
        setInspectorOpen(false);
      }
    }
  });

  // Hot-plug recovery (#93): announce capture-card unplug/replug and keep a
  // persistent badge while the USB keyboard/mouse gadget re-enumerates.
  let hidLinkLost = false;
  function renderHotplugTransitions(previousReason, video, hid) {
    const reason = video?.reason || '';
    if (reason === 'device_missing' && previousReason !== 'device_missing') toast('采集卡已断开', true);
    else if (previousReason === 'device_missing' && reason !== 'device_missing') toast('采集卡已恢复');
    const lost = !!(hid?.keyboard_reenumerating || hid?.mouse_reenumerating);
    const badge = $('#hid-link-state');
    if (badge) badge.classList.toggle('hidden', !lost);
    if (lost && !hidLinkLost) toast('USB 键鼠通道重新连接中', true);
    else if (!lost && hidLinkLost) toast('USB 键鼠通道已恢复');
    hidLinkLost = lost;
  }

  async function refreshStatus() {
    if (statusTimer === -1) return;
    if (statusTimer > 0) clearTimeout(statusTimer);
    if (app.classList.contains('hidden') || document.hidden) {
      statusTimer = 0;
      renderVideoUi();
      return;
    }
    statusTimer = -1;
    try {
      const status = await request('/api/status');
      const previousVideoReason = latestVideoStatus?.reason || '';
      latestVideoStatus = status.video || null;
      renderHotplugTransitions(previousVideoReason, latestVideoStatus, status.hid);
      latestPowerStatus = status.power || null;
      latestSystemStatus = status.system || null;
      renderPowerLed(latestPowerStatus);
      if (status.hid?.keyboard_leds) renderKeyboardLeds(status.hid.keyboard_leds);
      if (status.client) renderClientInfo(status.client);
      if (latestSystemStatus) renderSystemMetrics(latestSystemStatus);
      if (status.runtime) renderRuntimeMetrics(status.runtime, status.webrtc);
      renderVideoUi();
    } catch {
      // A failed status poll says nothing about the video stream itself: keep
      // the last known video state (and a playing surface) instead of
      // inventing "offline". Only the power LED reading is marked unknown.
      latestPowerStatus = { power_led: { configured: true, state: 'unknown', sense_error: '连接失败' } };
      renderPowerLed(latestPowerStatus);
    } finally {
      statusTimer =
        app.classList.contains('hidden') || document.hidden
          ? 0
          : setTimeout(() => {
              statusTimer = 0;
              refreshStatus();
            }, 3000);
    }
  }
  const mediaChoices = new Map();
  let mediaBusy = false,
    mediaServerBusy = false,
    mediaDownloadBusy = false,
    mediaStatus = {};
  function mediaItems(data) {
    return Array.isArray(data) ? data : data?.items || data?.images || [];
  }
  function mediaTypeLabel(type) {
    return type === 'cdrom' ? '光驱' : type === 'disk' ? 'U 盘' : '自动';
  }
  function mediaStateLabel(state) {
    return (
      {
        attached: '已挂载',
        mounted: '已挂载',
        detached: '未挂载',
        attaching: '正在挂载',
        detaching: '正在弹出',
        ejecting: '正在弹出',
        error: '状态异常',
      }[state] || ''
    );
  }
  function formatBytes(value) {
    const bytes = Number(value);
    if (!Number.isFinite(bytes) || bytes < 0) return '';
    if (bytes < 1024) return `${bytes} B`;
    const units = ['KB', 'MB', 'GB', 'TB'];
    let size = bytes,
      index = -1;
    do {
      size /= 1024;
      index++;
    } while (size >= 1024 && index < units.length - 1);
    return `${size >= 10 ? size.toFixed(0) : size.toFixed(1)} ${units[index]}`;
  }
  function formatUptime(seconds) {
    const s = Math.floor(Number(seconds) || 0);
    if (s <= 0) return '--';
    const days = Math.floor(s / 86400);
    const hours = Math.floor((s % 86400) / 3600);
    const mins = Math.floor((s % 3600) / 60);
    if (days > 0) return `${days}d ${hours}h`;
    if (hours > 0) return `${hours}h ${mins}m`;
    return `${mins}m ${s % 60}s`;
  }
  const topbarPerfBtn = $('#topbar-perf-btn'),
    topbarPerfPopover = $('#topbar-perf-popover');
  let perfPopoverPinned = false,
    perfHoverTimer = 0,
    perfLeaveTimer = 0;
  function openTopbarPerf(pinned = false) {
    if (!topbarPerfPopover) return;
    topbarPerfPopover.removeAttribute('hidden');
    topbarPerfBtn?.setAttribute('aria-expanded', 'true');
    topbarPerfBtn?.classList.add('active');
    perfPopoverPinned = !!pinned;
    $('#power-menu')?.setAttribute('hidden', '');
    $('#power-menu-toggle')?.setAttribute('aria-expanded', 'false');
    $('#lang-menu')?.setAttribute('hidden', '');
    $('#lang-toggle')?.setAttribute('aria-expanded', 'false');
    $('#diagnostics-popover')?.setAttribute('hidden', '');
    $('#diagnostics-toggle')?.setAttribute('aria-expanded', 'false');
    if (latestSystemStatus) renderSystemMetrics(latestSystemStatus);
  }
  function closeTopbarPerf() {
    if (!topbarPerfPopover) return;
    topbarPerfPopover.setAttribute('hidden', '');
    topbarPerfBtn?.setAttribute('aria-expanded', 'false');
    topbarPerfBtn?.classList.remove('active');
    perfPopoverPinned = false;
    clearTimeout(perfHoverTimer);
    clearTimeout(perfLeaveTimer);
  }
  function toggleTopbarPerf(pinned = true) {
    if (!topbarPerfPopover || topbarPerfPopover.hasAttribute('hidden')) openTopbarPerf(pinned);
    else closeTopbarPerf();
  }
  topbarPerfBtn?.addEventListener('click', (event) => {
    event.stopPropagation();
    toggleTopbarPerf(true);
  });
  topbarPerfBtn?.addEventListener('pointerenter', () => {
    if (matchMedia('(pointer:coarse)').matches) return;
    clearTimeout(perfLeaveTimer);
    if (topbarPerfPopover?.hasAttribute('hidden')) {
      perfHoverTimer = setTimeout(() => openTopbarPerf(false), 200);
    }
  });
  topbarPerfBtn?.addEventListener('pointerleave', () => {
    if (matchMedia('(pointer:coarse)').matches) return;
    clearTimeout(perfHoverTimer);
    if (topbarPerfPopover && !topbarPerfPopover.hasAttribute('hidden') && !perfPopoverPinned) {
      perfLeaveTimer = setTimeout(() => closeTopbarPerf(), 250);
    }
  });
  topbarPerfPopover?.addEventListener('pointerenter', () => {
    if (matchMedia('(pointer:coarse)').matches) return;
    clearTimeout(perfLeaveTimer);
  });
  topbarPerfPopover?.addEventListener('pointerleave', () => {
    if (matchMedia('(pointer:coarse)').matches) return;
    if (topbarPerfPopover && !topbarPerfPopover.hasAttribute('hidden') && !perfPopoverPinned) {
      perfLeaveTimer = setTimeout(() => closeTopbarPerf(), 250);
    }
  });
  $('#perf-pop-close')?.addEventListener('click', (event) => {
    event.stopPropagation();
    closeTopbarPerf();
  });
  $('#pop-more-diag')?.addEventListener('click', (event) => {
    event.stopPropagation();
    closeTopbarPerf();
    $('#diagnostics-toggle')?.click();
  });

  // Host metrics appear in two places: the compact top-bar chip + popover and
  // the full diagnostics panel. Each value is computed once and written to both.
  function setText(selector, text) {
    const el = $(selector);
    if (el) el.textContent = text;
  }
  function setBar(selector, pct) {
    const bar = $(selector);
    if (!bar) return;
    bar.style.width = `${pct}%`;
    bar.style.backgroundColor = pct > 85 ? 'var(--red)' : pct > 70 ? 'var(--amber)' : 'var(--blue)';
  }
  function clampPercent(value) {
    return Math.min(100, Math.max(0, Number(value) || 0));
  }
  // Service-side counters: version and uptime, live connections, and whether
  // keyboard/mouse writes are reaching the target (with the latest error).
  function renderRuntimeMetrics(runtime, webrtc) {
    setText('#diag-version', `v${runtime.version} · ${formatUptime(runtime.uptime_seconds)}`);
    setText('#diag-connections', `键鼠 ${runtime.input_sockets} · WebRTC ${webrtc?.sessions ?? 0}`);
    const hid = runtime.hid || {};
    let text = `成功 ${hid.ok ?? 0} · 失败 ${hid.failed ?? 0}`;
    if (hid.last_error) text += ` · 最近错误：${hid.last_error}（${formatUptime(hid.last_error_age_seconds || 1)}前）`;
    setText('#diag-hid', text);
  }
  function renderSystemMetrics(sys) {
    if (!sys) return;
    latestSystemStatus = sys;
    const { cpu, memory: mem, disk } = sys;
    if (cpu) {
      const pct = clampPercent(cpu.percent),
        text = `${pct.toFixed(1)}%`,
        cores = `${cpu.cores || 1} ${t('cpu_cores', '核')}`;
      setText('#topbar-cpu-val', text);
      setText('#topbar-compact-val', `${Math.round(pct)}%`);
      setText('#pop-cpu-val', text);
      setText('#pop-cpu-cores', cores);
      setBar('#pop-cpu-bar', pct);
      setText('#diag-cpu', `${text} (${cores})`);
    }
    if (Array.isArray(sys.load_avg)) {
      const load = sys.load_avg.map((v) => Number(v).toFixed(2)).join(', ');
      setText('#pop-load-val', load);
      setText('#diag-load', load);
    }
    for (const [key, usage] of [
      ['mem', mem],
      ['disk', disk],
    ]) {
      if (!usage) continue;
      const pct = clampPercent(usage.percent),
        text = `${pct.toFixed(0)}%`,
        detail = usage.total_bytes > 0 ? `${formatBytes(usage.used_bytes)} / ${formatBytes(usage.total_bytes)}` : '--';
      if (key === 'mem') setText('#topbar-mem-val', text);
      setText(`#pop-${key}-val`, text);
      setText(`#pop-${key}-detail`, detail);
      setBar(`#pop-${key}-bar`, pct);
      setText(`#diag-${key}`, usage.total_bytes > 0 ? `${detail} (${text})` : '--');
    }
    const hasTemp = sys.temperature !== null && sys.temperature !== undefined;
    $('#diag-temp-card')?.classList.toggle('hidden', !hasTemp);
    $('#topbar-temp-seg')?.classList.toggle('hidden', !hasTemp);
    $('#pop-temp-badge')?.classList.toggle('hidden', !hasTemp);
    if (hasTemp) {
      const temp = Number(sys.temperature).toFixed(1);
      setText('#diag-temp', `${temp} °C`);
      setText('#topbar-temp-val', `${temp}°C`);
      setText('#pop-temp-val', temp);
      $('#pop-temp-badge')?.classList.toggle('warning', Number(sys.temperature) >= 75);
    }
    if (sys.uptime_seconds) {
      const uptime = formatUptime(sys.uptime_seconds);
      setText('#diag-uptime', uptime);
      setText('#pop-uptime-val', uptime);
    }
  }

