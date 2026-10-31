  function sessionMethodLabel(method) {
    if (method === 'password') return t('session_method_password', '密码登录');
    if (method === 'passkey') return t('session_method_passkey', 'Passkey 登录');
    if (method === 'setup') return t('session_method_setup', '初始化登录');
    return t('session_method_unknown', '未知方式');
  }
  function describeUserAgent(userAgent) {
    if (!userAgent) return t('sessions_unknown_device', '未知设备');
    const os = userAgent.includes('iPhone')
      ? 'iPhone'
      : userAgent.includes('iPad')
        ? 'iPad'
        : userAgent.includes('Android')
          ? 'Android'
          : userAgent.includes('Windows')
            ? 'Windows'
            : userAgent.includes('Macintosh') || userAgent.includes('Mac OS X')
              ? 'macOS'
              : userAgent.includes('Linux')
                ? 'Linux'
                : '';
    const browser = userAgent.includes('Edg/')
      ? 'Edge'
      : userAgent.includes('Firefox/')
        ? 'Firefox'
        : userAgent.includes('Chrome/') || userAgent.includes('CriOS/')
          ? 'Chrome'
          : userAgent.includes('Safari/')
            ? 'Safari'
            : '';
    const label = [os, browser].filter(Boolean).join(' · ');
    return label || userAgent.slice(0, 60);
  }
  function relativeTime(unixMs) {
    const age = Date.now() - Number(unixMs || 0);
    if (!Number.isFinite(age) || age < 60_000) return tr('刚刚');
    if (age < 3_600_000) return tr(`${Math.round(age / 60_000)} 分钟前`);
    if (age < 86_400_000) return tr(`${Math.round(age / 3_600_000)} 小时前`);
    return tr(`${Math.round(age / 86_400_000)} 天前`);
  }
  function addMetaPart(container, text) {
    if (!text) return;
    if (container.childNodes.length) container.append(document.createTextNode(' · '));
    container.append(document.createTextNode(text));
  }
  function setInlineError(id, message) {
    const box = $(id);
    if (!box) return;
    if (message) {
      box.textContent = message;
      box.classList.remove('hidden');
    } else {
      box.textContent = '';
      box.classList.add('hidden');
    }
  }
  async function finishSessionRevocation() {
    closeSettingsDialog();
    clearAuthSecrets({ token: true });
    await stopInput();
    bootstrap.authenticated = false;
    showState('login');
  }

  const sessionsTicket = latestOnly();
  async function refreshSessions() {
    const isCurrent = sessionsTicket();
    const container = $('#session-items');
    setInlineError('#sessions-error', '');
    try {
      const response = await request('/api/sessions');
      if (!isCurrent() || !container) return;
      const sessions = response.sessions || [];
      container.replaceChildren();
      if (!sessions.length) {
        const empty = document.createElement('div');
        empty.className = 'hint';
        empty.textContent = t('sessions_empty', '当前没有登录会话');
        container.append(empty);
        return;
      }
      sessions.forEach((session) => {
        const item = document.createElement('div');
        item.className = 'media-item';
        item.setAttribute('role', 'listitem');
        item.dataset.sessionId = session.id || '';
        item.dataset.sessionCurrent = session.current ? 'true' : 'false';
        item.style.padding = '8px 10px';
        item.style.marginBottom = '6px';

        const head = document.createElement('div');
        head.className = 'media-item-head';
        const nameBox = document.createElement('span');
        nameBox.className = 'media-item-name';
        const title = document.createElement('strong');
        title.textContent = describeUserAgent(session.user_agent);
        const meta = document.createElement('span');
        meta.className = 'media-item-meta';
        addMetaPart(meta, session.client_ip || t('sessions_unknown_ip', '未知地址'));
        addMetaPart(meta, sessionMethodLabel(session.login_method));
        addMetaPart(meta, relativeTime(session.last_seen_unix_ms));
        nameBox.append(title, meta);
        if (session.current) {
          const badge = document.createElement('span');
          badge.className = 'hint session-current-badge';
          badge.textContent = t('sessions_current', '当前会话');
          nameBox.append(badge);
        }
        head.append(nameBox);
        item.append(head);

        const controls = document.createElement('div');
        controls.className = 'media-item-controls';
        const button = document.createElement('button');
        button.type = 'button';
        button.className = session.current ? 'secondary' : 'danger';
        button.style.minHeight = '28px';
        button.style.padding = '0 8px';
        button.style.fontSize = '12px';
        button.dataset.sessionRevoke = session.id || '';
        button.textContent = session.current
          ? t('sessions_logout', '退出登录')
          : t('sessions_revoke', '下线');
        button.addEventListener('click', async () => {
          if (button.disabled) return;
          const prompt = session.current
            ? t('sessions_logout_confirm', '退出登录会结束当前会话，确定继续吗？')
            : t('sessions_revoke_confirm', '确定要让该设备下线吗？该设备上的连接会立即断开。');
          if (!confirm(prompt)) return;
          button.disabled = true;
          try {
            await request('/api/sessions/revoke', {
              method: 'POST',
              body: JSON.stringify({ id: session.id }),
            });
            if (session.current) {
              await finishSessionRevocation();
              return;
            }
            toast(t('sessions_revoked', '已下线'));
            await refreshSessions();
          } catch (err) {
            toast(err.message || t('sessions_revoke_failed', '下线失败'), true);
          } finally {
            button.disabled = false;
          }
        });
        controls.append(button);
        item.append(controls);
        container.append(item);
      });
    } catch (err) {
      if (!isCurrent()) return;
      container?.replaceChildren();
      setInlineError(
        '#sessions-error',
        `${t('sessions_error', '无法读取登录会话')}：${err.message || ''}`,
      );
    }
  }

  function auditEventLabel(event) {
    const labels = {
      login_succeeded: ['audit_event_login_succeeded', '登录成功'],
      login_failed: ['audit_event_login_failed', '登录失败'],
      logout: ['audit_event_logout', '退出登录'],
      session_revoked: ['audit_event_session_revoked', '会话下线'],
      sessions_revoked_others: ['audit_event_sessions_revoked_others', '其他设备下线'],
      power_action: ['audit_event_power_action', '电源操作'],
      gpio_test: ['audit_event_gpio_test', 'GPIO 测试'],
      config_changed: ['audit_event_config_changed', '配置变更'],
      passkey_added: ['audit_event_passkey_added', '添加 Passkey'],
      passkey_removed: ['audit_event_passkey_removed', '删除 Passkey'],
      setup_completed: ['audit_event_setup_completed', '初始化完成'],
    };
    const entry = labels[event];
    return entry ? t(entry[0], entry[1]) : event || '';
  }
  function auditTimestamp(unixMs) {
    const date = new Date(Number(unixMs || 0));
    if (Number.isNaN(date.getTime())) return '';
    return date.toLocaleString([], {
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    });
  }

  const auditTicket = latestOnly();
  async function refreshAudit() {
    const isCurrent = auditTicket();
    const container = $('#audit-items');
    setInlineError('#audit-error', '');
    try {
      const response = await request('/api/audit?limit=50');
      if (!isCurrent() || !container) return;
      const entries = response.entries || [];
      container.replaceChildren();
      if (!entries.length) {
        const empty = document.createElement('div');
        empty.className = 'hint';
        empty.textContent = t('audit_empty', '暂无审计记录');
        container.append(empty);
        return;
      }
      entries.forEach((entry) => {
        const item = document.createElement('div');
        item.className = 'media-item audit-item';
        item.setAttribute('role', 'listitem');
        item.style.padding = '6px 10px';
        item.style.marginBottom = '4px';
        const head = document.createElement('div');
        head.className = 'media-item-head';
        const nameBox = document.createElement('span');
        nameBox.className = 'media-item-name';
        const label = document.createElement('strong');
        label.textContent = auditEventLabel(entry.event);
        const time = document.createElement('span');
        time.className = 'media-item-meta';
        time.textContent = auditTimestamp(entry.ts);
        nameBox.append(label, time);
        head.append(nameBox);
        const meta = document.createElement('span');
        meta.className = 'media-item-meta media-item-name';
        meta.textContent = [entry.ip || '', entry.detail || ''].filter(Boolean).join(' · ');
        item.append(head, meta);
        container.append(item);
      });
    } catch (err) {
      if (!isCurrent()) return;
      container?.replaceChildren();
      setInlineError(
        '#audit-error',
        `${t('audit_error', '无法读取审计日志')}：${err.message || ''}`,
      );
    }
  }

  $('#sessions-revoke-others')?.addEventListener('click', async (event) => {
    const button = event.currentTarget;
    if (button.disabled) return;
    if (!confirm(t('sessions_revoke_others_confirm', '确定要让其他所有设备下线吗？其他设备上的连接会立即断开。')))
      return;
    button.disabled = true;
    try {
      const result = await request('/api/sessions/revoke-others', { method: 'POST' });
      toast(tr(`已让其他设备下线（${result.revoked ?? 0} 个）`));
      await refreshSessions();
    } catch (err) {
      toast(err.message || t('sessions_revoke_others_failed', '其他设备下线失败'), true);
    } finally {
      button.disabled = false;
    }
  });
  $('#audit-details')?.addEventListener('toggle', (event) => {
    if (event.target.open) refreshAudit();
  });
