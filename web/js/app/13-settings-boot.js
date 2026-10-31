  const settingsDialog = $('#settings-dialog');
  function applyRenderPixelated(enabled) {
    setVideoRendering(enabled ? 'pixelated' : 'smooth', true);
  }
  function syncSettingsDialogUi() {
    if (!settingsDialog) return;
    const hidCheckbox = $('#setting-suppress-hid-timeout');
    if (hidCheckbox) hidCheckbox.checked = getSetting('suppress_hid_timeout', false);
    const pulseCheckbox = $('#setting-capture-pulse');
    if (pulseCheckbox) pulseCheckbox.checked = getSetting('capture_pulse', true);
    const pasteCheckbox = $('#setting-paste-shortcut');
    if (pasteCheckbox) pasteCheckbox.checked = getSetting('paste_shortcut', true);
    const releaseCheckbox = $('#setting-release-toast');
    if (releaseCheckbox) releaseCheckbox.checked = getSetting('release_key_toast', true);
    const pixelCheckbox = $('#setting-render-pixelated');
    if (pixelCheckbox) pixelCheckbox.checked = getSetting('render_pixelated', true);
    const inspectorCheckbox = $('#setting-auto-inspector');
    if (inspectorCheckbox) inspectorCheckbox.checked = getSetting('auto_expand_inspector', true);
    const proxySelect = $('#setting-proxy-protocol');
    if (proxySelect) {
      const config = unwrapConfig(bootstrap);
      proxySelect.value = config.server?.proxy_protocol || 'disabled';
    }
  }
  function openSettingsDialog() {
    if (!settingsDialog) return;
    syncSettingsDialogUi();
    refreshPasskeys();
    try {
      if (typeof settingsDialog.showModal === 'function') {
        settingsDialog.showModal();
      } else if (typeof settingsDialog.show === 'function') {
        settingsDialog.show();
      }
    } catch (_) {
      try {
        settingsDialog.show?.();
      } catch (__) {}
    }
  }
  function closeSettingsDialog() {
    if (!settingsDialog) return;
    try {
      settingsDialog.close?.();
    } catch (_) {}
  }
  $('#settings-toggle')?.addEventListener('click', openSettingsDialog);
  $('#settings-inspector-btn')?.addEventListener('click', openSettingsDialog);
  $$('[data-settings-close]', settingsDialog).forEach((btn) =>
    btn.addEventListener('click', closeSettingsDialog),
  );
  settingsDialog?.addEventListener('click', (event) => {
    if (event.target === settingsDialog) closeSettingsDialog();
  });
  $('#setting-suppress-hid-timeout')?.addEventListener('change', (e) => {
    setSetting('suppress_hid_timeout', e.target.checked);
    toast(t('settings_saved', '设置已保存'));
  });
  $('#setting-capture-pulse')?.addEventListener('change', (e) => {
    setSetting('capture_pulse', e.target.checked);
    toast(t('settings_saved', '设置已保存'));
  });
  $('#setting-paste-shortcut')?.addEventListener('change', (e) => {
    setSetting('paste_shortcut', e.target.checked);
    toast(t('settings_saved', '设置已保存'));
  });
  $('#setting-release-toast')?.addEventListener('change', (e) => {
    setSetting('release_key_toast', e.target.checked);
    toast(t('settings_saved', '设置已保存'));
  });
  $('#setting-render-pixelated')?.addEventListener('change', (e) => {
    applyRenderPixelated(e.target.checked);
    toast(t('settings_saved', '设置已保存'));
  });
  $('#setting-auto-inspector')?.addEventListener('change', (e) => {
    setSetting('auto_expand_inspector', e.target.checked);
    toast(t('settings_saved', '设置已保存'));
  });
  $('#setting-proxy-protocol')?.addEventListener('change', async (e) => {
    const select = e.target;
    const mode = select.value;
    const previousMode = unwrapConfig(bootstrap).server?.proxy_protocol || 'disabled';
    // This is a server listener setting that applies to every new connection
    // immediately; "required" locks out any client not behind a PPv2 proxy.
    const warning =
      mode === 'required'
        ? t(
            'ppv2_confirm_required',
            '“强制要求”会立即拒绝所有未经过 PPv2 代理的连接，包括你当前的直连访问。确认网页只通过已配置 PPv2 的反向代理访问后再继续。',
          )
        : mode === 'auto'
          ? t('ppv2_confirm_auto', '“自动检测”会信任连接开头的 PPv2 头。只应在监听端口仅对可信代理开放时启用。继续？')
          : null;
    if (warning && !confirm(warning)) {
      select.value = previousMode;
      return;
    }
    select.disabled = true;
    try {
      const saved = await request('/api/config', {
        method: 'POST',
        body: JSON.stringify({ server: { proxy_protocol: mode } }),
      });
      const config = unwrapConfig(bootstrap);
      if (!config.server) config.server = {};
      config.server.proxy_protocol = saved.server.proxy_protocol;
      select.value = config.server.proxy_protocol;
      toast(t('settings_saved', '设置已保存'));
      if (mode !== 'disabled') {
        // A fresh connection is negotiated under the new mode; if it fails the
        // operator needs recovery instructions while this page still works.
        fetch('/healthz', { cache: 'no-store' }).catch(() =>
          toast(
            t(
              'ppv2_probe_failed',
              '新连接已无法访问本服务。请通过已配置 PPv2 的代理访问，或在设备上把 config.json 的 server.proxy_protocol 改回 disabled 后重启服务。',
            ),
            true,
          ),
        );
      }
    } catch (err) {
      select.value = previousMode;
      toast(err.message || '保存失败', true);
    } finally {
      select.disabled = false;
    }
  });
  $('#setting-reset-btn')?.addEventListener('click', () => {
    if (!confirm(t('settings_reset_confirm', '确定要恢复所有全局偏好设置为默认值吗？'))) return;
    Object.keys(SETTING_DEFAULTS).forEach((key) => {
      try {
        localStorage.removeItem(`wingman_setting_${key}`);
      } catch (_) {}
    });
    syncSettingsDialogUi();
    applyRenderPixelated(SETTING_DEFAULTS.render_pixelated);
    toast(t('settings_reset_done', '已恢复默认设置'));
  });

  applyTheme(getPreferredTheme());
  applyRenderPixelated(getSetting('render_pixelated', true));
  setDisplayScale(readVideoUiPreference('scale', 'fit'));
  collectStaticPhrases();
  try {
    const savedLang = localStorage.getItem('wingman_lang');
    if (savedLang && ['zh-CN', 'zh-TW', 'en'].includes(savedLang)) setLanguage(savedLang);
    else setLanguage('zh-CN');
  } catch (_) {
    setLanguage('zh-CN');
  }
  consumeSetupToken();
  start();
})();
