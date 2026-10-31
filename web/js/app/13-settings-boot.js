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
    const layoutSelect = $('#setting-target-layout');
    if (layoutSelect) layoutSelect.value = targetLayout();
  }
  function openSettingsDialog() {
    if (!settingsDialog) return;
    syncSettingsDialogUi();
    refreshPasskeys();
    refreshSessions();
    refreshAudit();
    refreshNotifications();
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
  $('#setting-target-layout')?.addEventListener('change', (e) => {
    setSetting('target_layout', e.target.value);
    updatePasteLayoutLabel();
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
    updatePasteLayoutLabel();
    toast(t('settings_reset_done', '已恢复默认设置'));
  });

  const configExportBtn = $('#config-export-btn');
  const configImportBtn = $('#config-import-btn');
  const configImportFile = $('#config-import-file');
  const configImportDialog = $('#config-import-dialog');
  const configImportConfirm = $('#config-import-confirm');
  let pendingConfigImport = null;
  let configImportBusy = false;

  async function configResponseError(response, fallback) {
    try {
      const data = await response.json();
      if (data && data.error) return data.error;
    } catch (_) {}
    return fallback || `请求失败 (${response.status})`;
  }

  function configFilename(response) {
    const disposition = response.headers.get('content-disposition') || '';
    const match = /filename="?([^";]+)"?/i.exec(disposition);
    return match ? match[1] : `wingmankvm-config-${new Date().toISOString().slice(0, 10)}.json`;
  }

  async function exportConfigFile() {
    if (!configExportBtn || configExportBtn.disabled) return;
    configExportBtn.disabled = true;
    try {
      const response = await fetch('/api/config/export', {
        cache: 'no-store',
        credentials: 'same-origin',
      });
      if (!response.ok) throw new Error(await configResponseError(response, '导出失败'));
      const blob = await response.blob();
      const url = URL.createObjectURL(blob);
      const link = document.createElement('a');
      link.href = url;
      link.download = configFilename(response);
      link.rel = 'noopener';
      document.body.append(link);
      link.click();
      link.remove();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
      toast('配置已导出');
    } catch (err) {
      toast(err.message || '导出失败', true);
    } finally {
      configExportBtn.disabled = false;
    }
  }

  function setConfigImportStatus(message, bad = false) {
    const status = $('#config-import-status');
    if (!status) return;
    status.textContent = message;
    status.classList.toggle('error', bad);
  }

  function formatConfigValue(value) {
    if (value === undefined) return '—';
    try {
      return JSON.stringify(value);
    } catch (_) {
      return String(value);
    }
  }

  function renderConfigChanges(changes) {
    const list = $('#config-import-diff');
    if (!list) return;
    list.textContent = '';
    if (!changes.length) {
      const empty = document.createElement('p');
      empty.className = 'config-diff-empty';
      empty.textContent = '没有差异';
      list.append(empty);
      return;
    }
    for (const change of changes) {
      const item = document.createElement('div');
      item.className = 'config-diff-item';
      const path = document.createElement('code');
      path.className = 'config-diff-path';
      path.textContent = change.path;
      const values = document.createElement('span');
      values.className = 'config-diff-values';
      const oldValue = document.createElement('span');
      oldValue.className = 'config-diff-old';
      oldValue.textContent = formatConfigValue(change.old);
      const arrow = document.createElement('span');
      arrow.textContent = '→';
      const newValue = document.createElement('span');
      newValue.className = 'config-diff-new';
      newValue.textContent = formatConfigValue(change.new);
      values.append(oldValue, arrow, newValue);
      item.append(path, values);
      list.append(item);
    }
  }

  function openConfigImportDialog() {
    if (!configImportDialog) return;
    try {
      if (typeof configImportDialog.showModal === 'function') configImportDialog.showModal();
      else configImportDialog.show?.();
    } catch (_) {
      try {
        configImportDialog.show?.();
      } catch (__) {}
    }
  }

  function resetConfigImportDialog() {
    pendingConfigImport = null;
    configImportBusy = false;
    if (configImportConfirm) configImportConfirm.disabled = true;
    const summary = $('#config-import-summary');
    if (summary) summary.textContent = '';
    setConfigImportStatus('');
    renderConfigChanges([]);
  }

  function closeConfigImportDialog() {
    resetConfigImportDialog();
    try {
      configImportDialog?.close?.();
    } catch (_) {}
  }

  async function previewConfigImport(file) {
    if (!file || !configImportDialog) return;
    resetConfigImportDialog();
    openConfigImportDialog();
    if (file.size > 256 * 1024) {
      setConfigImportStatus('配置文件超过 256 KiB 限制', true);
      return;
    }
    let text;
    try {
      text = await file.text();
    } catch (_) {
      setConfigImportStatus('无法读取配置文件', true);
      return;
    }
    pendingConfigImport = text;
    configImportBusy = true;
    try {
      const preview = await request('/api/config/import/preview', { method: 'POST', body: text });
      const changes = Array.isArray(preview.changes) ? preview.changes : [];
      renderConfigChanges(changes);
      const summary = $('#config-import-summary');
      if (summary) summary.textContent = `共 ${changes.length} 处更改`;
      if (configImportConfirm) configImportConfirm.disabled = changes.length === 0;
      if (preview.restart_required) {
        setConfigImportStatus('导入后需要重启服务以应用新的监听地址或端口。');
      } else {
        setConfigImportStatus(
          changes.length ? '预览完成，确认后才会写入配置。' : '当前配置与导入文件一致，无需更改。',
        );
      }
    } catch (err) {
      pendingConfigImport = null;
      renderConfigChanges([]);
      const summary = $('#config-import-summary');
      if (summary) summary.textContent = '';
      setConfigImportStatus(`导入失败：${err.message}`, true);
    } finally {
      configImportBusy = false;
    }
  }

  async function applyConfigImport() {
    if (!pendingConfigImport || configImportBusy || !configImportConfirm) return;
    configImportBusy = true;
    configImportConfirm.disabled = true;
    try {
      const result = await request('/api/config/import', {
        method: 'POST',
        body: pendingConfigImport,
      });
      pendingConfigImport = null;
      const count = Array.isArray(result.changes) ? result.changes.length : 0;
      if (result.restart_required) {
        setConfigImportStatus(
          `配置导入成功：共 ${count} 处更改。新的监听地址或端口将在重启服务后生效。`,
        );
      } else {
        setConfigImportStatus(`配置导入成功：共 ${count} 处更改。`);
      }
      toast('配置导入成功');
    } catch (err) {
      setConfigImportStatus(`导入失败：${err.message}`, true);
      configImportConfirm.disabled = false;
    } finally {
      configImportBusy = false;
    }
  }

  configExportBtn?.addEventListener('click', exportConfigFile);
  configImportBtn?.addEventListener('click', () => {
    if (configImportFile) {
      configImportFile.value = '';
      configImportFile.click();
    }
  });
  configImportFile?.addEventListener('change', () => {
    const file = configImportFile.files && configImportFile.files[0];
    configImportFile.value = '';
    previewConfigImport(file);
  });
  configImportConfirm?.addEventListener('click', applyConfigImport);
  $('#config-import-cancel')?.addEventListener('click', closeConfigImportDialog);
  $('#config-import-dialog-close')?.addEventListener('click', closeConfigImportDialog);
  configImportDialog?.addEventListener('cancel', () => resetConfigImportDialog());
  configImportDialog?.addEventListener('click', (event) => {
    if (event.target === configImportDialog) closeConfigImportDialog();
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
