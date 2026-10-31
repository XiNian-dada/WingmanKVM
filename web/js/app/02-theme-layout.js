  function getPreferredTheme() {
    try {
      return localStorage.getItem('wingman_theme') || 'system';
    } catch (_) {
      return 'system';
    }
  }
  function applyTheme(theme) {
    const root = document.documentElement,
      darkIcon = $('.theme-icon-dark'),
      lightIcon = $('.theme-icon-light'),
      toggleBtn = $('#theme-toggle');
    if (theme === 'dark') {
      root.setAttribute('data-theme', 'dark');
      darkIcon?.classList.add('hidden');
      lightIcon?.classList.remove('hidden');
      if (toggleBtn) toggleBtn.title = t('theme_dark', '深色模式');
    } else if (theme === 'light') {
      root.setAttribute('data-theme', 'light');
      darkIcon?.classList.remove('hidden');
      lightIcon?.classList.add('hidden');
      if (toggleBtn) toggleBtn.title = t('theme_light', '浅色模式');
    } else {
      root.removeAttribute('data-theme');
      const isDark = matchMedia('(prefers-color-scheme: dark)').matches;
      darkIcon?.classList.toggle('hidden', isDark);
      lightIcon?.classList.toggle('hidden', !isDark);
      if (toggleBtn) toggleBtn.title = t('theme_system', '跟随系统');
    }
    try {
      localStorage.setItem('wingman_theme', theme);
    } catch (_) {}
  }
  function toggleTheme() {
    const cur = getPreferredTheme();
    const next = cur === 'system' ? 'dark' : cur === 'dark' ? 'light' : 'system';
    document.documentElement.classList.add('theme-switching');
    applyTheme(next);
    toast(t(next === 'dark' ? 'theme_dark' : next === 'light' ? 'theme_light' : 'theme_system'));
    setTimeout(() => document.documentElement.classList.remove('theme-switching'), 220);
  }
  matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
    if (getPreferredTheme() === 'system') {
      document.documentElement.classList.add('theme-switching');
      applyTheme('system');
      setTimeout(() => document.documentElement.classList.remove('theme-switching'), 220);
    }
  });
  let activeHelp = null,
    helpTimer = 0;
  function positionFieldTooltip() {
    if (!activeHelp || fieldTooltip.hidden) return;
    const anchor = activeHelp.getBoundingClientRect(),
      tip = fieldTooltip.getBoundingClientRect(),
      gap = 10,
      pad = 12;
    let left = anchor.right + gap;
    if (left + tip.width > innerWidth - pad) left = anchor.left - tip.width - gap;
    if (left < pad) left = Math.max(pad, (innerWidth - tip.width) / 2);
    let top = anchor.top + (anchor.height - tip.height) / 2;
    if (top < pad) top = anchor.bottom + gap;
    if (top + tip.height > innerHeight - pad) top = anchor.top - tip.height - gap;
    left = Math.max(pad, Math.min(left, innerWidth - tip.width - pad));
    top = Math.max(pad, Math.min(top, innerHeight - tip.height - pad));
    fieldTooltip.style.left = `${Math.round(left)}px`;
    fieldTooltip.style.top = `${Math.round(top)}px`;
  }
  function hideFieldTooltip() {
    clearTimeout(helpTimer);
    activeHelp = null;
    fieldTooltip.classList.remove('visible');
    helpTimer = setTimeout(() => {
      if (!activeHelp) fieldTooltip.hidden = true;
    }, 160);
  }
  function showFieldTooltip(button) {
    const text = button.dataset.help?.trim();
    if (!text) return;
    clearTimeout(helpTimer);
    activeHelp = button;
    fieldTooltip.textContent = text;
    fieldTooltip.hidden = false;
    fieldTooltip.classList.remove('visible');
    requestAnimationFrame(() => {
      if (activeHelp !== button) return;
      positionFieldTooltip();
      fieldTooltip.classList.add('visible');
    });
  }
  $$('[data-help]').forEach((button) => {
    button.setAttribute('aria-describedby', 'field-tooltip');
    button.addEventListener('pointerenter', () => showFieldTooltip(button));
    button.addEventListener('pointerleave', () => {
      if (document.activeElement !== button) hideFieldTooltip();
    });
    button.addEventListener('focus', () => showFieldTooltip(button));
    button.addEventListener('blur', hideFieldTooltip);
    button.addEventListener('click', (event) => {
      event.preventDefault();
      event.stopPropagation();
      showFieldTooltip(button);
      clearTimeout(helpTimer);
      helpTimer = setTimeout(hideFieldTooltip, 3200);
    });
  });
  addEventListener('resize', positionFieldTooltip, { passive: true });
  addEventListener('scroll', positionFieldTooltip, { passive: true, capture: true });
  let cachedSurfaceBox = null;
  function invalidateSurfaceBox() {
    cachedSurfaceBox = null;
  }
  function getVideoDimensions() {
    const surface = activeVideoSurface();
    if (!surface) return null;
    const w = surface instanceof HTMLVideoElement ? surface.videoWidth : surface.naturalWidth;
    const h = surface instanceof HTMLVideoElement ? surface.videoHeight : surface.naturalHeight;
    if (w > 0 && h > 0) return { width: w, height: h, ratio: w / h };
    return null;
  }
  let consoleUserMoved = false;
  function fitConsoleToVideo(forceCenter = false) {
    if (document.fullscreenElement || matchMedia('(max-width:768px)').matches) {
      consoleBox.style.width = '';
      consoleBox.style.height = '';
      consoleBox.style.left = '';
      consoleBox.style.top = '';
      invalidateSurfaceBox();
      return;
    }
    if (!consoleBox.classList.contains('mode-fit')) {
      invalidateSurfaceBox();
      return;
    }
    const ws = $('#workspace');
    if (!ws) return;
    const wsW = ws.clientWidth,
      wsH = ws.clientHeight;
    if (!wsW || !wsH) return;
    const inspectorOffset =
      app.classList.contains('inspector-open') && !matchMedia('(max-width:900px)').matches ? 360 : 0;
    const effectiveWsW = Math.max(320, wsW - inspectorOffset);
    const dim = getVideoDimensions();
    const ratio = dim && dim.ratio > 0 ? dim.ratio : 16 / 9;
    const pad = 24,
      headH = 46,
      borderW = 2,
      borderH = 2;
    const availW = Math.max(320, effectiveWsW - pad * 2 - borderW);
    const availH = Math.max(200, wsH - pad * 2 - headH - borderH);
    let targetVpW, targetVpH;
    if (availW / availH > ratio) {
      targetVpH = availH;
      targetVpW = Math.round(targetVpH * ratio);
    } else {
      targetVpW = availW;
      targetVpH = Math.round(targetVpW / ratio);
    }
    const totalW = targetVpW + borderW;
    const totalH = targetVpH + headH + borderH;
    consoleBox.style.width = `${totalW}px`;
    consoleBox.style.height = `${totalH}px`;
    if (forceCenter || !consoleUserMoved) {
      const left = Math.max(0, Math.round((effectiveWsW - totalW) / 2));
      const top = Math.max(0, Math.round((wsH - totalH) / 2));
      consoleBox.style.left = `${left}px`;
      consoleBox.style.top = `${top}px`;
    } else {
      const maxL = Math.max(0, effectiveWsW - totalW);
      const maxT = Math.max(0, wsH - totalH);
      const curL = parseInt(consoleBox.style.left, 10) || 0;
      const curT = parseInt(consoleBox.style.top, 10) || 0;
      consoleBox.style.left = `${Math.max(0, Math.min(curL, maxL))}px`;
      consoleBox.style.top = `${Math.max(0, Math.min(curT, maxT))}px`;
    }
    invalidateSurfaceBox();
  }
  window.addEventListener(
    'resize',
    () => {
      fitConsoleToVideo();
      invalidateSurfaceBox();
    },
    { passive: true },
  );
  window.addEventListener('scroll', invalidateSurfaceBox, { passive: true });
  document.addEventListener('fullscreenchange', () => {
    fitConsoleToVideo();
    invalidateSurfaceBox();
  });
  const wsResizeObserver = new ResizeObserver(() => {
    fitConsoleToVideo();
    if (!consoleBox.classList.contains('mode-fit')) {
      const ws = $('#workspace');
      if (ws && ws.clientWidth > 0) {
        const curL = parseInt(consoleBox.style.left, 10);
        const cW = consoleBox.offsetWidth;
        if (!isNaN(curL) && curL + cW > ws.clientWidth) {
          consoleBox.style.left = `${Math.max(0, ws.clientWidth - cW - 24)}px`;
        }
      }
    }
    invalidateSurfaceBox();
  });
  const wsTarget = $('#workspace');
  if (wsTarget) wsResizeObserver.observe(wsTarget);
  viewport.addEventListener('scroll', invalidateSurfaceBox, { passive: true, capture: true });
  let isPanActive = false,
    isDraggingPan = false;
  let panStartX = 0,
    panStartY = 0,
    panScrollX = 0,
    panScrollY = 0;
  function setPanMode(active) {
    isPanActive = !!active && consoleBox.classList.contains('mode-native');
    $('#pan-toggle')?.classList.toggle('active', isPanActive);
    viewport.classList.toggle('panning', isPanActive);
    if (!isPanActive) {
      viewport.classList.remove('panning-active');
      isDraggingPan = false;
    }
  }
  function centerNativeViewport() {
    if (!consoleBox.classList.contains('mode-native')) return;
    viewport.scrollLeft = Math.max(0, Math.round((viewport.scrollWidth - viewport.clientWidth) / 2));
    viewport.scrollTop = Math.max(0, Math.round((viewport.scrollHeight - viewport.clientHeight) / 2));
    invalidateSurfaceBox();
  }
