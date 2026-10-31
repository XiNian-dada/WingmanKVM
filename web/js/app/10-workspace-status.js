  function setWorkspaceMode(mode) {
    const next = mode === 'terminal' ? 'terminal' : 'video',
      terminalMode = next === 'terminal',
      changed = videoWorkspace !== next;
    videoWorkspace = next;
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
  window.addEventListener('keydown', (event) => {
    if (event.key === 'Escape' && !inputEnabled()) {
      if (topbarPerfPopover && !topbarPerfPopover.hasAttribute('hidden')) {
        closeTopbarPerf();
        return;
      }
      if (app.classList.contains('inspector-open') && !keyboardDialog?.open && !settingsDialog?.open) {
        setInspectorOpen(false);
      }
    }
  });

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
      latestVideoStatus = status.video || null;
      latestPowerStatus = status.power || null;
      latestSystemStatus = status.system || null;
      renderPowerLed(latestPowerStatus);
      if (status.hid?.keyboard_leds) renderKeyboardLeds(status.hid.keyboard_leds);
      if (status.client) renderClientInfo(status.client);
      if (latestSystemStatus) renderSystemMetrics(latestSystemStatus);
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

  function renderSystemMetrics(sys) {
    if (!sys) return;
    latestSystemStatus = sys;
    const cpu = sys.cpu,
      mem = sys.memory,
      disk = sys.disk;
    if (cpu) {
      const cpuPct = Math.min(100, Math.max(0, Number(cpu.percent) || 0));
      const cpuStr = `${cpuPct.toFixed(1)}%`,
        cpuRoundStr = `${Math.round(cpuPct)}%`;
      const cpuBarColor = cpuPct > 85 ? 'var(--red)' : cpuPct > 70 ? 'var(--amber)' : 'var(--blue)';
      const coresStr = `${cpu.cores || 1} ${t('cpu_cores', '核')}`;
      const cpuEl = $('#perf-cpu-val'),
        bar = $('#perf-cpu-bar');
      if (cpuEl) cpuEl.textContent = cpuStr;
      if (bar) {
        bar.style.width = `${cpuPct}%`;
        bar.style.backgroundColor = cpuBarColor;
      }
      const coresEl = $('#perf-cpu-cores');
      if (coresEl) coresEl.textContent = coresStr;
      const diagCpu = $('#diag-cpu');
      if (diagCpu) diagCpu.textContent = `${cpuStr} (${coresStr})`;
      const topbarCpu = $('#topbar-cpu-val');
      if (topbarCpu) topbarCpu.textContent = cpuStr;
      const topbarCompact = $('#topbar-compact-val');
      if (topbarCompact) topbarCompact.textContent = cpuRoundStr;
      const popCpu = $('#pop-cpu-val');
      if (popCpu) popCpu.textContent = cpuStr;
      const popBar = $('#pop-cpu-bar');
      if (popBar) {
        popBar.style.width = `${cpuPct}%`;
        popBar.style.backgroundColor = cpuBarColor;
      }
      const popCores = $('#pop-cpu-cores');
      if (popCores) popCores.textContent = coresStr;
    }
    if (sys.load_avg && Array.isArray(sys.load_avg)) {
      const load1 = sys.load_avg.length > 0 ? Number(sys.load_avg[0]).toFixed(2) : '--';
      const loadStr = sys.load_avg.map((v) => Number(v).toFixed(2)).join(', ');
      const loadEl = $('#perf-load');
      if (loadEl) {
        loadEl.textContent = `${t('load', '负载')} ${load1}`;
        loadEl.title = `${t('load_average', '系统负载')}: ${loadStr}`;
      }
      const diagLoad = $('#diag-load');
      if (diagLoad) diagLoad.textContent = loadStr;
      const popLoad = $('#pop-load-val');
      if (popLoad) popLoad.textContent = loadStr;
    }
    if (mem) {
      const memPct = Math.min(100, Math.max(0, Number(mem.percent) || 0));
      const memStr = `${memPct.toFixed(0)}%`;
      const memBarColor = memPct > 85 ? 'var(--red)' : memPct > 70 ? 'var(--amber)' : 'var(--blue)';
      const memDetailStr =
        mem.total_bytes > 0 ? `${formatBytes(mem.used_bytes)} / ${formatBytes(mem.total_bytes)}` : '--';
      const memEl = $('#perf-mem-val'),
        bar = $('#perf-mem-bar');
      if (memEl) memEl.textContent = memStr;
      if (bar) {
        bar.style.width = `${memPct}%`;
        bar.style.backgroundColor = memBarColor;
      }
      const memDetail = $('#perf-mem-detail');
      if (memDetail) memDetail.textContent = memDetailStr;
      const diagMem = $('#diag-mem');
      if (diagMem) diagMem.textContent = mem.total_bytes > 0 ? `${memDetailStr} (${memStr})` : '--';
      const topbarMem = $('#topbar-mem-val');
      if (topbarMem) topbarMem.textContent = memStr;
      const popMem = $('#pop-mem-val');
      if (popMem) popMem.textContent = memStr;
      const popBar = $('#pop-mem-bar');
      if (popBar) {
        popBar.style.width = `${memPct}%`;
        popBar.style.backgroundColor = memBarColor;
      }
      const popDetail = $('#pop-mem-detail');
      if (popDetail) popDetail.textContent = memDetailStr;
    }
    if (disk) {
      const diskPct = Math.min(100, Math.max(0, Number(disk.percent) || 0));
      const diskStr = `${diskPct.toFixed(0)}%`;
      const diskBarColor = diskPct > 85 ? 'var(--red)' : diskPct > 70 ? 'var(--amber)' : 'var(--blue)';
      const diskDetailStr =
        disk.total_bytes > 0 ? `${formatBytes(disk.used_bytes)} / ${formatBytes(disk.total_bytes)}` : '--';
      const diskEl = $('#perf-disk-val'),
        bar = $('#perf-disk-bar');
      if (diskEl) diskEl.textContent = diskStr;
      if (bar) {
        bar.style.width = `${diskPct}%`;
        bar.style.backgroundColor = diskBarColor;
      }
      const diskDetail = $('#perf-disk-detail');
      if (diskDetail) diskDetail.textContent = diskDetailStr;
      const diagDisk = $('#diag-disk');
      if (diagDisk) diagDisk.textContent = disk.total_bytes > 0 ? `${diskDetailStr} (${diskStr})` : '--';
      const popDisk = $('#pop-disk-val');
      if (popDisk) popDisk.textContent = diskStr;
      const popBar = $('#pop-disk-bar');
      if (popBar) {
        popBar.style.width = `${diskPct}%`;
        popBar.style.backgroundColor = diskBarColor;
      }
      const popDetail = $('#pop-disk-detail');
      if (popDetail) popDetail.textContent = diskDetailStr;
    }
    if (sys.temperature !== null && sys.temperature !== undefined) {
      const temp = Number(sys.temperature);
      const tempStr = temp.toFixed(1);
      const isWarm = temp >= 75;
      const tempBadge = $('#perf-temp-badge'),
        tempVal = $('#perf-temp-val');
      if (tempBadge && tempVal) {
        tempBadge.classList.remove('hidden');
        tempVal.textContent = tempStr;
        tempBadge.classList.toggle('warning', isWarm);
      }
      const diagTempCard = $('#diag-temp-card'),
        diagTemp = $('#diag-temp');
      if (diagTempCard && diagTemp) {
        diagTempCard.classList.remove('hidden');
        diagTemp.textContent = `${tempStr} °C`;
      }
      const topbarTempSeg = $('#topbar-temp-seg'),
        topbarTempVal = $('#topbar-temp-val');
      if (topbarTempSeg && topbarTempVal) {
        topbarTempSeg.classList.remove('hidden');
        topbarTempVal.textContent = `${tempStr}°C`;
      }
      const popTempBadge = $('#pop-temp-badge'),
        popTempVal = $('#pop-temp-val');
      if (popTempBadge && popTempVal) {
        popTempBadge.classList.remove('hidden');
        popTempVal.textContent = tempStr;
        popTempBadge.classList.toggle('warning', isWarm);
      }
    } else {
      $('#perf-temp-badge')?.classList.add('hidden');
      $('#diag-temp-card')?.classList.add('hidden');
      $('#topbar-temp-seg')?.classList.add('hidden');
      $('#pop-temp-badge')?.classList.add('hidden');
    }
    if (sys.uptime_seconds) {
      const upStr = formatUptime(sys.uptime_seconds);
      const upEl = $('#perf-uptime'),
        diagUp = $('#diag-uptime'),
        popUp = $('#pop-uptime-val');
      if (upEl) upEl.textContent = upStr;
      if (diagUp) diagUp.textContent = upStr;
      if (popUp) popUp.textContent = upStr;
    }
  }
