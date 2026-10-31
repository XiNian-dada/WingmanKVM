  (() => {
    'use strict';
    const $ = (s, root = document) => root.querySelector(s);
    const $$ = (s, root = document) => [...root.querySelectorAll(s)];
    const authView = $('#auth-view'), app = $('#app'), setupForm = $('#setup-form'), loginForm = $('#login-form');
    const viewport = $('#video-viewport'), feed = $('#video-feed'), webrtcFeed = $('#webrtc-feed'), consoleBox = $('#console');
    const fieldTooltip = $('#field-tooltip');
    const I18N = {
      'zh-CN': {
        power:'电源',power_status:'电源状态',short_press:'短按电源开机/关机',reset_pc:'复位重启 (Reset)',force_off:'强制关机 (长按 5 秒)',
        keyboard:'键盘',inspector:'控制面板',inspector_close:'收起',
        diag_title:'连接与设备诊断',diag_transport:'视频传输链路',diag_h264_failure:'H.264 回退原因',diag_resolution:'采集画面分辨率',diag_input:'键鼠控制模式',diag_power_led:'被控机电源 LED',diag_client_ip:'客户端来源地址',diag_reconnect:'重连视频',diag_scan:'扫描设备',
        tab_remote_screen:'远程画面',tab_terminal:'终端',release_keys:'释放按键',reconnect:'重连',clear:'清空',mode_fit:'适应',pan_mode:'平移',fullscreen:'全屏',exit_fullscreen:'退出全屏',mode_prefix:'画面比例: ',
        tab_control:'控制',tab_video:'视频',tab_devices:'设备',tab_media:'介质',remote_control:'远程控制',forward_input:'转发键鼠',
        short_press_btn:'短按电源',reset_btn:'复位',force_off_btn:'强制关机',refresh:'刷新',
        preset_desktop:'桌面优化 (1080p 60fps 8M)',preset_bios:'BIOS / UEFI (720p 30fps MJPEG)',preset_bandwidth:'低带宽省流 (720p 25fps 2M)',
        apply:'应用',rescan:'重新检测',save:'保存',upload:'上传',logout:'退出登录',virtual_keyboard:'虚拟键盘',close:'收起',
        input_paused:'输入已暂停',input_captured:'相对鼠标已捕获',input_click_to_capture:'点击画面捕获鼠标',input_forwarding:'键鼠正在转发',input_released:'已释放所有按键与鼠标锁定',
        theme_dark:'深色模式',theme_light:'浅色模式',theme_system:'跟随系统',switch_theme:'切换外观主题',
        connecting:'正在连接…',video_connecting:'正在连接视频…',video_paused:'视频已暂停',device_offline:'设备离线',
        collapse_inspector:'收起面板',open_inspector:'控制面板',lang_name:'简',
        passkey_title:'Passkey 通行密钥',passkey_desc:'绑定 Apple Touch ID / Face ID，无需密码快速登录',
        passkey_add:'+ 绑定此设备',passkey_signin:'使用 Passkey (Touch ID / Face ID) 登录',passkey_none:'尚未绑定任何 Passkey',
        passkey_delete:'删除',passkey_prompt_name:'为新 Passkey 命名（例如：MacBook Pro、iPhone）：',
        passkey_insecure:'Passkey 需要在安全连接（HTTPS 或 localhost）下使用',
        passkey_ip_warn:'WebAuthn 规范要求使用域名（如 localhost 或 wingman.local），不支持 IP 地址',
        passkey_added_success:'Passkey 绑定成功！',passkey_delete_confirm:'确定要删除此 Passkey 吗？',
        settings:'设置',settings_title:'全局偏好设置',settings_desc:'配置交互反馈、通知拦截与全局渲染偏好',
        settings_group_input:'交互与通知',
        settings_suppress_hid:'屏蔽 HID 写入超时警告',settings_suppress_hid_desc:'在纯 Linux 环境或被控端未开机时，忽略鼠标与键盘写入超时 (writable timeout) 弹窗警告',
        settings_capture_pulse:'接管边缘微光脉冲',settings_capture_pulse_desc:'激活键鼠转发时，在远端画面边缘显示光晕视觉反馈',
        settings_release_toast:'按键释放提示',settings_release_toast_desc:'执行「释放按键」或切换控制权时弹出状态提示',
        settings_group_display:'画面与渲染',
        settings_render_pixelated:'像素锐利渲染 (Pixelated)',settings_render_pixelated_desc:'近邻采样字符边缘无模糊滤镜，适合 BIOS 与纯文本终端',
        settings_auto_inspector:'宽屏默认展开面板',settings_auto_inspector_desc:'屏幕宽度充足时，进入控制台默认展开右侧控制面板',
        settings_group_network:'网络与代理',
        settings_ppv2:'PROXY Protocol v2 (PPv2)',settings_ppv2_desc:'解析并透传反向代理 (HAProxy / Nginx) 转发的真实客户端 IP',
        proxy_disabled:'关闭',proxy_auto:'自动检测',proxy_required:'强制要求',
        settings_group_system:'系统与重置',
        settings_reset_btn:'恢复默认设置',settings_reset_desc:'清除所有保存在本机的偏好设置并还原为初始默认值',
        settings_reset_action:'还原',settings_reset_confirm:'确定要恢复所有全局偏好设置为默认值吗？',settings_reset_done:'已恢复默认设置',settings_saved:'设置已保存',
        keyboard_leds_title:'键盘锁定状态',keyboard_leds_desc:'远程主机键盘锁定指示灯（点击快速切换）',
        host_performance:'主控机性能',cpu_usage:'CPU',cpu_cores:'核',load:'负载',load_average:'系统负载',memory_usage:'内存',mem_short:'内存',disk_usage:'存储',temperature:'主机温度',uptime:'运行时间',full_diagnostics:'完整诊断 ↗',perf_hud_title:'主控机性能 (点击查看详情)',
        led_num:'数字锁定 (Num Lock)',led_caps:'大写锁定 (Caps Lock)',led_scroll:'滚动锁定 (Scroll Lock)'
      },
      'zh-TW': {
        power:'電源',power_status:'電源狀態',short_press:'短按電源開機/關機',reset_pc:'重置重啟 (Reset)',force_off:'強制關機 (長按 5 秒)',
        keyboard:'鍵盤',inspector:'控制面板',inspector_close:'收起',
        diag_title:'連線與設備診斷',diag_transport:'視訊傳輸鏈路',diag_h264_failure:'H.264 回退原因',diag_resolution:'採集畫面解析度',diag_input:'鍵鼠控制模式',diag_power_led:'被控機電源 LED',diag_client_ip:'客戶端來源位址',diag_reconnect:'重新連線視訊',diag_scan:'掃描設備',
        tab_remote_screen:'遠端畫面',tab_terminal:'終端機',release_keys:'釋放按鍵',reconnect:'重新連線',clear:'清除',mode_fit:'適應',pan_mode:'平移',fullscreen:'全螢幕',exit_fullscreen:'退出全螢幕',mode_prefix:'畫面比例: ',
        tab_control:'控制',tab_video:'視訊',tab_devices:'設備',tab_media:'媒體',remote_control:'遠端控制',forward_input:'轉發鍵鼠',
        short_press_btn:'短按電源',reset_btn:'重置',force_off_btn:'強制關機',refresh:'重新整理',
        preset_desktop:'桌面最佳化 (1080p 60fps 8M)',preset_bios:'BIOS / UEFI (720p 30fps MJPEG)',preset_bandwidth:'低頻寬省流 (720p 25fps 2M)',
        apply:'套用',rescan:'重新檢測',save:'儲存',upload:'上傳',logout:'登出',virtual_keyboard:'虛擬鍵盤',close:'收起',
        input_paused:'輸入已暫停',input_captured:'相對滑鼠已捕獲',input_click_to_capture:'點擊畫面捕獲滑鼠',input_forwarding:'鍵鼠正在轉發',input_released:'已釋放所有按鍵與滑鼠鎖定',
        theme_dark:'深色模式',theme_light:'淺色模式',theme_system:'跟隨系統',switch_theme:'切換外觀主題',
        connecting:'正在連線…',video_connecting:'正在連線視訊…',video_paused:'視訊已暫停',device_offline:'設備離線',
        collapse_inspector:'收起面板',open_inspector:'控制面板',lang_name:'繁',
        passkey_title:'Passkey 通行密鑰',passkey_desc:'綁定 Apple Touch ID / Face ID，無需密碼快速登入',
        passkey_add:'+ 綁定此設備',passkey_signin:'使用 Passkey (Touch ID / Face ID) 登入',passkey_none:'尚未綁定任何 Passkey',
        passkey_delete:'刪除',passkey_prompt_name:'為新 Passkey 命名（例如：MacBook Pro、iPhone）：',
        passkey_insecure:'Passkey 需要在安全連線（HTTPS 或 localhost）下使用',
        passkey_ip_warn:'WebAuthn 規範要求使用網域名稱（如 localhost 或 wingman.local），不支援 IP 位址',
        passkey_added_success:'Passkey 綁定成功！',passkey_delete_confirm:'確定要刪除此 Passkey 嗎？',
        settings:'設定',settings_title:'全局偏好設定',settings_desc:'配置交互反饋、通知攔截與全局渲染偏好',
        settings_group_input:'交互與通知',
        settings_suppress_hid:'屏蔽 HID 寫入逾時警告',settings_suppress_hid_desc:'在純 Linux 環境或被控端未開機時，忽略滑鼠與鍵盤寫入逾時 (writable timeout) 彈窗警告',
        settings_capture_pulse:'接管邊緣微光脈衝',settings_capture_pulse_desc:'激活鍵鼠轉發時，在遠端畫面邊緣顯示光暈視覺反饋',
        settings_release_toast:'按鍵釋放提示',settings_release_toast_desc:'執行「釋放按鍵」或切換控制權時彈出狀態提示',
        settings_group_display:'畫面與渲染',
        settings_render_pixelated:'像素銳利渲染 (Pixelated)',settings_render_pixelated_desc:'近鄰採樣字符邊緣無模糊濾鏡，適合 BIOS 與純文本終端',
        settings_auto_inspector:'寬螢幕預設展開面板',settings_auto_inspector_desc:'螢幕寬度充足時，進入控制台預設展開右側控制面板',
        settings_group_network:'網路與代理',
        settings_ppv2:'PROXY Protocol v2 (PPv2)',settings_ppv2_desc:'解析並透傳反向代理 (HAProxy / Nginx) 轉發的真實客戶端 IP',
        proxy_disabled:'關閉',proxy_auto:'自動檢測',proxy_required:'強制要求',
        settings_group_system:'系統與重設',
        settings_reset_btn:'恢復預設設定',settings_reset_desc:'清除所有保存在本機的偏好設定並還原為初始預設值',
        settings_reset_action:'還原',settings_reset_confirm:'確定要恢復所有全局偏好設定為預設值嗎？',settings_reset_done:'已恢復預設設定',settings_saved:'設定已儲存',
        keyboard_leds_title:'鍵盤鎖定狀態',keyboard_leds_desc:'遠端主機鍵盤鎖定指示燈（點擊快速切換）',
        host_performance:'主控機效能',cpu_usage:'CPU',cpu_cores:'核',load:'負載',load_average:'系統負載',memory_usage:'記憶體',mem_short:'記憶體',disk_usage:'儲存',temperature:'主機溫度',uptime:'運行時間',full_diagnostics:'完整診斷 ↗',perf_hud_title:'主控機效能 (點擊查看詳情)',
        led_num:'數字鎖定 (Num Lock)',led_caps:'大寫鎖定 (Caps Lock)',led_scroll:'捲動鎖定 (Scroll Lock)'
      },
      'en': {
        power:'Power',power_status:'Power Status',short_press:'Power On / Off (Short Press)',reset_pc:'System Reset (Reset)',force_off:'Force Power Off (Hold 5s)',
        keyboard:'Keyboard',inspector:'Inspector',inspector_close:'Collapse',
        diag_title:'Diagnostics',diag_transport:'Video Transport',diag_h264_failure:'H.264 Fallback Reason',diag_resolution:'Capture Resolution',diag_input:'Input Mode',diag_power_led:'Target Power LED',diag_client_ip:'Client Address',diag_reconnect:'Reconnect Video',diag_scan:'Scan Devices',
        tab_remote_screen:'Remote Screen',tab_terminal:'Terminal',release_keys:'Release Keys',reconnect:'Reconnect',clear:'Clear',mode_fit:'Fit',pan_mode:'Pan',fullscreen:'Fullscreen',exit_fullscreen:'Exit Fullscreen',mode_prefix:'Display Mode: ',
        tab_control:'Control',tab_video:'Video',tab_devices:'Devices',tab_media:'Media',remote_control:'Remote Control',forward_input:'Forward Input',
        short_press_btn:'Power',reset_btn:'Reset',force_off_btn:'Force Off',refresh:'Refresh',
        preset_desktop:'Desktop (1080p 60fps 8M)',preset_bios:'BIOS / UEFI (720p 30fps MJPEG)',preset_bandwidth:'Low Bandwidth (720p 25fps 2M)',
        apply:'Apply',rescan:'Rescan',save:'Save',upload:'Upload',logout:'Log Out',virtual_keyboard:'Virtual Keyboard',close:'Close',
        input_paused:'Input Paused',input_captured:'Relative Mouse Captured',input_click_to_capture:'Click to Capture Mouse',input_forwarding:'Input Active',input_released:'All keys and pointer locks released',
        theme_dark:'Dark Mode',theme_light:'Light Mode',theme_system:'System Theme',switch_theme:'Toggle Theme',
        connecting:'Connecting…',video_connecting:'Connecting video…',video_paused:'Video paused',device_offline:'Device offline',
        collapse_inspector:'Collapse',open_inspector:'Inspector',lang_name:'EN',
        passkey_title:'Passkey Credentials',passkey_desc:'Sign in with Apple Touch ID / Face ID or Windows Hello',
        passkey_add:'+ Add This Device',passkey_signin:'Sign in with Passkey',passkey_none:'No Passkeys registered yet',
        passkey_delete:'Delete',passkey_prompt_name:'Enter a name for this Passkey (e.g. MacBook Pro, iPhone):',
        passkey_insecure:'Passkey requires a secure context (HTTPS or localhost)',
        passkey_ip_warn:'WebAuthn requires a domain name (e.g. localhost or wingman.local), IP addresses not supported',
        passkey_added_success:'Passkey registered successfully!',passkey_delete_confirm:'Are you sure you want to delete this Passkey?',
        settings:'Settings',settings_title:'Global Preferences',settings_desc:'Configure interaction feedback, alert silencing, and rendering preferences',
        settings_group_input:'Interaction & Alerts',
        settings_suppress_hid:'Silence HID Timeout Warnings',settings_suppress_hid_desc:'Suppress popup warnings when mouse/keyboard /dev/hidg device times out waiting to become writable',
        settings_capture_pulse:'Input Capture Edge Glow',settings_capture_pulse_desc:'Display a subtle blue edge glow feedback when activating remote keyboard/mouse capture',
        settings_release_toast:'Key Release Notification',settings_release_toast_desc:'Show toast notification when releasing held keys or toggling remote input',
        settings_group_display:'Display & Rendering',
        settings_render_pixelated:'Pixelated Sharp Scaling',settings_render_pixelated_desc:'Disable bilinear blur filtering for crisp characters in BIOS and text terminals',
        settings_auto_inspector:'Auto-expand Inspector on Wide Screens',settings_auto_inspector_desc:'Automatically keep the right-hand Inspector open on large screens',
        settings_group_network:'Network & Proxy',
        settings_ppv2:'PROXY Protocol v2 (PPv2)',settings_ppv2_desc:'Parse and forward real client IPs from reverse proxies (HAProxy / NGINX)',
        proxy_disabled:'Disabled',proxy_auto:'Auto Detect',proxy_required:'Required',
        settings_group_system:'System & Reset',
        settings_reset_btn:'Reset to Defaults',settings_reset_desc:'Clear all locally stored preferences and restore initial default values',
        settings_reset_action:'Reset',settings_reset_confirm:'Are you sure you want to reset all preferences to defaults?',settings_reset_done:'Preferences reset to defaults',settings_saved:'Settings saved',
        keyboard_leds_title:'Keyboard Lock LEDs',keyboard_leds_desc:'Remote host lock indicators (click to toggle)',
        host_performance:'Host Performance',cpu_usage:'CPU Usage',cpu_cores:'Cores',load:'Load',load_average:'Load Average',memory_usage:'Memory',mem_short:'RAM',disk_usage:'Storage',temperature:'Temperature',uptime:'Uptime',full_diagnostics:'Full Diagnostics ↗',perf_hud_title:'Host Performance (Click to view details)',
        led_num:'Num Lock',led_caps:'Caps Lock',led_scroll:'Scroll Lock'
      }
    };
    let currentLang = 'zh-CN';
    function t(key, fallback = '') { return I18N[currentLang]?.[key] ?? I18N['zh-CN']?.[key] ?? fallback ?? key; }
    function updateI18nElements() {
      $$('[data-i18n]').forEach(el => {
        const key = el.dataset.i18n;
        if (!key || !I18N[currentLang]?.[key]) return;
        const target = el.querySelector('.mode-text') || el.querySelector('.pan-text');
        if (target) target.textContent = I18N[currentLang][key];
        else if (el.querySelector('svg')) {
          el.title = I18N[currentLang][key];
          el.setAttribute('aria-label', I18N[currentLang][key]);
        } else {
          el.textContent = I18N[currentLang][key];
        }
      });
      const numTitle = t('led_num', 'Num Lock'), capsTitle = t('led_caps', 'Caps Lock'), scrlTitle = t('led_scroll', 'Scroll Lock');
      $('#led-num')?.setAttribute('title', numTitle);
      $('#led-caps')?.setAttribute('title', capsTitle);
      $('#led-scroll')?.setAttribute('title', scrlTitle);
      $('#inspector-led-num')?.setAttribute('title', numTitle);
      $('#inspector-led-caps')?.setAttribute('title', capsTitle);
      $('#inspector-led-scroll')?.setAttribute('title', scrlTitle);
      $('#topbar-perf-btn')?.setAttribute('title', t('perf_hud_title', '主控机性能 (点击查看详情)'));
      const langLabel = $('#lang-current-label');
      if (langLabel) langLabel.textContent = I18N[currentLang]?.lang_name || '简';
      $$('[data-lang]').forEach(btn => btn.classList.toggle('active', btn.dataset.lang === currentLang));
    }
    function setLanguage(lang) {
      currentLang = ['zh-CN','zh-TW','en'].includes(lang) ? lang : 'zh-CN';
      try { localStorage.setItem('wingman_lang', currentLang); } catch (_) {}
      document.documentElement.lang = currentLang;
      updateI18nElements();
      syncInputUi();
      renderVideoUi();
      if (app.classList.contains('inspector-open')) {
        $('#inspector-toggle').textContent = t('collapse_inspector', '收起面板');
      } else {
        $('#inspector-toggle').textContent = t('open_inspector', '控制面板');
      }
      if (latestSystemStatus) renderSystemMetrics(latestSystemStatus);
      if (!app.classList.contains('hidden')) {
        refreshPasskeys();
      } else if (!loginForm.classList.contains('hidden')) {
        updateLoginPasskeyUI();
      }
    }
    function getPreferredTheme() { try { return localStorage.getItem('wingman_theme') || 'system'; } catch (_) { return 'system'; } }
    function applyTheme(theme) {
      const root = document.documentElement, darkIcon = $('.theme-icon-dark'), lightIcon = $('.theme-icon-light'), toggleBtn = $('#theme-toggle');
      if (theme === 'dark') {
        root.setAttribute('data-theme', 'dark');
        darkIcon?.classList.add('hidden'); lightIcon?.classList.remove('hidden');
        if (toggleBtn) toggleBtn.title = t('theme_dark', '深色模式');
      } else if (theme === 'light') {
        root.setAttribute('data-theme', 'light');
        darkIcon?.classList.remove('hidden'); lightIcon?.classList.add('hidden');
        if (toggleBtn) toggleBtn.title = t('theme_light', '浅色模式');
      } else {
        root.removeAttribute('data-theme');
        const isDark = matchMedia('(prefers-color-scheme: dark)').matches;
        darkIcon?.classList.toggle('hidden', isDark); lightIcon?.classList.toggle('hidden', !isDark);
        if (toggleBtn) toggleBtn.title = t('theme_system', '跟随系统');
      }
      try { localStorage.setItem('wingman_theme', theme); } catch (_) {}
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
    let activeHelp = null, helpTimer = 0;
    function positionFieldTooltip() {
      if (!activeHelp || fieldTooltip.hidden) return;
      const anchor = activeHelp.getBoundingClientRect(), tip = fieldTooltip.getBoundingClientRect(), gap = 10, pad = 12;
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
      clearTimeout(helpTimer); activeHelp = null; fieldTooltip.classList.remove('visible');
      helpTimer = setTimeout(() => { if (!activeHelp) fieldTooltip.hidden = true; }, 160);
    }
    function showFieldTooltip(button) {
      const text = button.dataset.help?.trim(); if (!text) return;
      clearTimeout(helpTimer); activeHelp = button; fieldTooltip.textContent = text; fieldTooltip.hidden = false; fieldTooltip.classList.remove('visible');
      requestAnimationFrame(() => { if (activeHelp !== button) return; positionFieldTooltip(); fieldTooltip.classList.add('visible'); });
    }
    $$('[data-help]').forEach(button => {
      button.setAttribute('aria-describedby', 'field-tooltip');
      button.addEventListener('pointerenter', () => showFieldTooltip(button));
      button.addEventListener('pointerleave', () => { if (document.activeElement !== button) hideFieldTooltip(); });
      button.addEventListener('focus', () => showFieldTooltip(button));
      button.addEventListener('blur', hideFieldTooltip);
      button.addEventListener('click', event => { event.preventDefault(); event.stopPropagation(); showFieldTooltip(button); clearTimeout(helpTimer); helpTimer = setTimeout(hideFieldTooltip, 3200); });
    });
    addEventListener('resize', positionFieldTooltip, { passive: true });
    addEventListener('scroll', positionFieldTooltip, { passive: true, capture: true });
    let cachedSurfaceBox = null;
    function invalidateSurfaceBox() { cachedSurfaceBox = null; }
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
      const wsW = ws.clientWidth, wsH = ws.clientHeight;
      if (!wsW || !wsH) return;
      const inspectorOffset = (app.classList.contains('inspector-open') && !matchMedia('(max-width:900px)').matches) ? 360 : 0;
      const effectiveWsW = Math.max(320, wsW - inspectorOffset);
      const dim = getVideoDimensions();
      const ratio = (dim && dim.ratio > 0) ? dim.ratio : (16 / 9);
      const pad = 24, headH = 46, borderW = 2, borderH = 2;
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
    window.addEventListener('resize', () => { fitConsoleToVideo(); invalidateSurfaceBox(); }, { passive: true });
    window.addEventListener('scroll', invalidateSurfaceBox, { passive: true });
    document.addEventListener('fullscreenchange', () => { fitConsoleToVideo(); invalidateSurfaceBox(); });
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
    let isPanActive = false, isDraggingPan = false;
    let panStartX = 0, panStartY = 0, panScrollX = 0, panScrollY = 0;
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
    let bootstrap = {}, remoteWanted = false, pageActive = !document.hidden, reconnectTimer = 0, reconnectDelay = 500;
    let videoGeneration = 0, videoWorkspace = 'video', videoPreference = 'auto', activeVideoTransport = 'none', videoUiState = 'paused', videoFallback = false, videoFallbackReason = '';
    let webrtcPeer = null, webrtcSessionId = null, webrtcAbort = null, videoFirstFrameTimer = 0, statusTimer = 0, latestVideoStatus = null, latestPowerStatus = null, latestSystemStatus = null, videoConfigRequestId = 0;
    let toastTimer = 0, mouseX = 0, mouseY = 0, absolutePending = null, mouseBusy = false, mouseTimer = 0, lastMouseSend = 0, gpioTestBusy = false;
    const setupGuides={account:{kicker:'01 / 管理员',title:'账号只设置一次',items:[['网页与终端','管理员密码也会同步给本机终端用户 wingman。'],['强密码','至少 12 位，并包含大小写字母、数字和符号。'],['一次性链接','初始化令牌会从地址中自动读取并立即清除。']]},devices:{kicker:'02 / 连接',title:'先自动检测',items:[['唯一候选','验证通过且只有一个设备时直接采用。'],['多个设备','只在存在多个可用候选时让你选择。'],['手动配置','设备路径只留给自定义 Gadget 和特殊板卡。']]},video:{kicker:'视频采集',title:'认准视频节点',items:[['插在 Host 口','采集卡是输入设备，不要接到 OTG Device 口。'],['检查能力','选择同时支持 Video Capture 和 MJPG 的 USB 节点。'],['排除 metadata','只有 Metadata Capture 的 /dev/videoN 不会输出画面。']]},input:{kicker:'USB 控制',title:'优先精准同步',items:[['OTG Device 口','键盘、鼠标和虚拟 U 盘从这里连接被控机。'],['绝对指针','网页位置会直接映射到远端位置，默认优先使用。'],['相对鼠标','保留给只支持 Boot Mouse 的 BIOS 或 UEFI。']]},media:{kicker:'虚拟介质',title:'镜像就是一只 U 盘',items:[['ISO','始终按只读光驱挂载。'],['IMG','可作为读写 U 盘，但弹出前先在被控机卸载。'],['避免双写','同一镜像不要同时被 RK3399 和被控机写入。']]},power:{kicker:'GPIO 电源',title:'接线必须人工确认',items:[['使用继电器','用干接点并联被控机电源按钮，不要让 GPIO 直接承受外部电压。'],['线路与极性','软件能发现 gpiochip，但不能猜出 line 和高低电平。'],['可以跳过','先完成 KVM，确认原理图和接线后再启用电源控制。']]}};

    let setupGuideTrigger=null;
    function openSetupGuide(key,trigger){const guide=setupGuides[key];if(!guide)return;if(setupGuideTrigger)setupGuideTrigger.setAttribute('aria-expanded','false');setupGuideTrigger=trigger||null;if(setupGuideTrigger)setupGuideTrigger.setAttribute('aria-expanded','true');$('#auth-guide-kicker').textContent=guide.kicker;$('#auth-guide-title').textContent=guide.title;const body=$('#auth-guide-body');body.replaceChildren();for(const [title,copy] of guide.items){const item=document.createElement('div');item.className='auth-guide-item';const strong=document.createElement('strong');strong.textContent=title;const text=document.createElement('span');text.textContent=copy;item.append(strong,text);body.append(item);}$('#auth-story-home').classList.add('hidden');const panel=$('#auth-guide');panel.classList.remove('hidden');requestAnimationFrame(()=>{if(matchMedia('(max-width:720px)').matches)panel.scrollIntoView({block:'start',behavior:matchMedia('(prefers-reduced-motion:reduce)').matches?'auto':'smooth'});$('#auth-guide-back').focus({preventScroll:true});});}
    function closeSetupGuide(restoreFocus=true){$('#auth-guide').classList.add('hidden');$('#auth-story-home').classList.remove('hidden');const trigger=setupGuideTrigger;if(trigger)trigger.setAttribute('aria-expanded','false');setupGuideTrigger=null;if(restoreFocus&&trigger?.isConnected)requestAnimationFrame(()=>trigger.focus());}
    document.addEventListener('click',event=>{const button=event.target.closest?.('[data-guide]');if(!button)return;event.preventDefault();event.stopPropagation();openSetupGuide(button.dataset.guide,button);});
    $('#auth-guide-back').addEventListener('click',closeSetupGuide);

    function toast(message, bad = false) { const el = $('#toast'); el.textContent = message; el.style.borderColor = bad ? '#ee0000' : ''; el.style.color = bad ? '#c50000' : ''; el.classList.add('show'); clearTimeout(toastTimer); toastTimer = setTimeout(() => el.classList.remove('show'), 2600); }
    async function request(url, options = {}) {
      const headers = new Headers(options.headers || {}); if (options.body && !(options.body instanceof FormData) && !headers.has('content-type')) headers.set('content-type', 'application/json');
      const response = await fetch(url, {...options, headers, cache: 'no-store', credentials: 'same-origin'});
      const type = response.headers.get('content-type') || ''; const data = type.includes('json') ? await response.json() : await response.text();
      if (response.status === 401 && url !== '/api/login' && !app.classList.contains('hidden')) { clearAuthSecrets({token:true}); bootstrap.authenticated=false; stopInput(); showState('login'); $('#login-error').textContent='会话已过期，请重新登录。'; }
      if (!response.ok) throw new Error((data && (data.error || data.message)) || data || `请求失败 (${response.status})`); return data;
    }
    const rawValue = (form, name) => new FormData(form).get(name)?.toString() || '';
    const value = (form, name) => rawValue(form, name).trim();
    function clearAuthSecrets(options={}) { if(options.login!==false)loginForm.elements.password.value=''; if(options.setup!==false){setupForm.elements.password.value='';setupForm.elements.confirm_password.value='';} if(options.token){setupForm.elements.setup_token.value='';if(bootstrap?.setup_required&&bootstrap?.token_required!==false)setupForm.elements.setup_token.closest('.field')?.classList.remove('hidden');} }
    const optionalNumber = v => v === '' ? null : Number(v);
    function hardwareFrom(form) { const mouse=value(form,'mouse_device')||null,absolute=value(form,'absolute_pointer_device')||null,selected=value(form,'pointer_mode')==='relative'?'relative':'absolute';return {video_device:value(form,'video_device')||null,keyboard_device:value(form,'keyboard_device')||null,mouse_device:mouse,absolute_pointer_device:absolute,pointer_mode:selected,power_enabled:form.elements.power_enabled.checked,gpio_chip:value(form,'gpio_chip')||null,gpio_line:optionalNumber(value(form,'gpio_line')),active_high:value(form,'active_high')!=='false',media_enabled:form.elements.media_enabled.checked,lun_path:value(form,'lun_path')||null,image_directory:value(form,'image_directory')||null}; }
    function showState(state) {
      const wasMain=!app.classList.contains('hidden');
      if(state==='main')clearAuthSecrets({token:true}); else if(state==='login'&&wasMain)clearAuthSecrets({token:true});
      $('#boot-panel').classList.add('hidden'); setupForm.classList.toggle('hidden', state !== 'setup'); loginForm.classList.toggle('hidden', state !== 'login'); authView.classList.toggle('hidden', state === 'main'); app.classList.toggle('hidden', state !== 'main');if(state!=='setup')closeSetupGuide(false);
      if (state === 'main') {
        pageActive=!document.hidden; applyBootstrap(); startVideo(); refreshStatus(); scanDevices();
        let defaultOpen=!matchMedia('(max-width:900px)').matches;
        try{const saved=localStorage.getItem('wingman_inspector_open');if(saved!==null)defaultOpen=saved==='true';}catch(_){}
        setInspectorOpen(defaultOpen);
        if(bootstrap.capabilities?.mass_storage)refreshMedia();else $('#media-list').textContent='虚拟介质尚未配置';
        refreshPasskeys();
        requestAnimationFrame(()=>{fitConsoleToVideo(true);viewport.focus({preventScroll:true});});
        if(videoWorkspace==='terminal'){terminalReset();terminalConnect();}
      }
      else { terminalDisconnect(); stopInput(); stopVideo(); if (state === 'login') updateLoginPasskeyUI(); requestAnimationFrame(()=>$(state==='setup'?'#setup-form h2':'#login-form h2')?.focus()); }
    }
    function consumeSetupToken() { const params=new URLSearchParams(location.hash.slice(1)),token=params.get('setup');if(!token)return;setupForm.elements.setup_token.value=token;$('#setup-token-field').classList.add('hidden');history.replaceState(null,'',`${location.pathname}${location.search}`); }
    async function start() {
      try { bootstrap = await request('/api/bootstrap'); const token=setupForm.elements.setup_token; token.required=bootstrap.token_required !== false; token.closest('.field').classList.toggle('hidden',bootstrap.setup_required && (bootstrap.token_required === false || !!token.value)); const state=bootstrap.setup_required ? 'setup' : bootstrap.authenticated ? 'main' : 'login';showState(state);if(state==='setup'&&!setupScanStarted){setupScanStarted=true;scanDevices($('#setup-device-results'));} }
      catch (error) { $('#boot-panel h2').textContent = '无法连接 WingmanKVM'; $('#boot-panel p').textContent = error.message; }
    }
    let setupStep = 0, setupScanStarted = false;
    function setSetupStep(next) { setupStep=Math.max(0,Math.min(1,next)); $$('.setup-step',setupForm).forEach((step,index)=>step.classList.toggle('active',index===setupStep)); $$('.progress-step',setupForm).forEach((step,index)=>{step.classList.toggle('active',index===setupStep);step.classList.toggle('done',index<setupStep);}); $('.auth-panel').scrollTop=0;if(setupStep===1){if(!setupScanStarted){setupScanStarted=true;scanDevices($('#setup-device-results'));}updateSetupCapabilities();} }
    function validateSetupAccount() { const username=value(setupForm,'username'),password=rawValue(setupForm,'password'),confirm=rawValue(setupForm,'confirm_password'),usernameInput=setupForm.elements.username,passwordInput=setupForm.elements.password,confirmInput=setupForm.elements.confirm_password,strong=[...password].length>=12&&/\p{Lu}/u.test(password)&&/\p{Ll}/u.test(password)&&/\p{N}/u.test(password)&&/[^\p{L}\p{N}\s]/u.test(password);usernameInput.setCustomValidity(/^[A-Za-z0-9._-]{3,64}$/.test(username)?'':'账号使用 3–64 位字母、数字、点、下划线或短横线'); passwordInput.setCustomValidity(strong?'':'密码至少 12 位，并包含大写字母、小写字母、数字和符号'); confirmInput.setCustomValidity(password===confirm?'':'两次输入的密码不一致'); const valid=[setupForm.elements.setup_token,usernameInput,passwordInput,confirmInput].every(input=>input.reportValidity()); return valid; }
    $$('[data-setup-next]',setupForm).forEach(button=>button.addEventListener('click',()=>{if(setupStep===0&&!validateSetupAccount())return;setSetupStep(setupStep+1);}));
    $$('[data-setup-prev]',setupForm).forEach(button=>button.addEventListener('click',()=>setSetupStep(setupStep-1)));
    setupForm.elements.password.addEventListener('input',()=>setupForm.elements.password.setCustomValidity(''));
    setupForm.elements.confirm_password.addEventListener('input',()=>setupForm.elements.confirm_password.setCustomValidity(''));
    setupForm.addEventListener('submit', async event => {
      event.preventDefault(); if(!validateSetupAccount()){setSetupStep(0);return;} const button = $('button[type=submit]', setupForm), error = $('#setup-error'), label=button.textContent; button.disabled = true; button.textContent='正在初始化…'; setupForm.setAttribute('aria-busy','true'); error.textContent = '';
      try { const payload = {username:value(setupForm,'username'), password:rawValue(setupForm,'password'), setup_token:rawValue(setupForm,'setup_token'), ...hardwareFrom(setupForm)}; await request('/api/setup',{method:'POST',body:JSON.stringify(payload)}); clearAuthSecrets({token:true}); toast('初始化完成，正在建立控制台'); bootstrap = await request('/api/bootstrap'); showState(bootstrap.authenticated?'main':'login'); }
      catch (e) { error.textContent = e.message;clearAuthSecrets({login:false});if(e.message.includes('令牌'))$('#setup-token-field').classList.remove('hidden'); } finally { button.disabled = false; button.textContent=label; setupForm.setAttribute('aria-busy','false'); }
    });
    loginForm.addEventListener('submit', async event => {
      event.preventDefault(); const button = $('button[type=submit]', loginForm), error = $('#login-error'), label=button.textContent; button.disabled = true; button.textContent='正在连接…'; loginForm.setAttribute('aria-busy','true'); error.textContent = '';
      try { await request('/api/login',{method:'POST',body:JSON.stringify({username:value(loginForm,'username'),password:rawValue(loginForm,'password')})}); clearAuthSecrets({token:true}); bootstrap = await request('/api/bootstrap'); showState(bootstrap.authenticated?'main':'login'); }
      catch (e) { error.textContent = e.message;clearAuthSecrets({setup:false}); } finally { button.disabled = false; button.textContent=label; loginForm.setAttribute('aria-busy','false'); }
    });
    $('#logout').addEventListener('click', async () => { clearAuthSecrets({token:true}); await stopInput(); try { await request('/api/logout',{method:'POST'}); } finally { bootstrap.authenticated=false; try { bootstrap = await request('/api/bootstrap'); } catch(_) {} showState('login'); } });

    function unwrapConfig(source) { return source?.config || source || {}; }
    const resolutionPresets = new Set(['3840x2160','2560x1440','1920x1080','1280x720','720x480']);
    const fpsPresets = new Set(['60','30','25','24','15']);
    const h264BitratePresets = new Set(['2000','4000','8000','12000']);
    const displayModes = [['fit','mode-fit','适应'],['native','mode-native','1:1'],['fill','mode-fill','填满']];
    let modeIndex = 0;
    function readVideoUiPreference(name, fallback) { try { return localStorage.getItem(`wingmankvm.video.${name}`) || fallback; } catch { return fallback; } }
    function saveVideoUiPreference(name, value) { try { localStorage.setItem(`wingmankvm.video.${name}`, value); } catch {} }
    videoPreference=readVideoUiPreference('transport','auto');
    if(!['auto','webrtc_h264','mjpeg'].includes(videoPreference))videoPreference='auto';
    function browserSupportsH264(){if(typeof RTCPeerConnection!=='function')return false;const codecs=globalThis.RTCRtpReceiver?.getCapabilities?.('video')?.codecs;return !codecs||codecs.some(codec=>String(codec.mimeType||'').toLowerCase()==='video/h264');}
    function h264Available(){return bootstrap.capabilities?.video_webrtc_h264===true&&browserSupportsH264();}
    function syncTransportControl(){const select=$('#video-config').elements.transport,option=select.querySelector('option[value="webrtc_h264"]');option.disabled=bootstrap.capabilities?.video_webrtc_h264===false;select.value=videoPreference;}
    function setVideoPreference(value){videoPreference=['auto','webrtc_h264','mjpeg'].includes(value)?value:'auto';saveVideoUiPreference('transport',videoPreference);syncTransportControl();restartVideo();}
    function setDisplayScale(mode, remember = false) {
      const nextIndex=Math.max(0,displayModes.findIndex(item=>item[0]===mode)), next=displayModes[nextIndex];
      displayModes.forEach(item=>consoleBox.classList.remove(item[1]));modeIndex=nextIndex;consoleBox.classList.add(next[1]);
      const modeText=$('#mode-button .mode-text');if(modeText)modeText.textContent=next[2];else $('#mode-button').textContent=next[2];$('#mode-button').title=`切换显示方式：${next[2]} (适应窗口 / 1:1 像素 / 填满窗口)`;$('#video-config').elements.display_scale.value=next[0];
      if(remember)saveVideoUiPreference('scale',next[0]);
      const panToggle = $('#pan-toggle');
      if(next[0]==='fit'){
        panToggle?.classList.add('hidden');
        setPanMode(false);
        fitConsoleToVideo();
      } else if(next[0]==='native') {
        panToggle?.classList.remove('hidden');
        consoleBox.style.width='';consoleBox.style.height='';
        if(!consoleUserMoved){consoleBox.style.left='';consoleBox.style.top='';}
        setTimeout(centerNativeViewport, 60);
      } else {
        panToggle?.classList.add('hidden');
        setPanMode(false);
        consoleBox.style.width='';consoleBox.style.height='';
        if(!consoleUserMoved){consoleBox.style.left='';consoleBox.style.top='';}
      }
      invalidateSurfaceBox();
    }
    $('#pan-toggle')?.addEventListener('click', () => setPanMode(!isPanActive));
    $('#pan-toggle')?.addEventListener('dblclick', () => {
      centerNativeViewport();
      toast(t('pan_mode', '平移') + ': 已居中');
    });
    function setVideoRendering(rendering, remember = false) {
      const next=rendering==='smooth'?'smooth':'pixelated';consoleBox.classList.toggle('render-pixelated',next==='pixelated');
      $('#video-config').elements.rendering.value=next;if(remember)saveVideoUiPreference('rendering',next);
    }
    function syncVideoPresets() {
      const form=$('#video-config'), resolution=value(form,'resolution_preset'), fps=value(form,'fps_preset');
      $$('.resolution-custom',form).forEach(field=>field.classList.toggle('hidden',resolution!=='custom'));
      $('.fps-custom',form).classList.toggle('hidden',fps!=='custom');
      if(resolutionPresets.has(resolution)){const [width,height]=resolution.split('x');form.elements.width.value=width;form.elements.height.value=height;}
      else if(resolution==='auto'){form.elements.width.value='';form.elements.height.value='';}
      if(fpsPresets.has(fps))form.elements.frames_per_second.value=fps;else if(fps==='auto')form.elements.frames_per_second.value='';
    }
    for(const name of ['resolution_preset','fps_preset'])$('#video-config').elements[name].addEventListener('change',syncVideoPresets);
    function syncH264Preset() {
      const form=$('#video-config'),preset=value(form,'h264_bitrate_preset');
      $('.h264-bitrate-custom',form).classList.toggle('hidden',preset!=='custom');
      if(h264BitratePresets.has(preset))form.elements.h264_bitrate_kbps.value=preset;
    }
    function selectH264Preset(h264) {
      const form=$('#video-config'),bitrate=Number(h264?.bitrate_kbps)||4000;
      form.elements.h264_bitrate_preset.value=h264BitratePresets.has(String(bitrate))?String(bitrate):'custom';
      form.elements.h264_bitrate_kbps.value=String(bitrate); syncH264Preset();
      form.elements.h264_encoder.value=h264?.encoder||'auto';
      form.elements.h264_max_sessions.value=String(h264?.max_sessions||1);
      form.elements.h264_allow_software.checked=!!h264?.allow_software;
    }
    function selectVideoPresets(video) {
      const form=$('#video-config'), resolution=video.width&&video.height?`${video.width}x${video.height}`:'auto', fps=video.frames_per_second==null?'auto':String(video.frames_per_second);
      form.elements.resolution_preset.value=resolutionPresets.has(resolution)?resolution:(resolution==='auto'?'auto':'custom');
      form.elements.fps_preset.value=fpsPresets.has(fps)?fps:(fps==='auto'?'auto':'custom');
      syncVideoPresets();
    }
    function renderClientInfo(client){
      if(!client)return;
      const ip=client.ip||client;if(!ip)return;
      const port=client.port?`:${client.port}`:'';
      const isProxy=client.proxy_protocol&&client.proxy_protocol!=='disabled';
      const proxySuffix=isProxy?' (PPv2)':'';
      const diagEl=$('#diag-client-ip');
      if(diagEl)diagEl.textContent=`${ip}${port}${proxySuffix}`;
      const sessionIpEl=$('#session-ip');
      if(sessionIpEl)sessionIpEl.textContent=`(${ip}${proxySuffix})`;
    }
    function applyBootstrap() {
      const config = unwrapConfig(bootstrap), video = config.video || {}, h264 = video.h264 || {}, hid = config.hid || {}, power = config.power || {}, media = config.media || {};
      const vf = $('#video-config'), df = $('#device-config'), mf = $('#media-config');
      for (const name of ['device','width','height','frames_per_second','encoding','jpeg_quality']) if (video[name] != null && vf.elements[name]) vf.elements[name].value = video[name];
      selectVideoPresets(video);
      selectH264Preset(h264);
      for (const name of ['keyboard_device','mouse_device','absolute_pointer_device']) df.elements[name].value = hid[name] || '';
      df.elements.pointer_mode.value = bootstrap.capabilities?.pointer_mode==='relative'?'relative':'absolute';
      df.elements.power_enabled.checked = !!power.enabled; df.elements.gpio_chip.value = power.gpio_chip ?? ''; df.elements.gpio_line.value = power.gpio_line ?? ''; df.elements.active_high.value = power.active_high === false ? 'false' : 'true';
      const reset=power.reset_switch||{},led=power.power_led||{}; df.elements.reset_gpio_chip.value=reset.gpio_chip??''; df.elements.reset_gpio_line.value=reset.gpio_line??''; df.elements.reset_active_high.value=reset.active_high===false?'false':'true'; df.elements.reset_pulse_ms.value=reset.pulse_ms??''; df.elements.power_led_gpio_chip.value=led.gpio_chip??''; df.elements.power_led_gpio_line.value=led.gpio_line??''; df.elements.power_led_active_low.value=led.active_low===false?'false':'true'; df.elements.power_led_bias.value=led.bias||'pull_up'; df.elements.power_led_poll_interval_ms.value=led.poll_interval_ms??''; df.elements.power_led_debounce_ms.value=led.debounce_ms??'';
      mf.elements.enabled.checked = !!media.enabled; if (media.lun_path != null) mf.elements.lun_path.value = media.lun_path; if (media.lun_file != null && !media.lun_path) mf.elements.lun_path.value = media.lun_file; if (media.image_directory != null) mf.elements.image_directory.value = media.image_directory;
      $('#quality-value').textContent = vf.elements.jpeg_quality.value; $('#session-user').textContent = bootstrap.username || bootstrap.user?.username || '管理员';if(bootstrap.client_ip)renderClientInfo({ip:bootstrap.client_ip,proxy_protocol:unwrapConfig(bootstrap).server?.proxy_protocol});if(bootstrap.capabilities?.pointer_mode==='absolute'&&relativeCaptured())document.exitPointerLock();syncInputUi();syncTransportControl(); syncGpioControls(); renderPowerLed(latestPowerStatus);
    }
    function syncGpioControls() {
      const caps=bootstrap.capabilities||{};
      $$('[data-power-action="press"],[data-power-action="force-off"]').forEach(b=>b.disabled=caps.gpio_power!==true);
      $$('[data-power-action="reset"]').forEach(b=>b.disabled=caps.gpio_reset!==true);
      $$('[data-gpio-test="power"]').forEach(button=>button.disabled=gpioTestBusy||caps.gpio_power!==true);
      $$('[data-gpio-test="reset"]').forEach(button=>button.disabled=gpioTestBusy||caps.gpio_reset!==true);
      $$('[data-power-led-refresh]').forEach(button=>button.disabled=caps.gpio_power_led!==true);
    }
    function renderPowerLed(powerStatus) {
      const led=powerStatus?.power_led||{},dots=$$('[data-power-led-dot]'),states=$$('[data-power-led-state]');
      if(!dots.length||!states.length)return;
      dots.forEach(dot=>dot.classList.remove('online','warning','error'));
      const show=(label,tone='')=>{states.forEach(state=>state.textContent=label);if(tone)dots.forEach(dot=>dot.classList.add(tone));};
      if(led.configured===false){show('未配置');updateDiagnosticsCard();return;}
      if(led.sense_error){show('读取失败','error');updateDiagnosticsCard();return;}
      const ledState=led.state||(led.active===true?'on':led.active===false?'off':'unknown');
      if(ledState==='on'){show('亮','online');updateDiagnosticsCard();return;}
      if(ledState==='off'){show('灭');updateDiagnosticsCard();return;}
      show('未知','warning');updateDiagnosticsCard();
    }
    const powerMenuToggle=$('#power-menu-toggle'),powerMenu=$('#power-menu');
    powerMenuToggle?.addEventListener('click',event=>{
      event.stopPropagation();
      const open=!powerMenu.hasAttribute('hidden');
      if(open){powerMenu.setAttribute('hidden','');powerMenuToggle.setAttribute('aria-expanded','false');}
      else{powerMenu.removeAttribute('hidden');powerMenuToggle.setAttribute('aria-expanded','true');$('#diagnostics-popover')?.setAttribute('hidden','');$('#diagnostics-toggle')?.setAttribute('aria-expanded','false');$('#lang-menu')?.setAttribute('hidden','');$('#lang-toggle')?.setAttribute('aria-expanded','false');}
    });
    const langToggle=$('#lang-toggle'),langMenu=$('#lang-menu');
    langToggle?.addEventListener('click',event=>{
      event.stopPropagation();
      const open=!langMenu.hasAttribute('hidden');
      if(open){langMenu.setAttribute('hidden','');langToggle.setAttribute('aria-expanded','false');}
      else{langMenu.removeAttribute('hidden');langToggle.setAttribute('aria-expanded','true');powerMenu?.setAttribute('hidden','');powerMenuToggle?.setAttribute('aria-expanded','false');$('#diagnostics-popover')?.setAttribute('hidden','');$('#diagnostics-toggle')?.setAttribute('aria-expanded','false');}
    });
    $$('[data-lang]').forEach(btn=>btn.addEventListener('click',()=>{setLanguage(btn.dataset.lang);langMenu?.setAttribute('hidden','');langToggle?.setAttribute('aria-expanded','false');}));
    $('#theme-toggle')?.addEventListener('click',toggleTheme);
    const diagToggle=$('#diagnostics-toggle'),diagPopover=$('#diagnostics-popover');
    function updateDiagnosticsCard(){
      if(!diagPopover||diagPopover.hasAttribute('hidden'))return;
      $('#diag-transport').textContent=activeVideoTransport==='webrtc_h264'?'WebRTC (H.264)':activeVideoTransport==='mjpeg'?'MJPEG':'未连接';
      $('#diag-h264-failure-card').classList.toggle('hidden',!videoFallbackReason);$('#diag-h264-failure').textContent=videoFallbackReason||'--';
      const w=latestVideoStatus?.width,h=latestVideoStatus?.height,fps=latestVideoStatus?.frames_per_second;
      $('#diag-resolution').textContent=w&&h?`${w} × ${h}${fps?` @ ${Math.round(fps)} FPS`:''}`:'--';
      $('#diag-input-mode').textContent=!inputEnabled()?'已暂停':absoluteMode()?'绝对指针 (USB Tablet)':relativeCaptured()?'相对鼠标 (已捕获)':'相对鼠标 (未捕获)';
      const led=latestPowerStatus?.power_led;
      const ledOn=led?.state==='on'||led?.active===true;
      const ledOff=led?.state==='off'||led?.active===false;
      $('#diag-power-led').textContent=ledOn?'已开机 (LED 亮)':ledOff?'已关机 (LED 灭)':led?.configured===false?'未配置':'未知';
      if(latestSystemStatus)renderSystemMetrics(latestSystemStatus);
    }
    diagToggle?.addEventListener('click',event=>{
      event.stopPropagation();
      const open=!diagPopover.hasAttribute('hidden');
      if(open){diagPopover.setAttribute('hidden','');diagToggle.setAttribute('aria-expanded','false');}
      else{diagPopover.removeAttribute('hidden');diagToggle.setAttribute('aria-expanded','true');powerMenu?.setAttribute('hidden','');powerMenuToggle?.setAttribute('aria-expanded','false');langMenu?.setAttribute('hidden','');langToggle?.setAttribute('aria-expanded','false');updateDiagnosticsCard();}
    });
    $('#diagnostics-close')?.addEventListener('click',()=>{diagPopover?.setAttribute('hidden','');diagToggle?.setAttribute('aria-expanded','false');});
    $('#diagnostics-reconnect')?.addEventListener('click',()=>{diagPopover?.setAttribute('hidden','');diagToggle?.setAttribute('aria-expanded','false');restartVideo();toast('已重新连接视频');});
    $('#diagnostics-scan')?.addEventListener('click',()=>{diagPopover?.setAttribute('hidden','');diagToggle?.setAttribute('aria-expanded','false');scanDevices();toast('已触发设备扫描');});
    document.addEventListener('click',event=>{
      if(!event.target.closest('.power-menu-container')){powerMenu?.setAttribute('hidden','');powerMenuToggle?.setAttribute('aria-expanded','false');}
      if(!event.target.closest('.diagnostics-popover')&&!event.target.closest('#diagnostics-toggle')){diagPopover?.setAttribute('hidden','');diagToggle?.setAttribute('aria-expanded','false');}
      if(!event.target.closest('.lang-menu-container')){langMenu?.setAttribute('hidden','');langToggle?.setAttribute('aria-expanded','false');}
      if(!event.target.closest('#topbar-perf-btn')&&!event.target.closest('#topbar-perf-popover')){closeTopbarPerf();}
    });
    async function executePowerAction(action) {
      const caps=bootstrap.capabilities||{};
      if(action==='reset'&&caps.gpio_reset!==true){toast('复位引脚未配置',true);return;}
      if((action==='press'||action==='force-off')&&caps.gpio_power!==true){toast('电源引脚未配置',true);return;}
      let url='/power',duration='0.5',label='电源';
      if(action==='reset'){if(!confirm('确定复位吗？'))return;url='/reset';duration='0.5';label='复位';}
      else if(action==='force-off'){if(!confirm('确定长按电源 5 秒吗？这可能强制关机。'))return;url='/power';duration='5';label='强制关机';}
      else{label='短按电源';}
      toast(`${label}操作已发送…`);
      try{
        const response=await fetch(url,{method:'POST',body:new URLSearchParams({duration}),credentials:'same-origin'});
        const type=response.headers.get('content-type')||'';const data=type.includes('json')?await response.json():await response.text();
        if(!response.ok)throw new Error((data&&data.error)||data||`${label}操作失败`);
        toast(`${label}操作已执行`);setTimeout(refreshStatus,1000);
      }catch(error){toast(error.message||`${label}操作失败`,true);}
    }
    $$('[data-power-action]').forEach(button=>button.addEventListener('click',()=>{
      powerMenu?.setAttribute('hidden','');powerMenuToggle?.setAttribute('aria-expanded','false');
      executePowerAction(button.dataset.powerAction);
    }));
    async function runGpioTest(target,button){
      if(gpioTestBusy||button.disabled)return;
      gpioTestBusy=true;const label=button.textContent;button.disabled=true;button.textContent='…';syncGpioControls();
      try { await request('/api/gpio/test',{method:'POST',body:JSON.stringify({target})});toast(`${target==='reset'?'RESET':'POWER'} 已执行`);await refreshStatus(); }
      catch(error){toast(error.message||'GPIO 操作失败',true);}
      finally {button.textContent=label;gpioTestBusy=false;syncGpioControls();}
    }
    $$('[data-gpio-test]').forEach(button=>button.addEventListener('click',()=>runGpioTest(button.dataset.gpioTest,button)));
    $$('[data-power-led-refresh]').forEach(button=>button.addEventListener('click',async event=>{const current=event.currentTarget;if(current.disabled)return;const label=current.textContent;current.disabled=true;current.textContent='…';try{await refreshStatus();}finally{current.textContent=label;syncGpioControls();}}));
    $('#video-config').addEventListener('submit', async event => {
      event.preventDefault(); const f=event.currentTarget,button=$('button[type=submit]',f);if(button.disabled)return;syncVideoPresets();const selectedTransport=value(f,'transport'),requestId=++videoConfigRequestId,label=button.textContent,payload={video:{device:value(f,'device')||null,width:optionalNumber(value(f,'width')),height:optionalNumber(value(f,'height')),frames_per_second:optionalNumber(value(f,'frames_per_second')),encoding:value(f,'encoding'),jpeg_quality:Number(value(f,'jpeg_quality')),h264:{bitrate_kbps:Number(value(f,'h264_bitrate_kbps')),encoder:value(f,'h264_encoder'),allow_software:f.elements.h264_allow_software.checked,max_sessions:Number(value(f,'h264_max_sessions'))}}};
      button.disabled=true;button.textContent='应用中…';stopVideo();
      try { await request('/api/config',{method:'POST',body:JSON.stringify(payload)});if(requestId!==videoConfigRequestId)return;videoPreference=['auto','webrtc_h264','mjpeg'].includes(selectedTransport)?selectedTransport:'auto';saveVideoUiPreference('transport',videoPreference);bootstrap=await request('/api/bootstrap');applyBootstrap();toast('视频设置已应用'); }
      catch(e){if(requestId===videoConfigRequestId)toast(e.message,true);}
      finally {if(requestId===videoConfigRequestId){button.disabled=false;button.textContent=label;if(shouldVideoRun())startVideo(true);}}
    });
    $$('[data-video-preset]').forEach(chip=>chip.addEventListener('click',()=>{
      $$('.preset-chip').forEach(c=>c.classList.toggle('active',c===chip));
      const preset=chip.dataset.videoPreset,form=$('#video-config');
      if(preset==='desktop'){
        form.elements.transport.value='auto';setVideoPreference('auto');
        form.elements.resolution_preset.value='1920x1080';
        form.elements.fps_preset.value='60';
        form.elements.h264_bitrate_preset.value='8000';
        setDisplayScale('fit',true);setVideoRendering('smooth',true);
        toast(t('preset_desktop_applied','已加载桌面优化预设 (1080p 60fps 8M)'));
      }else if(preset==='bios'){
        form.elements.transport.value='mjpeg';setVideoPreference('mjpeg');
        form.elements.resolution_preset.value='1280x720';
        form.elements.fps_preset.value='30';
        form.elements.h264_bitrate_preset.value='4000';
        setDisplayScale('native',true);setVideoRendering('pixelated',true);
        toast(t('preset_bios_applied','已加载 BIOS/UEFI 预设 (720p 30fps MJPEG 像素锐利)'));
      }else if(preset==='bandwidth'){
        form.elements.transport.value='auto';setVideoPreference('auto');
        form.elements.resolution_preset.value='1280x720';
        form.elements.fps_preset.value='24';
        form.elements.h264_bitrate_preset.value='2000';
        setDisplayScale('fit',true);setVideoRendering('smooth',true);
        toast(t('preset_bandwidth_applied','已加载低带宽省流预设 (720p 24fps 2M)'));
      }
      syncVideoPresets();syncH264Preset();
    }));
    function gpioPulseFrom(form){const chip=value(form,'reset_gpio_chip'),rawLine=value(form,'reset_gpio_line');if(!chip&&!rawLine)return null;if(!chip||!rawLine)throw new Error('RESET SW 需要芯片和线路');return {gpio_chip:chip,gpio_line:Number(rawLine),active_high:value(form,'reset_active_high')!=='false',pulse_ms:optionalNumber(value(form,'reset_pulse_ms'))??500};}
    function gpioInputFrom(form){const chip=value(form,'power_led_gpio_chip'),rawLine=value(form,'power_led_gpio_line');if(!chip&&!rawLine)return null;if(!chip||!rawLine)throw new Error('PWR LED 需要芯片和线路');return {gpio_chip:chip,gpio_line:Number(rawLine),active_low:value(form,'power_led_active_low')!=='false',bias:value(form,'power_led_bias')||'pull_up',poll_interval_ms:optionalNumber(value(form,'power_led_poll_interval_ms'))??1000,debounce_ms:optionalNumber(value(form,'power_led_debounce_ms'))??50};}
    $('#device-config').addEventListener('submit', async event => {
      event.preventDefault(); const f=event.currentTarget;
      try { const payload={hid:{keyboard_device:value(f,'keyboard_device')||null,mouse_device:value(f,'mouse_device')||null,absolute_pointer_device:value(f,'absolute_pointer_device')||null,pointer_mode:value(f,'pointer_mode')},power:{enabled:f.elements.power_enabled.checked,gpio_chip:value(f,'gpio_chip')||null,gpio_line:optionalNumber(value(f,'gpio_line')),active_high:value(f,'active_high')!=='false',reset_switch:gpioPulseFrom(f),power_led:gpioInputFrom(f)}}; await request('/api/config',{method:'POST',body:JSON.stringify(payload)}); bootstrap=await request('/api/bootstrap');applyBootstrap();toast('已保存'); } catch(e){toast(e.message,true);}
    });
    $('#media-config').addEventListener('submit', async event => {
      event.preventDefault(); const f=event.currentTarget, payload={media:{enabled:f.elements.enabled.checked,lun_path:value(f,'lun_path')||null,image_directory:value(f,'image_directory')||null}};
      try { await request('/api/config',{method:'POST',body:JSON.stringify(payload)}); toast('虚拟介质设置已保存'); await refreshMedia(); } catch(e){toast(e.message,true);}
    });
    async function scanDevices(target = $('#device-results')) {
      const setupTarget=target.id==='setup-device-results',setupVisible=!setupForm.classList.contains('hidden');
      if(setupTarget)renderSetupDetection(null,true);else renderDeviceDetection(null,true);
      try { const data=await request(setupVisible?'/api/setup/devices':'/api/devices/scan',{method:'POST'});fillDetected(data);if(setupTarget)renderSetupDetection(data);else renderDeviceDetection(data);const diagnostics=$('#device-diagnostics');if(diagnostics)diagnostics.textContent=JSON.stringify(data,null,2);return data; }
      catch(e){if(setupTarget)renderSetupDetection({error:e.message});else renderDeviceDetection({error:e.message});toast(e.message,true);}
    }
    function firstPath(v) { if (typeof v === 'string') return v; if (Array.isArray(v)) return firstPath(v[0]); return v?.path || v?.device || ''; }
    function firstLabel(v) { if(Array.isArray(v))return firstLabel(v[0]);return v?.label||firstPath(v); }
    function asCandidates(v){return (Array.isArray(v)?v:v?[v]:[]).filter(Boolean);}
    function candidateReady(candidate,kind='gadget'){if(!firstPath(candidate))return false;if(typeof candidate==='string')return kind!=='video';if(kind==='video')return candidate.video_capture===true&&candidate.supports_mjpeg===true;return candidate.compatible===true&&candidate.gadget_bound===true&&candidate.function_linked===true;}
    function readyCandidates(v,kind){return asCandidates(v).filter(candidate=>candidateReady(candidate,kind));}
    function candidateGroups(root){return {video:readyCandidates(root.video||root.video_devices,'video'),keyboard:readyCandidates(root.keyboard||root.keyboards),mouse:readyCandidates(root.mouse||root.mice),absolute:readyCandidates(root.absolute_pointer||root.absolute_pointers),luns:readyCandidates(root.mass_storage_luns),gpio:asCandidates(root.gpio_chip||root.gpio).filter(candidate=>!!firstPath(candidate))};}
    function uniquePath(candidates){return candidates.length===1?firstPath(candidates[0]):'';}
    function selectedPath(field,candidates){const selected=value(setupForm,field);return candidates.some(candidate=>firstPath(candidate)===selected)?selected:'';}
    function setIfEmpty(input,next){if(input&&next!=null&&next!==''&&!input.value)input.value=next;}
    function setAutoValue(input,next){if(!input||input.dataset.userEdited)return;input.value=next||'';input.dataset.autoFilled=next?'true':'';}
    let lastSetupDiscovery=null;
    function fillDetected(data) {
      const root=data?.devices||data||{}, setup=!setupForm.classList.contains('hidden')?setupForm:null, device=$('#device-config'), media=$('#media-config'),groups=candidateGroups(root);
      const detected={video:uniquePath(groups.video),keyboard:uniquePath(groups.keyboard),mouse:uniquePath(groups.mouse),absolute:uniquePath(groups.absolute),lun:uniquePath(groups.luns),gpio:uniquePath(groups.gpio)};
      if(setup){lastSetupDiscovery=root;setAutoValue(setup.elements.video_device,detected.video);setAutoValue(setup.elements.keyboard_device,detected.keyboard);setAutoValue(setup.elements.mouse_device,detected.mouse);setAutoValue(setup.elements.absolute_pointer_device,detected.absolute);setAutoValue(setup.elements.gpio_chip,detected.gpio);setAutoValue(setup.elements.lun_path,detected.lun);setAutoValue(setup.elements.image_directory,'/var/lib/wingmankvm/images');if(!setup.elements.pointer_mode.dataset.userEdited)setup.elements.pointer_mode.value=detected.absolute?'absolute':detected.mouse?'relative':'absolute';if(!setup.elements.media_enabled.dataset.userEdited)setup.elements.media_enabled.checked=!!detected.lun;populateGpioOptions(groups.gpio);}
      setIfEmpty(device.elements.keyboard_device,detected.keyboard);setIfEmpty(device.elements.mouse_device,detected.mouse);setIfEmpty(device.elements.absolute_pointer_device,detected.absolute);setIfEmpty(media.elements.lun_path,detected.lun);setIfEmpty(media.elements.image_directory,'/var/lib/wingmankvm/images');updateSetupCapabilities();
    }
    function populateGpioOptions(candidates){const list=$('#setup-gpio-options');list.replaceChildren();for(const candidate of candidates){const option=document.createElement('option');option.value=firstPath(candidate);option.label=firstLabel(candidate);list.append(option);}}
    function candidateSelect(label,field,candidates){const select=document.createElement('select'),placeholder=document.createElement('option');placeholder.value='';placeholder.textContent=`${label} · 请选择`;select.append(placeholder);for(const candidate of candidates){const option=document.createElement('option');option.value=firstPath(candidate);option.textContent=firstLabel(candidate);select.append(option);}select.value=value(setupForm,field);select.addEventListener('change',()=>{const input=setupForm.elements[field];input.value=select.value;input.dataset.userEdited='true';renderSetupDetection(lastSetupDiscovery);updateSetupCapabilities();});return select;}
    function detectionCard(label,state,detail,selections=[],guide=''){const card=document.createElement('div');card.className='capability';const dot=document.createElement('span');dot.className=`status-dot${state==='online'?' online':state==='warning'?' warning':''}`;const heading=document.createElement('span');heading.className='capability-heading';const title=document.createElement('strong');title.textContent=label;heading.append(title);if(guide){const help=document.createElement('button');help.type='button';help.className='guide-help';help.dataset.guide=guide;help.setAttribute('aria-label',`${label}说明`);help.setAttribute('aria-controls','auth-guide');help.setAttribute('aria-expanded','false');help.textContent='?';heading.append(help);}const copy=document.createElement('p');copy.textContent=detail;copy.title=detail;card.append(dot,heading,copy);for(const selection of selections){if(selection.candidates.length>1)card.append(candidateSelect(selection.label,selection.field,selection.candidates));}return card;}
    function renderDeviceDetection(data,loading=false){const box=$('#device-results');box.replaceChildren();if(loading){box.append(detectionCard('正在检测','warning','请稍候'));return;}if(data?.error){box.append(detectionCard('检测失败','',data.error));return;}const root=data?.devices||data||{},groups=candidateGroups(root),caps=bootstrap.capabilities||{},videoFound=groups.video.length>0,videoLabel=groups.video.length===1?firstLabel(groups.video[0]):`${groups.video.length} 个可用设备`,inputFound=groups.keyboard.length>0&&(groups.mouse.length>0||groups.absolute.length>0),mediaFound=groups.luns.length>0,powerFound=groups.gpio.length>0;box.append(detectionCard('视频',caps.video?(videoFound?'online':'warning'):videoFound?'warning':'',caps.video?(videoFound?videoLabel:'已配置，当前未检测到'):videoFound?'可选择':'未配置'),detectionCard('键盘与鼠标',caps.keyboard&&caps.mouse?(inputFound?'online':'warning'):inputFound?'warning':'',caps.keyboard&&caps.mouse?(inputFound?(caps.pointer_mode==='absolute'?'绝对指针':'相对鼠标'):'已配置，当前未检测到'):inputFound?'可选择':'未配置'),detectionCard('虚拟介质',caps.mass_storage?(mediaFound?'online':'warning'):mediaFound?'warning':'',caps.mass_storage?(mediaFound?'已就绪':'已配置，当前未检测到'):mediaFound?'可启用':'未配置'),detectionCard('电源',caps.gpio_power?(powerFound?'online':'warning'):powerFound?'warning':'',caps.gpio_power?(powerFound?'已配置':'已配置，当前未检测到'):powerFound?'线路待确认':'未配置'));}
    function renderSetupDetection(data,loading=false){const box=$('#setup-device-results');box.replaceChildren();if(loading){box.append(detectionCard('正在检测','warning','请稍候'));return;}if(data?.error){box.append(detectionCard('检测失败','',data.error));return;}const root=data?.devices||data||lastSetupDiscovery||{},groups=candidateGroups(root);lastSetupDiscovery=root;const video=selectedPath('video_device',groups.video),keyboard=selectedPath('keyboard_device',groups.keyboard),mouse=selectedPath('mouse_device',groups.mouse),absolute=selectedPath('absolute_pointer_device',groups.absolute),pointerMode=value(setupForm,'pointer_mode')==='relative'?'relative':'absolute',pointer=pointerMode==='relative'?mouse:absolute,lun=selectedPath('lun_path',groups.luns),gpio=value(setupForm,'gpio_chip');const videoState=video?'online':groups.video.length?'warning':'';const inputState=keyboard&&pointer?'online':(groups.keyboard.length&&(groups.mouse.length||groups.absolute.length))?'warning':'';const mediaState=lun?'online':groups.luns.length?'warning':'';const gpioKnown=groups.gpio.some(candidate=>firstPath(candidate)===gpio);box.append(detectionCard('视频',videoState,video?firstLabel(groups.video.find(item=>firstPath(item)===video)):groups.video.length?'请选择采集卡':'未发现可用采集卡',[{label:'视频',field:'video_device',candidates:groups.video}],'video'),detectionCard('USB 控制',inputState,inputState==='online'?(pointerMode==='absolute'?'绝对指针已就绪':'相对鼠标已就绪'):groups.keyboard.length&&(groups.mouse.length||groups.absolute.length)?'请选择与指针模式匹配的设备':'未发现完整键鼠',[{label:'键盘',field:'keyboard_device',candidates:groups.keyboard},{label:'相对鼠标',field:'mouse_device',candidates:groups.mouse},{label:'绝对指针',field:'absolute_pointer_device',candidates:groups.absolute}],'input'),detectionCard('虚拟介质',mediaState,lun?'已就绪':groups.luns.length?'请选择接口':'未配置',[{label:'虚拟介质',field:'lun_path',candidates:groups.luns}],'media'),detectionCard('电源',gpioKnown?'warning':'',gpioKnown?'检测到控制器，仍需选择线路':groups.gpio.length?'请选择控制器':'稍后配置',[{label:'GPIO',field:'gpio_chip',candidates:groups.gpio}],'power'));}
    function updateSetupCapabilities(){const groups=candidateGroups(lastSetupDiscovery||{}),video=selectedPath('video_device',groups.video),keyboard=selectedPath('keyboard_device',groups.keyboard),pointerMode=value(setupForm,'pointer_mode')==='relative'?'relative':'absolute',pointer=pointerMode==='relative'?selectedPath('mouse_device',groups.mouse):selectedPath('absolute_pointer_device',groups.absolute),lun=selectedPath('lun_path',groups.luns),gpio=value(setupForm,'gpio_chip'),line=value(setupForm,'gpio_line'),powerReady=!!gpio&&line!==''&&groups.gpio.some(candidate=>firstPath(candidate)===gpio),mediaReady=!!lun;const powerToggle=setupForm.elements.power_enabled,mediaToggle=setupForm.elements.media_enabled;if(!powerReady)powerToggle.checked=false;if(!mediaReady)mediaToggle.checked=false;powerToggle.disabled=!powerReady;mediaToggle.disabled=!mediaReady;const missing=[];if(!video)missing.push('视频');if(!keyboard||!pointer)missing.push('键鼠');const readiness=$('#setup-readiness'),submit=$('#setup-submit');readiness.textContent=missing.length?`${missing.join('、')}可稍后配置`:'推荐配置已就绪';submit.textContent=missing.length?'仍然进入控制台':'进入控制台';}
    ['video_device','keyboard_device','mouse_device','absolute_pointer_device','pointer_mode','gpio_chip','gpio_line','lun_path','image_directory'].forEach(name=>setupForm.elements[name].addEventListener('input',event=>{if(event.isTrusted)event.target.dataset.userEdited='true';if(name==='pointer_mode')renderSetupDetection(lastSetupDiscovery);if(name==='pointer_mode'||name==='gpio_chip'||name==='gpio_line')updateSetupCapabilities();}));
    setupForm.elements.media_enabled.addEventListener('change',event=>{event.target.dataset.userEdited='true';updateSetupCapabilities();});
    setupForm.elements.power_enabled.addEventListener('change',event=>{event.target.dataset.userEdited='true';updateSetupCapabilities();});
    $('#scan-devices').addEventListener('click',()=>scanDevices());
    $('#setup-scan').addEventListener('click',()=>scanDevices($('#setup-device-results')));
    $('#media-scan').addEventListener('click',async()=>{const button=$('#media-scan'),label=button.textContent;button.disabled=true;button.textContent='检测中…';try{const data=await scanDevices(),luns=readyCandidates((data?.devices||data||{}).mass_storage_luns);toast(value($('#media-config'),'lun_path')?'已找到虚拟介质':luns.length>1?'发现多个接口，请选择路径':'未发现可用 LUN',!value($('#media-config'),'lun_path'));}finally{button.disabled=false;button.textContent=label;}});

    function shouldVideoRun(){return !app.classList.contains('hidden')&&!document.hidden&&videoWorkspace==='video';}
    function showVideoSurface(kind){feed.classList.toggle('hidden',kind!=='mjpeg');webrtcFeed.classList.toggle('hidden',kind!=='webrtc_h264');invalidateSurfaceBox();}
    function setVideoMessage(message,visible=true){const box=$('#video-message');box.textContent=message;box.classList.toggle('hidden',!visible);}
    function videoFrameAgeMs(){const rawAge=latestVideoStatus?.last_frame_age_ms;if(rawAge!==null&&rawAge!==undefined&&rawAge!==''){const age=Number(rawAge);if(Number.isFinite(age)&&age>=0)return age;}const rawUnix=latestVideoStatus?.last_frame_unix_ms;if(rawUnix!==null&&rawUnix!==undefined&&rawUnix!==''){const unix=Number(rawUnix);if(Number.isFinite(unix)&&unix>0)return Math.max(0,Date.now()-unix);}return null;}
    function videoFrameAgeText(){const age=videoFrameAgeMs();if(age===null)return '';if(age<1000)return '最近有效帧：刚刚';if(age<60000)return `最近有效帧：${Math.round(age/1000)} 秒前`;return `最近有效帧：${Math.round(age/60000)} 分钟前`;}
    function renderVideoUi(){
      const dot=$('#status-dot'),label=$('#status-label'),meta=$('#video-transport-label'),mode=activeVideoTransport==='webrtc_h264'?'H.264':activeVideoTransport==='mjpeg'?'MJPEG':'视频';
      const serverState=latestVideoStatus?.state,serverReady=serverState==='ready',serverStarting=serverState==='starting',serverStalled=serverState==='stalled',serverUnavailable=serverState&&!serverReady&&!serverStarting,ageText=videoFrameAgeText();
      dot.classList.toggle('online',videoUiState==='playing'&&serverReady);dot.classList.toggle('warning',videoUiState==='connecting'||videoUiState==='fallback'||serverStarting||serverUnavailable);
      viewport.classList.toggle('video-unhealthy',!!serverState&&!serverReady);
      if(!shouldVideoRun()){label.textContent='视频已暂停';meta.textContent='暂停';return;}
      if(serverUnavailable){
        showVideoSurface('none');
        const reason=latestVideoStatus?.message||(serverStalled?'视频已停止更新':'设备离线');
        setVideoMessage(ageText?`${reason} · ${ageText}`:reason);
        label.textContent=serverStalled?'视频已停止更新':reason;
        meta.textContent=serverStalled?'已停止更新':mode;
        return;
      }
      if(serverStarting){
        showVideoSurface('none');
        setVideoMessage(ageText?`正在等待首帧… · ${ageText}`:'正在等待首帧…');
        label.textContent=`正在等待 ${mode} 首帧…`;
        meta.textContent=mode;
        return;
      }
      if(videoUiState==='playing'&&serverReady){const fps=latestVideoStatus?.frames_per_second;showVideoSurface(activeVideoTransport==='webrtc_h264'?'webrtc_h264':'mjpeg');label.textContent=videoFallback?'MJPEG · H.264 回退':`${mode}${fps?` · ${Math.round(fps)} FPS`:''}`;meta.textContent=`LIVE / ${mode}`;setVideoMessage('',false);return;}
      if(videoUiState==='fallback'){label.textContent='MJPEG · H.264 回退';meta.textContent='MJPEG';return;}
      label.textContent=videoUiState==='connecting'?`正在连接 ${mode}…`:'视频连接中';meta.textContent=mode;updateDiagnosticsCard();
    }
    function setVideoState(state,transport=activeVideoTransport){activeVideoTransport=transport;videoUiState=state;renderVideoUi();updateDiagnosticsCard();}
    function clearVideoTimers(){clearTimeout(reconnectTimer);clearTimeout(videoFirstFrameTimer);reconnectTimer=0;videoFirstFrameTimer=0;}
    function closeWebrtcSession(sessionId){if(!sessionId)return;fetch('/api/webrtc/close',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({session_id:sessionId}),credentials:'same-origin',keepalive:true}).catch(()=>{});}
    function cleanupWebrtc(){const peer=webrtcPeer,sessionId=webrtcSessionId,abort=webrtcAbort;webrtcPeer=null;webrtcSessionId=null;webrtcAbort=null;abort?.abort();if(peer){peer.ontrack=null;peer.onconnectionstatechange=null;peer.oniceconnectionstatechange=null;peer.getReceivers().forEach(receiver=>receiver.track?.stop());peer.close();}if(sessionId)closeWebrtcSession(sessionId);webrtcFeed.onplaying=null;webrtcFeed.onloadedmetadata=null;webrtcFeed.pause();webrtcFeed.srcObject=null;}
    function stopVideo(showPaused=true){videoGeneration++;clearVideoTimers();reconnectDelay=500;cleanupWebrtc();feed.onload=null;feed.onerror=null;feed.removeAttribute('src');showVideoSurface('none');activeVideoTransport='none';videoFallback=false;videoFallbackReason='';videoUiState='paused';if(showPaused)renderVideoUi();updateDiagnosticsCard();}
    function restartVideo(){stopVideo(false);if(shouldVideoRun())startVideo();}
    function markVideoPlaying(generation,transport){if(generation!==videoGeneration||!shouldVideoRun()||activeVideoTransport!==transport)return;clearTimeout(videoFirstFrameTimer);setVideoState('playing',transport);if(latestVideoStatus?.state==='ready')setVideoMessage('',false);}
    function startMjpeg(generation,fallback=false){
      if(generation!==videoGeneration||!shouldVideoRun())return;
      activeVideoTransport='mjpeg';videoFallback=fallback||videoFallback;setVideoState(videoFallback?'fallback':'connecting','mjpeg');setVideoMessage(videoFallback?'H.264 不可用，切换 MJPEG…':'正在连接 MJPEG…');showVideoSurface('mjpeg');feed.onload=null;feed.onerror=null;feed.removeAttribute('src');
      const url=`/video_feed?t=${Date.now()}`;
      const onLoad=()=>{if(generation!==videoGeneration||!shouldVideoRun()||feed.getAttribute('src')!==url)return;reconnectDelay=500;markVideoPlaying(generation,'mjpeg');fitConsoleToVideo();};
      const onError=()=>{if(generation!==videoGeneration||!shouldVideoRun()||feed.getAttribute('src')!==url)return;feed.onload=null;feed.onerror=null;feed.removeAttribute('src');setVideoState(videoFallback?'fallback':'connecting','mjpeg');setVideoMessage('视频断开，正在重连…');clearTimeout(reconnectTimer);const delay=reconnectDelay;reconnectTimer=setTimeout(()=>{if(generation===videoGeneration&&shouldVideoRun())startMjpeg(generation,videoFallback);},delay);reconnectDelay=Math.min(reconnectDelay*2,10000);};
      feed.onload=onLoad;feed.onerror=onError;feed.setAttribute('src',url);clearTimeout(videoFirstFrameTimer);videoFirstFrameTimer=setTimeout(onError,6000);
    }
    function waitForIceGathering(peer,signal,timeout=2500){if(peer.iceGatheringState==='complete')return Promise.resolve();return new Promise((resolve,reject)=>{let timer=0;const finish=(error)=>{clearTimeout(timer);peer.removeEventListener('icegatheringstatechange',changed);signal?.removeEventListener('abort',aborted);error?reject(error):resolve();};const changed=()=>{if(peer.iceGatheringState==='complete')finish();};const aborted=()=>finish(new Error('视频连接已取消'));peer.addEventListener('icegatheringstatechange',changed);signal?.addEventListener('abort',aborted,{once:true});timer=setTimeout(()=>finish(new Error('WebRTC 本地 ICE 候选收集超时')),timeout);});}
    function fallbackToMjpeg(generation,reason='H.264 连接未能建立'){if(generation!==videoGeneration||!shouldVideoRun()||activeVideoTransport!=='webrtc_h264')return;videoFallbackReason=String(reason);console.warn('H.264 回退到 MJPEG:',videoFallbackReason);cleanupWebrtc();startMjpeg(generation,true);}
    async function startWebrtc(generation){
      if(generation!==videoGeneration||!shouldVideoRun())return;
      activeVideoTransport='webrtc_h264';setVideoState('connecting','webrtc_h264');setVideoMessage('正在连接 H.264…');showVideoSurface('webrtc_h264');
      const controller=new AbortController();webrtcAbort=controller;let peer=null,signalTimer=0;
      try {
        peer=new RTCPeerConnection({iceServers:bootstrap.ice_servers||[]});if(generation!==videoGeneration||!shouldVideoRun()){peer.close();return;}webrtcPeer=peer;
        const transceiver=peer.addTransceiver('video',{direction:'recvonly'});if(transceiver?.setCodecPreferences&&globalThis.RTCRtpReceiver?.getCapabilities){const codecs=RTCRtpReceiver.getCapabilities('video')?.codecs||[],h264=codecs.filter(codec=>String(codec.mimeType||'').toLowerCase()==='video/h264');if(h264.length)transceiver.setCodecPreferences([...h264,...codecs.filter(codec=>!h264.includes(codec))]);}
        peer.ontrack=event=>{if(generation!==videoGeneration||!shouldVideoRun()||activeVideoTransport!=='webrtc_h264'||event.track.kind!=='video')return;const stream=event.streams?.[0]||new MediaStream([event.track]);webrtcFeed.srcObject=stream;webrtcFeed.onplaying=()=>markVideoPlaying(generation,'webrtc_h264');webrtcFeed.onloadedmetadata=()=>{setDisplayScale(displayModes[modeIndex][0]);fitConsoleToVideo();};webrtcFeed.onresize=()=>fitConsoleToVideo();event.track.onended=()=>fallbackToMjpeg(generation,'H.264 视频轨道已结束');showVideoSurface('webrtc_h264');webrtcFeed.play().catch(()=>{});};
        peer.onconnectionstatechange=()=>{if(generation!==videoGeneration||peer!==webrtcPeer)return;if(peer.connectionState==='failed'||peer.connectionState==='closed')fallbackToMjpeg(generation,`WebRTC 连接状态：${peer.connectionState}`);else if(peer.connectionState==='disconnected'){clearTimeout(reconnectTimer);reconnectTimer=setTimeout(()=>{if(generation===videoGeneration&&peer===webrtcPeer&&peer.connectionState==='disconnected')fallbackToMjpeg(generation,'WebRTC 媒体连接已断开');},2000);}else clearTimeout(reconnectTimer);};
        peer.oniceconnectionstatechange=()=>{if(generation!==videoGeneration||peer!==webrtcPeer)return;if(peer.iceConnectionState==='failed')fallbackToMjpeg(generation,'WebRTC ICE 连通性检查失败');};
        const offer=await peer.createOffer();await peer.setLocalDescription(offer);await waitForIceGathering(peer,controller.signal);if(generation!==videoGeneration||!shouldVideoRun())return;
        signalTimer=setTimeout(()=>controller.abort(),8000);const answer=await request('/api/webrtc/offer',{method:'POST',body:JSON.stringify({type:'offer',sdp:peer.localDescription?.sdp||''}),signal:controller.signal});clearTimeout(signalTimer);signalTimer=0;if(generation!==videoGeneration||!shouldVideoRun()){closeWebrtcSession(answer?.session_id);return;}webrtcSessionId=answer?.session_id||null;if(!answer?.sdp||!/H264\/90000/i.test(answer.sdp))throw new Error('未协商 H.264');clearTimeout(videoFirstFrameTimer);videoFirstFrameTimer=setTimeout(()=>fallbackToMjpeg(generation,`等待 H.264 首帧超时（连接：${peer.connectionState}，ICE：${peer.iceConnectionState}）`),6000);await peer.setRemoteDescription({type:answer.type||'answer',sdp:answer.sdp});
      } catch(error) {clearTimeout(signalTimer);if(generation!==videoGeneration||!shouldVideoRun()){if(peer&&peer!==webrtcPeer)peer.close();return;}fallbackToMjpeg(generation,error?.name==='AbortError'?'H.264 信令请求超时':error?.message||String(error));}
    }
    function startVideo(force=false){if(!shouldVideoRun())return;if(force)stopVideo(false);else if(videoUiState!=='paused')return;const generation=++videoGeneration,canH264=h264Available();clearVideoTimers();videoFallback=false;videoFallbackReason=videoPreference==='webrtc_h264'&&!canH264?(bootstrap.webrtc?.message||'当前浏览器不支持 H.264 WebRTC'):'';reconnectDelay=500;showVideoSurface('none');setVideoMessage('正在连接视频…');if(videoPreference==='mjpeg'||!canH264)startMjpeg(generation,videoPreference==='webrtc_h264'&&!canH264);else startWebrtc(generation);}
    $('#mode-button').addEventListener('click',()=>{
      const nextIndex=(modeIndex+1)%displayModes.length;
      setDisplayScale(displayModes[nextIndex][0],true);
      toast(t('mode_prefix','画面比例: ')+displayModes[nextIndex][2]);
    });
    async function toggleFullscreen(){try{if(document.fullscreenElement)await document.exitFullscreen();else await consoleBox.requestFullscreen();}catch(e){toast(e.message,true);}}
    $$('#fullscreen,[data-viewer-fullscreen]').forEach(button=>button.addEventListener('click',toggleFullscreen));
    document.addEventListener('fullscreenchange',()=>{
      const isFs=!!document.fullscreenElement;
      $$('[data-viewer-fullscreen]').forEach(button=>button.textContent=isFs?t('exit_fullscreen','退出全屏'):t('fullscreen','进入全屏'));
      const fsBtn=$('#fullscreen');
      if(fsBtn){
        fsBtn.querySelector('.fullscreen-icon')?.classList.toggle('hidden',isFs);
        fsBtn.querySelector('.fullscreen-exit-icon')?.classList.toggle('hidden',!isFs);
        fsBtn.title=isFs?t('exit_fullscreen','退出全屏 (Esc/F11)'):t('fullscreen','全屏 (F11)');
        fsBtn.setAttribute('aria-label',isFs?t('exit_fullscreen','退出全屏'):t('fullscreen','全屏'));
      }
      if(!isFs)$('#console-head')?.classList.remove('revealed');
    });
    let fsMoveRaf = 0;
    document.addEventListener('pointermove', event => {
      if (!document.fullscreenElement) return;
      if (fsMoveRaf) return;
      const cy = event.clientY;
      fsMoveRaf = requestAnimationFrame(() => {
        fsMoveRaf = 0;
        if (!document.fullscreenElement) return;
        const head = $('#console-head');
        if (!head) return;
        if (cy < 48) head.classList.add('revealed');
        else if (cy > 84 && !head.matches(':hover') && !head.contains(document.activeElement)) head.classList.remove('revealed');
      });
    }, { passive: true });

    let drag = null, dragRaf = 0, dragArea = null, dragBoxWidth = 0, dragBoxHeight = 0, pendingDragX = 0, pendingDragY = 0;
    $('#console-head').addEventListener('pointerdown', event => {
      if (event.button !== 0 || event.target.closest('button,input,select,a') || matchMedia('(max-width:900px)').matches) return;
      const r = consoleBox.getBoundingClientRect(), w = $('#workspace').getBoundingClientRect();
      drag = { x: event.clientX - r.left, y: event.clientY - r.top, startX: event.clientX, startY: event.clientY };
      dragArea = w;
      dragBoxWidth = consoleBox.offsetWidth;
      dragBoxHeight = consoleBox.offsetHeight;
      consoleBox.classList.add('dragging');
      event.currentTarget.setPointerCapture(event.pointerId);
    });
    $('#console-head').addEventListener('pointermove', event => {
      if (!drag) return;
      pendingDragX = event.clientX;
      pendingDragY = event.clientY;
      if (Math.abs(event.clientX - drag.startX) > 3 || Math.abs(event.clientY - drag.startY) > 3) consoleUserMoved = true;
      if (!dragRaf) {
        dragRaf = requestAnimationFrame(() => {
          dragRaf = 0;
          if (!drag || !dragArea) return;
          const left = Math.max(0, Math.min(pendingDragX - dragArea.left - drag.x, dragArea.width - dragBoxWidth));
          const top = Math.max(0, Math.min(pendingDragY - dragArea.top - drag.y, dragArea.height - dragBoxHeight));
          consoleBox.style.left = `${left}px`;
          consoleBox.style.top = `${top}px`;
          invalidateSurfaceBox();
        });
      }
    });
    const stopDrag = () => {
      if (drag) {
        drag = null;
        if (dragRaf) { cancelAnimationFrame(dragRaf); dragRaf = 0; }
        consoleBox.classList.remove('dragging');
        invalidateSurfaceBox();
      }
    };
    $('#console-head').addEventListener('pointerup', stopDrag);
    $('#console-head').addEventListener('pointercancel', stopDrag);
    $('#console-head').addEventListener('dblclick', event => {
      if (event.target.closest('button,input,select,a')) return;
      consoleUserMoved = false;
      fitConsoleToVideo(true);
    });

    const SETTING_DEFAULTS = {
      suppress_hid_timeout: true,
      capture_pulse: true,
      release_key_toast: true,
      render_pixelated: true,
      auto_expand_inspector: true,
    };
    function getSetting(key, fallback = null) {
      try {
        const v = localStorage.getItem(`wingman_setting_${key}`);
        if (v === null) return fallback !== null ? fallback : SETTING_DEFAULTS[key];
        if (v === 'true') return true;
        if (v === 'false') return false;
        return v;
      } catch (_) {
        return fallback !== null ? fallback : SETTING_DEFAULTS[key];
      }
    }
    function setSetting(key, value) {
      try { localStorage.setItem(`wingman_setting_${key}`, String(value)); } catch (_) {}
    }
    let lastHidToast = 0;
    function isHidWritableTimeoutError(message) {
      if (!message) return false;
      const msg = String(message).toLowerCase();
      return msg.includes('writable') || msg.includes('timed out') || msg.includes('timeout') || msg.includes('超时') || msg.includes('未就绪');
    }
    function handleHidError(error) {
      const message = (error && (error.message || error.error)) || String(error);
      if (isHidWritableTimeoutError(message)) {
        if (getSetting('suppress_hid_timeout', true)) return;
        const now = performance.now();
        if (now - lastHidToast < 10000) return;
        lastHidToast = now;
      }
      toast(message, true);
    }

    function interactiveTarget(target){ return !!target.closest('input,textarea,select,button,a,[contenteditable="true"],dialog'); }
    function inputEnabled(){ return remoteWanted && pageActive && videoWorkspace==='video' && !app.classList.contains('hidden'); }
    function localExitShortcut(event){return remoteWanted&&event.ctrlKey&&event.altKey&&(event.code==='Backquote'||event.key==='`'||event.key==='~');}
    function elementVisible(el){if(!el||el.disabled)return false;const r=el.getBoundingClientRect(),style=getComputedStyle(el);return r.width>0&&r.height>0&&r.right>0&&r.bottom>0&&r.left<innerWidth&&r.top<innerHeight&&style.visibility!=='hidden'&&style.display!=='none';}
    function visibleLocalToggle(){
      const fullscreen=document.fullscreenElement,candidates=[$('#remote-input'),$('#input-state'),$('#inspector-toggle')];
      return candidates.find(el=>elementVisible(el)&&(!fullscreen||fullscreen.contains(el)))
        ||candidates.find(elementVisible)
        ||$('#input-state')||$('#remote-input');
    }
    function focusLocalToggle(){const target=visibleLocalToggle();if(!target)return;try{target.focus({preventScroll:true});}catch(_){try{target.focus();}catch(_){}}}
    function exitRemoteCapture(){
      setRemote(false);
      if(getSetting('release_key_toast',true))toast('已退出键鼠转发（本地快捷键 Ctrl+Alt+`）');
      const restore=()=>requestAnimationFrame(focusLocalToggle);
      if(document.fullscreenElement){
        try{const pending=document.exitFullscreen();if(pending&&typeof pending.finally==='function')pending.catch(()=>{}).finally(restore);else restore();}
        catch(_){restore();}
      } else restore();
    }
    function relativeCaptured(){return document.pointerLockElement===viewport;}
    function requestRelativeCapture(){try{const pending=viewport.requestPointerLock?.();if(pending&&typeof pending.catch==='function')pending.catch(()=>{});}catch(_){}}
    function relativeCaptureEnter(event){return event.key==='Enter'&&!event.ctrlKey&&!event.altKey&&!event.metaKey&&!event.shiftKey&&document.activeElement===viewport&&inputEnabled()&&!absoluteMode()&&!relativeCaptured();}
    function pulseViewport(){if(!getSetting('capture_pulse',true))return;viewport.classList.remove('just-captured');requestAnimationFrame(()=>{requestAnimationFrame(()=>{viewport.classList.add('just-captured');setTimeout(()=>viewport.classList.remove('just-captured'),450);});});}
    function syncInputUi(){const enabled=inputEnabled(),relative=enabled&&!absoluteMode(),captured=relative&&relativeCaptured();$$('#remote-input,.remote-input-mirror').forEach(input=>input.checked=remoteWanted);viewport.classList.toggle('remote',enabled);const stateEl=$('#input-state');if(stateEl){stateEl.classList.toggle('active',enabled);const text=!enabled?t('input_paused','输入已暂停'):relative?(captured?t('input_captured','相对鼠标已捕获'):t('input_click_to_capture','点击画面或按 Enter 捕获鼠标')):t('input_forwarding','键鼠正在转发');const textSpan=stateEl.querySelector('.input-text');if(textSpan)textSpan.textContent=text;else stateEl.textContent=text;stateEl.title=text+' (点击切换键鼠转发)';}$('#input-release')?.classList.toggle('hidden',!enabled);updateDiagnosticsCard();}
    function setRemote(enabled){remoteWanted=enabled;if(!enabled&&relativeCaptured())document.exitPointerLock();syncInputUi();if(!enabled)releaseAll();else pulseViewport();}
    $$('#remote-input,.remote-input-mirror').forEach(input=>input.addEventListener('change',event=>setRemote(event.target.checked)));
    $('#input-state')?.addEventListener('click',()=>{setRemote(!remoteWanted);if(getSetting('release_key_toast',true))toast(remoteWanted?t('toast_input_started','已开启键鼠转发'):t('toast_input_paused','已暂停键鼠转发'));});
    $('#input-release')?.addEventListener('click',async()=>{await releaseAll();if(relativeCaptured())document.exitPointerLock();if(getSetting('release_key_toast',true))toast(t('input_released','已释放所有按键与鼠标锁定'));});
    const VIRTUAL_MODIFIERS = new Set(['Control', 'Shift', 'Alt', 'Meta']);
    const activeModifiers = new Set();
    function updateVirtualModifierButtons() {
      $('#virtual-keyboard')?.querySelectorAll('button[data-key]').forEach(b => {
        if (VIRTUAL_MODIFIERS.has(b.dataset.key)) {
          b.classList.toggle('active', activeModifiers.has(b.dataset.key));
        }
      });
    }
    function resetVirtualModifiers() {
      if (activeModifiers.size > 0) {
        activeModifiers.clear();
        updateVirtualModifierButtons();
      }
    }
    async function sendKey(key, event={}) {
      if (!inputEnabled()) { toast(t('input_paused', '请先开启“转发键鼠”'), true); return; }
      const res = await request('/api/key', { method: 'POST', body: JSON.stringify({ key, ctrl: !!event.ctrlKey || activeModifiers.has('Control'), shift: !!event.shiftKey || activeModifiers.has('Shift'), alt: !!event.altKey || activeModifiers.has('Alt'), meta: !!event.metaKey || activeModifiers.has('Meta'), hold_ms: key.startsWith('F') ? 100 : 25 }) });
      if (res && res.keyboard_leds) renderKeyboardLeds(res.keyboard_leds);
      return res;
    }
    async function sendQuickKey(action){
      if(!inputEnabled())setRemote(true);
      try{
        let res;
        if(action==='cad'){res=await request('/api/key',{method:'POST',body:JSON.stringify({key:'Delete',ctrl:true,alt:true,hold_ms:100})});toast('已发送 Ctrl+Alt+Del');}
        else if(action==='win'){res=await request('/api/key',{method:'POST',body:JSON.stringify({key:'Meta',meta:true,hold_ms:60})});toast('已发送 Win');}
        else if(action==='alttab'){res=await request('/api/key',{method:'POST',body:JSON.stringify({key:'Tab',alt:true,hold_ms:100})});toast('已发送 Alt+Tab');}
        else if(action==='esc'){res=await request('/api/key',{method:'POST',body:JSON.stringify({key:'Escape',hold_ms:40})});toast('已发送 Esc');}
        if(res&&res.keyboard_leds)renderKeyboardLeds(res.keyboard_leds);
      }catch(e){toast(e.message,true);}
    }
    $$('[data-quick-key]').forEach(button=>button.addEventListener('click',()=>sendQuickKey(button.dataset.quickKey)));
    let latestKeyboardLeds = { num_lock: false, caps_lock: false, scroll_lock: false, raw: 0 };
    function renderKeyboardLeds(leds) {
      if (!leds) return;
      latestKeyboardLeds = leds;
      const num = !!leds.num_lock, caps = !!leds.caps_lock, scrl = !!leds.scroll_lock;
      $('#led-num')?.classList.toggle('active', num);
      $('#led-caps')?.classList.toggle('active', caps);
      $('#led-scroll')?.classList.toggle('active', scrl);
      $('#inspector-led-num')?.classList.toggle('active', num);
      $('#inspector-led-caps')?.classList.toggle('active', caps);
      $('#inspector-led-scroll')?.classList.toggle('active', scrl);
      $('#virtual-keyboard')?.querySelector('[data-key="CapsLock"]')?.classList.toggle('active', caps);
    }
    $$('[data-led]').forEach(button => button.addEventListener('click', async e => {
      e.stopPropagation();
      const key = button.dataset.led;
      if (!key) return;
      if (!inputEnabled()) setRemote(true);
      try {
        const res = await sendKey(key);
        if (res && res.keyboard_leds) renderKeyboardLeds(res.keyboard_leds);
        setTimeout(async () => {
          try {
            const l = await request('/api/hid/leds');
            renderKeyboardLeds(l);
          } catch (_) {}
        }, 120);
      } catch (err) {
        handleHidError(err);
      }
    }));
    document.addEventListener('keydown',event=>{if(localExitShortcut(event)){event.preventDefault();event.stopPropagation();if(!event.repeat)exitRemoteCapture();return;}if(relativeCaptureEnter(event)){event.preventDefault();event.stopPropagation();if(!event.repeat)requestRelativeCapture();return;}if(!inputEnabled()||interactiveTarget(event.target)||event.repeat)return;const key=normalizeKey(event);if(!key)return;event.preventDefault();sendKey(key,event).catch(handleHidError);});
    function normalizeKey(event){const code=event.code;if(/^Key[A-Z]$/.test(code)||/^Digit[0-9]$/.test(code)||/^F(?:[1-9]|1[0-2])$/.test(code)||['Escape','Delete','Enter','Tab','Backspace','Space','ArrowUp','ArrowDown','ArrowLeft','ArrowRight','Home','End','PageUp','PageDown','Insert','Minus','Equal','BracketLeft','BracketRight','Backslash','Semicolon','Quote','Backquote','Comma','Period','Slash','CapsLock'].includes(code))return code;return null;}
    $$('.key-strip [data-key]').forEach(button=>button.addEventListener('click',()=>sendKey(button.dataset.key).catch(handleHidError)));
    function releaseAll(){ mouseX=mouseY=0;absolutePending=null;resetVirtualModifiers();if(app.classList.contains('hidden')||bootstrap.authenticated===false)return Promise.resolve(); return request('/api/input/release-all',{method:'POST',keepalive:true}).catch(()=>{}); }
    function stopInput(){pageActive=false;remoteWanted=false;resetVirtualModifiers();if(relativeCaptured())document.exitPointerLock();syncInputUi();return releaseAll();}
    function suspendInput(){pageActive=false;if(relativeCaptured())document.exitPointerLock();syncInputUi();return releaseAll();}
    function resumeInput(){pageActive=!document.hidden&&document.hasFocus();syncInputUi();if(inputEnabled())scheduleMouse();}
    function handleVideoVisibility(){if(shouldVideoRun()){startVideo();refreshStatus();}else if(document.hidden||videoWorkspace!=='video'||app.classList.contains('hidden'))stopVideo();}
    document.addEventListener('visibilitychange',()=>{if(document.hidden){suspendInput();stopVideo();}else{resumeInput();handleVideoVisibility();}});window.addEventListener('blur',suspendInput);window.addEventListener('focus',resumeInput);window.addEventListener('pagehide',()=>{suspendInput();stopVideo();});window.addEventListener('pageshow',()=>{resumeInput();handleVideoVisibility();});

    function absoluteMode(){return bootstrap.capabilities?.pointer_mode==='absolute';}
    function activeVideoSurface(){return activeVideoTransport==='webrtc_h264'?webrtcFeed:feed;}
    function mapAbsolute(clientX,clientY){
      const surface=activeVideoSurface(),sourceWidth=surface instanceof HTMLVideoElement?surface.videoWidth:surface.naturalWidth,sourceHeight=surface instanceof HTMLVideoElement?surface.videoHeight:surface.naturalHeight;if(!sourceWidth||!sourceHeight)return null;
      if(!cachedSurfaceBox)cachedSurfaceBox=surface.getBoundingClientRect();
      const box=cachedSurfaceBox;if(!box.width||!box.height)return null;
      let left=box.left,top=box.top,width=box.width,height=box.height;
      if(consoleBox.classList.contains('mode-fit')){const scale=Math.min(box.width/sourceWidth,box.height/sourceHeight);width=sourceWidth*scale;height=sourceHeight*scale;left=box.left+(box.width-width)/2;top=box.top+(box.height-height)/2;}
      const nx=(clientX-left)/width,ny=(clientY-top)/height;if(nx<0||nx>=1||ny<0||ny>=1)return null;
      return {x:Math.max(0,Math.min(32767,Math.round(nx*32767))),y:Math.max(0,Math.min(32767,Math.round(ny*32767)))};
    }
    document.addEventListener('pointerlockchange',()=>{if(!relativeCaptured())mouseX=mouseY=0;syncInputUi();});document.addEventListener('pointerlockerror',()=>toast('浏览器未允许捕获鼠标',true));
    viewport.addEventListener('pointermove',event=>{
      if(isDraggingPan){
        event.preventDefault();event.stopPropagation();
        const dx=event.clientX-panStartX,dy=event.clientY-panStartY;
        viewport.scrollLeft=panScrollX-dx;viewport.scrollTop=panScrollY-dy;
        invalidateSurfaceBox();return;
      }
      if(!inputEnabled())return;
      if(absoluteMode()){const point=mapAbsolute(event.clientX,event.clientY);absolutePending=point;if(!point)return;}
      else{if(!relativeCaptured())return;mouseX+=event.movementX;mouseY+=event.movementY;}
      scheduleMouse();
    });
    viewport.addEventListener('pointerdown',event=>{
      if(event.button>2)return;
      if(consoleBox.classList.contains('mode-native')&&(isPanActive||event.button===1||event.altKey||!inputEnabled())){
        event.preventDefault();event.stopPropagation();
        isDraggingPan=true;panStartX=event.clientX;panStartY=event.clientY;panScrollX=viewport.scrollLeft;panScrollY=viewport.scrollTop;
        viewport.classList.add('panning-active');try{viewport.setPointerCapture?.(event.pointerId);}catch(_){}
        return;
      }
      if(!inputEnabled()){
        setRemote(true);
        if(getSetting('release_key_toast',true))toast('已激活键鼠转发 (点击状态栏或关闭开关可暂停)');
        viewport.focus({preventScroll:true});
        return;
      }
      event.preventDefault();viewport.focus({preventScroll:true});
      if(!absoluteMode()&&!relativeCaptured()){requestRelativeCapture();pulseViewport();return;}
      const button=event.button===0?1:event.button===2?2:4;
      if(absoluteMode()){const point=mapAbsolute(event.clientX,event.clientY);absolutePending=point;if(!point)return;request('/api/mouse/absolute',{method:'POST',body:JSON.stringify({action:'click',...point,button})}).catch(handleHidError);}
      else request('/api/mouse/click',{method:'POST',body:JSON.stringify({button})}).catch(handleHidError);
    });
    function stopPanning(event){if(isDraggingPan){isDraggingPan=false;viewport.classList.remove('panning-active');try{if(event?.pointerId)viewport.releasePointerCapture?.(event.pointerId);}catch(_){}invalidateSurfaceBox();}}
    viewport.addEventListener('pointerup',stopPanning);
    viewport.addEventListener('pointercancel',stopPanning);
    viewport.addEventListener('contextmenu',event=>{if(inputEnabled()||isPanActive)event.preventDefault();});
    viewport.addEventListener('wheel',event=>{
      if(consoleBox.classList.contains('mode-native')&&(isPanActive||event.altKey||!inputEnabled())){
        event.preventDefault();
        viewport.scrollLeft+=event.deltaX||(event.shiftKey?event.deltaY:0);
        viewport.scrollTop+=event.shiftKey?0:event.deltaY;
        invalidateSurfaceBox();
        return;
      }
      if(!inputEnabled()||(!absoluteMode()&&!relativeCaptured()))return;
      event.preventDefault();const wheel=Math.max(-127,Math.min(127,Math.round(-event.deltaY/40)||Math.sign(-event.deltaY)));if(absoluteMode()){const point=mapAbsolute(event.clientX,event.clientY);absolutePending=point;if(!point)return;request('/api/mouse/absolute',{method:'POST',body:JSON.stringify({action:'scroll',...point,delta:wheel})}).catch(handleHidError);}else request('/api/mouse/scroll',{method:'POST',body:JSON.stringify({wheel})}).catch(handleHidError);
    },{passive:false});
    function scheduleMouse(){const pending=absoluteMode()?absolutePending:mouseX||mouseY;if(mouseBusy||mouseTimer||!pending)return;const wait=Math.max(0,16-(performance.now()-lastMouseSend));mouseTimer=setTimeout(flushMouse,wait);}
    async function flushMouse(){mouseTimer=0;if(mouseBusy||!inputEnabled())return;let url,body;if(absoluteMode()){if(!absolutePending)return;const point=absolutePending;absolutePending=null;url='/api/mouse/absolute';body={action:'move',...point};}else{const dx=Math.max(-127,Math.min(127,mouseX)),dy=Math.max(-127,Math.min(127,mouseY));if(!dx&&!dy)return;mouseX-=dx;mouseY-=dy;url='/api/mouse/move';body={dx,dy};}mouseBusy=true;lastMouseSend=performance.now();try{await request(url,{method:'POST',body:JSON.stringify(body)});}catch(e){handleHidError(e);}finally{mouseBusy=false;scheduleMouse();}}

    const rows=[['Escape','F1','F2','F3','F4','F5','F6','F7','F8','F9','F10','F11','F12','Delete'],['`','1','2','3','4','5','6','7','8','9','0','-','=','Backspace'],['Tab','Q','W','E','R','T','Y','U','I','O','P','[',']','\\'],['CapsLock','A','S','D','F','G','H','J','K','L',';','\'','Enter'],['Shift','Z','X','C','V','B','N','M',',','.','/','Shift'],['Control','Meta','Alt','Spacebar','Alt','Meta','Control']];
    const vk=$('#virtual-keyboard');rows.forEach(row=>{const line=document.createElement('div');line.className='keyboard-row';row.forEach(key=>{const b=document.createElement('button');b.type='button';b.textContent=key==='Spacebar'?'空格':key;b.dataset.key=key;if(key==='Spacebar')b.className='grow';b.addEventListener('click',()=>{if(VIRTUAL_MODIFIERS.has(key)){if(activeModifiers.has(key))activeModifiers.delete(key);else activeModifiers.add(key);updateVirtualModifierButtons();return;}sendKey(key).catch(handleHidError);});line.append(b);});vk.append(line);});
    let terminalSocket=null,terminal=null,terminalFit=null,terminalFitFrame=0,terminalLastSize='';
    const terminalEncoder=new TextEncoder();
    function ensureTerminal(){
      if(terminal)return;
      terminal=new Terminal({cursorBlink:true,cursorStyle:'block',fontFamily:'"Geist Mono",ui-monospace,SFMono-Regular,Menlo,Monaco,monospace',fontSize:13,lineHeight:1.25,scrollback:5000,convertEol:false,macOptionIsMeta:true,theme:{background:'#111111',foreground:'#e8e8e8',cursor:'#f5f5f5',cursorAccent:'#111111',selectionBackground:'#ffffff38',black:'#1d1f21',red:'#ff5f56',green:'#27c93f',yellow:'#ffbd2e',blue:'#5aa9ff',magenta:'#c792ea',cyan:'#66d9ef',white:'#e8e8e8',brightBlack:'#666666',brightRed:'#ff6e67',brightGreen:'#5af78e',brightYellow:'#f4f99d',brightBlue:'#7aa2f7',brightMagenta:'#d2a8ff',brightCyan:'#9aedfe',brightWhite:'#ffffff'}});
      terminalFit=new FitAddon.FitAddon();terminal.loadAddon(terminalFit);terminal.open($('#terminal-host'));
      terminal.onData(data=>{if(terminalSocket?.readyState===WebSocket.OPEN)terminalSocket.send(terminalEncoder.encode(data));});
      terminal.onResize(size=>terminalResize(size.cols,size.rows));
      new ResizeObserver(scheduleTerminalFit).observe($('#terminal-window'));
      document.fonts?.ready.then(scheduleTerminalFit);
    }
    function scheduleTerminalFit(){if(terminalFitFrame||$('#terminal-window').hidden)return;terminalFitFrame=requestAnimationFrame(()=>{terminalFitFrame=0;terminalFit?.fit();});}
    function terminalResize(cols,rows){const size=`${cols}x${rows}`;if(terminalSocket?.readyState===WebSocket.OPEN&&size!==terminalLastSize){terminalLastSize=size;terminalSocket.send(JSON.stringify({type:'resize',cols,rows}));}}
    function terminalConnect(){
      ensureTerminal();
      if(bootstrap.terminal_ready===false){terminal.writeln(`\r\n\x1b[31m${bootstrap.terminal_message||'终端当前不可用；请退出后重新登录以重试。KVM 画面与输入不受影响。'}\x1b[0m`);$('#terminal-reconnect')?.classList.remove('hidden');return;}
      if(terminalSocket&&terminalSocket.readyState<=WebSocket.OPEN)return;
      const scheme=location.protocol==='https:'?'wss':'ws';const socket=new WebSocket(`${scheme}://${location.host}/api/terminal/ws`);terminalSocket=socket;terminalLastSize='';socket.binaryType='arraybuffer';
      socket.onopen=()=>{if(terminalSocket!==socket)return;$('#terminal-reconnect')?.classList.add('hidden');scheduleTerminalFit();requestAnimationFrame(()=>{terminalResize(terminal.cols,terminal.rows);terminal.focus();});};
      socket.onmessage=event=>{if(terminalSocket!==socket)return;terminal.write(typeof event.data==='string'?event.data:new Uint8Array(event.data));};
      socket.onerror=()=>{if(terminalSocket!==socket)return;terminal.writeln('\r\n\x1b[31m终端连接失败\x1b[0m');};
      socket.onclose=()=>{if(terminalSocket!==socket)return;terminalSocket=null;if(videoWorkspace==='terminal')$('#terminal-reconnect')?.classList.remove('hidden');};
    }
    function terminalReset(){if(!terminal)return;terminalLastSize='';terminal.reset();}
    function terminalDisconnect(){if(terminalSocket){const socket=terminalSocket;terminalSocket=null;socket.close();}terminalReset();$('#terminal-reconnect')?.classList.remove('hidden');}
    $('#terminal-reconnect')?.addEventListener('click',async()=>{try{bootstrap=await request('/api/bootstrap');}catch(_){}terminalReset();terminalConnect();});
    $('#terminal-clear')?.addEventListener('click',()=>terminal?.clear());
    const keyboardDialog=$('#keyboard-dialog');function syncKeyboardDialogState(open){app.classList.toggle('keyboard-open',open);$$('#keyboard-toggle,#keyboard-toggle-mobile').forEach(btn=>{btn.classList.toggle('active',open);btn.setAttribute('aria-expanded',String(open));});invalidateSurfaceBox();fitConsoleToVideo();}
    $$('#keyboard-toggle,#keyboard-toggle-mobile').forEach(button=>button.addEventListener('click',()=>{if(keyboardDialog.open){resetVirtualModifiers();keyboardDialog.close();syncKeyboardDialogState(false);}else{keyboardDialog.show();syncKeyboardDialogState(true);}}));$('[data-close]',keyboardDialog).addEventListener('click',()=>{resetVirtualModifiers();keyboardDialog.close();syncKeyboardDialogState(false);});keyboardDialog.addEventListener('close',()=>{resetVirtualModifiers();syncKeyboardDialogState(false);});

    function setWorkspaceMode(mode){const next=mode==='terminal'?'terminal':'video',terminalMode=next==='terminal',changed=videoWorkspace!==next;videoWorkspace=next;invalidateSurfaceBox();$('#terminal-window').hidden=!terminalMode;$('#video-viewport').hidden=terminalMode;$$('[data-workspace]').forEach(tab=>tab.classList.toggle('active',tab.dataset.workspace===next));$('#terminal-clear')?.classList.toggle('hidden',!terminalMode);if(!terminalMode)$('#terminal-reconnect')?.classList.add('hidden');else if(!terminalSocket||terminalSocket.readyState>WebSocket.OPEN)$('#terminal-reconnect')?.classList.remove('hidden');$('.quick-keys')?.classList.toggle('hidden',terminalMode);$('#input-state')?.classList.toggle('hidden',terminalMode);$('#mode-button')?.classList.toggle('hidden',terminalMode);$('#video-transport-label')?.classList.toggle('hidden',terminalMode);syncInputUi();if(terminalMode){if(changed)stopVideo();terminalConnect();scheduleTerminalFit();requestAnimationFrame(()=>terminal?.focus());}else{if(changed)startVideo();fitConsoleToVideo();}}
    $$('[data-workspace]').forEach(tab=>tab.addEventListener('click',()=>setWorkspaceMode(tab.dataset.workspace)));
    $$('.tab-button').forEach(button=>button.addEventListener('click',()=>{const mode=button.dataset.panelTarget;$$('.tab-button').forEach(item=>item.classList.toggle('active',item===button));$$('[data-panel]').forEach(panel=>panel.hidden=panel.dataset.panel!==mode);}));
    const inspectorToggle=$('#inspector-toggle');
    function setInspectorOpen(open){invalidateSurfaceBox();app.classList.toggle('inspector-open',open);inspectorToggle.setAttribute('aria-expanded',String(open));inspectorToggle.classList.toggle('active',open);inspectorToggle.textContent=open?t('collapse_inspector','收起面板'):t('open_inspector','控制面板');try{localStorage.setItem('wingman_inspector_open',open?'true':'false');}catch(_){}requestAnimationFrame(()=>{invalidateSurfaceBox();if(videoWorkspace==='terminal')scheduleTerminalFit();else fitConsoleToVideo();});}
    inspectorToggle.addEventListener('click',()=>setInspectorOpen(!app.classList.contains('inspector-open')));
    $('#inspector-close')?.addEventListener('click',()=>setInspectorOpen(false));
    $('#inspector-backdrop')?.addEventListener('click',()=>setInspectorOpen(false));
    $('#inspector-float-open')?.addEventListener('click',()=>setInspectorOpen(true));
    window.addEventListener('keydown',event=>{if(event.key==='Escape'&&!inputEnabled()){if(topbarPerfPopover&&!topbarPerfPopover.hasAttribute('hidden')){closeTopbarPerf();return;}if(app.classList.contains('inspector-open')&&!keyboardDialog?.open&&!settingsDialog?.open){setInspectorOpen(false);}}});

    async function refreshStatus(){if(statusTimer===-1)return;if(statusTimer>0)clearTimeout(statusTimer);if(app.classList.contains('hidden')||document.hidden){statusTimer=0;renderVideoUi();return;}statusTimer=-1;try{const status=await request('/api/status');latestVideoStatus=status.video||null;latestPowerStatus=status.power||null;latestSystemStatus=status.system||null;renderPowerLed(latestPowerStatus);if(status.hid?.keyboard_leds)renderKeyboardLeds(status.hid.keyboard_leds);if(status.client)renderClientInfo(status.client);if(latestSystemStatus)renderSystemMetrics(latestSystemStatus);renderVideoUi();}catch{latestVideoStatus={state:'offline',message:'连接失败'};latestPowerStatus={power_led:{configured:true,state:'unknown',sense_error:'连接失败'}};renderPowerLed(latestPowerStatus);renderVideoUi();}finally{statusTimer=app.classList.contains('hidden')||document.hidden?0:setTimeout(()=>{statusTimer=0;refreshStatus();},3000);}}
    const mediaChoices=new Map();let mediaBusy=false,mediaServerBusy=false,mediaStatus={};
    function mediaItems(data){return Array.isArray(data)?data:(data?.items||data?.images||[]);}
    function mediaTypeLabel(type){return type==='cdrom'?'光驱':type==='disk'?'U 盘':'自动';}
    function mediaStateLabel(state){return ({attached:'已挂载',mounted:'已挂载',detached:'未挂载',attaching:'正在挂载',detaching:'正在弹出',ejecting:'正在弹出',error:'状态异常'})[state]||'';}
    function formatBytes(value){const bytes=Number(value);if(!Number.isFinite(bytes)||bytes<0)return '';if(bytes<1024)return `${bytes} B`;const units=['KB','MB','GB','TB'];let size=bytes,index=-1;do{size/=1024;index++;}while(size>=1024&&index<units.length-1);return `${size>=10?size.toFixed(0):size.toFixed(1)} ${units[index]}`;}
    function formatUptime(seconds){const s=Math.floor(Number(seconds)||0);if(s<=0)return '--';const days=Math.floor(s/86400);const hours=Math.floor((s%86400)/3600);const mins=Math.floor((s%3600)/60);if(days>0)return `${days}d ${hours}h`;if(hours>0)return `${hours}h ${mins}m`;return `${mins}m ${s%60}s`;}
    const topbarPerfBtn=$('#topbar-perf-btn'),topbarPerfPopover=$('#topbar-perf-popover');
    let perfPopoverPinned=false,perfHoverTimer=0,perfLeaveTimer=0;
    function openTopbarPerf(pinned=false){
      if(!topbarPerfPopover)return;
      topbarPerfPopover.removeAttribute('hidden');
      topbarPerfBtn?.setAttribute('aria-expanded','true');
      topbarPerfBtn?.classList.add('active');
      perfPopoverPinned=!!pinned;
      $('#power-menu')?.setAttribute('hidden','');$('#power-menu-toggle')?.setAttribute('aria-expanded','false');
      $('#lang-menu')?.setAttribute('hidden','');$('#lang-toggle')?.setAttribute('aria-expanded','false');
      $('#diagnostics-popover')?.setAttribute('hidden','');$('#diagnostics-toggle')?.setAttribute('aria-expanded','false');
      if(latestSystemStatus)renderSystemMetrics(latestSystemStatus);
    }
    function closeTopbarPerf(){
      if(!topbarPerfPopover)return;
      topbarPerfPopover.setAttribute('hidden','');
      topbarPerfBtn?.setAttribute('aria-expanded','false');
      topbarPerfBtn?.classList.remove('active');
      perfPopoverPinned=false;
      clearTimeout(perfHoverTimer);
      clearTimeout(perfLeaveTimer);
    }
    function toggleTopbarPerf(pinned=true){
      if(!topbarPerfPopover||topbarPerfPopover.hasAttribute('hidden'))openTopbarPerf(pinned);
      else closeTopbarPerf();
    }
    topbarPerfBtn?.addEventListener('click',event=>{event.stopPropagation();toggleTopbarPerf(true);});
    topbarPerfBtn?.addEventListener('pointerenter',()=>{
      if(matchMedia('(pointer:coarse)').matches)return;
      clearTimeout(perfLeaveTimer);
      if(topbarPerfPopover?.hasAttribute('hidden')){
        perfHoverTimer=setTimeout(()=>openTopbarPerf(false),200);
      }
    });
    topbarPerfBtn?.addEventListener('pointerleave',()=>{
      if(matchMedia('(pointer:coarse)').matches)return;
      clearTimeout(perfHoverTimer);
      if(topbarPerfPopover&&!topbarPerfPopover.hasAttribute('hidden')&&!perfPopoverPinned){
        perfLeaveTimer=setTimeout(()=>closeTopbarPerf(),250);
      }
    });
    topbarPerfPopover?.addEventListener('pointerenter',()=>{
      if(matchMedia('(pointer:coarse)').matches)return;
      clearTimeout(perfLeaveTimer);
    });
    topbarPerfPopover?.addEventListener('pointerleave',()=>{
      if(matchMedia('(pointer:coarse)').matches)return;
      if(topbarPerfPopover&&!topbarPerfPopover.hasAttribute('hidden')&&!perfPopoverPinned){
        perfLeaveTimer=setTimeout(()=>closeTopbarPerf(),250);
      }
    });
    $('#perf-pop-close')?.addEventListener('click',event=>{event.stopPropagation();closeTopbarPerf();});
    $('#pop-more-diag')?.addEventListener('click',event=>{event.stopPropagation();closeTopbarPerf();$('#diagnostics-toggle')?.click();});

    function renderSystemMetrics(sys){
      if(!sys)return;
      latestSystemStatus=sys;
      const cpu=sys.cpu,mem=sys.memory,disk=sys.disk;
      if(cpu){
        const cpuPct=Math.min(100,Math.max(0,Number(cpu.percent)||0));
        const cpuStr=`${cpuPct.toFixed(1)}%`,cpuRoundStr=`${Math.round(cpuPct)}%`;
        const cpuBarColor=cpuPct>85?'var(--red)':cpuPct>70?'var(--amber)':'var(--blue)';
        const coresStr=`${cpu.cores||1} ${t('cpu_cores','核')}`;
        const cpuEl=$('#perf-cpu-val'),bar=$('#perf-cpu-bar');if(cpuEl)cpuEl.textContent=cpuStr;if(bar){bar.style.width=`${cpuPct}%`;bar.style.backgroundColor=cpuBarColor;}
        const coresEl=$('#perf-cpu-cores');if(coresEl)coresEl.textContent=coresStr;
        const diagCpu=$('#diag-cpu');if(diagCpu)diagCpu.textContent=`${cpuStr} (${coresStr})`;
        const topbarCpu=$('#topbar-cpu-val');if(topbarCpu)topbarCpu.textContent=cpuStr;
        const topbarCompact=$('#topbar-compact-val');if(topbarCompact)topbarCompact.textContent=cpuRoundStr;
        const popCpu=$('#pop-cpu-val');if(popCpu)popCpu.textContent=cpuStr;
        const popBar=$('#pop-cpu-bar');if(popBar){popBar.style.width=`${cpuPct}%`;popBar.style.backgroundColor=cpuBarColor;}
        const popCores=$('#pop-cpu-cores');if(popCores)popCores.textContent=coresStr;
      }
      if(sys.load_avg&&Array.isArray(sys.load_avg)){
        const load1=sys.load_avg.length>0?Number(sys.load_avg[0]).toFixed(2):'--';
        const loadStr=sys.load_avg.map(v=>Number(v).toFixed(2)).join(', ');
        const loadEl=$('#perf-load');if(loadEl){loadEl.textContent=`${t('load','负载')} ${load1}`;loadEl.title=`${t('load_average','系统负载')}: ${loadStr}`;}
        const diagLoad=$('#diag-load');if(diagLoad)diagLoad.textContent=loadStr;
        const popLoad=$('#pop-load-val');if(popLoad)popLoad.textContent=loadStr;
      }
      if(mem){
        const memPct=Math.min(100,Math.max(0,Number(mem.percent)||0));
        const memStr=`${memPct.toFixed(0)}%`;
        const memBarColor=memPct>85?'var(--red)':memPct>70?'var(--amber)':'var(--blue)';
        const memDetailStr=mem.total_bytes>0?`${formatBytes(mem.used_bytes)} / ${formatBytes(mem.total_bytes)}`:'--';
        const memEl=$('#perf-mem-val'),bar=$('#perf-mem-bar');if(memEl)memEl.textContent=memStr;if(bar){bar.style.width=`${memPct}%`;bar.style.backgroundColor=memBarColor;}
        const memDetail=$('#perf-mem-detail');if(memDetail)memDetail.textContent=memDetailStr;
        const diagMem=$('#diag-mem');if(diagMem)diagMem.textContent=mem.total_bytes>0?`${memDetailStr} (${memStr})`:'--';
        const topbarMem=$('#topbar-mem-val');if(topbarMem)topbarMem.textContent=memStr;
        const popMem=$('#pop-mem-val');if(popMem)popMem.textContent=memStr;
        const popBar=$('#pop-mem-bar');if(popBar){popBar.style.width=`${memPct}%`;popBar.style.backgroundColor=memBarColor;}
        const popDetail=$('#pop-mem-detail');if(popDetail)popDetail.textContent=memDetailStr;
      }
      if(disk){
        const diskPct=Math.min(100,Math.max(0,Number(disk.percent)||0));
        const diskStr=`${diskPct.toFixed(0)}%`;
        const diskBarColor=diskPct>85?'var(--red)':diskPct>70?'var(--amber)':'var(--blue)';
        const diskDetailStr=disk.total_bytes>0?`${formatBytes(disk.used_bytes)} / ${formatBytes(disk.total_bytes)}`:'--';
        const diskEl=$('#perf-disk-val'),bar=$('#perf-disk-bar');if(diskEl)diskEl.textContent=diskStr;if(bar){bar.style.width=`${diskPct}%`;bar.style.backgroundColor=diskBarColor;}
        const diskDetail=$('#perf-disk-detail');if(diskDetail)diskDetail.textContent=diskDetailStr;
        const diagDisk=$('#diag-disk');if(diagDisk)diagDisk.textContent=disk.total_bytes>0?`${diskDetailStr} (${diskStr})`:'--';
        const popDisk=$('#pop-disk-val');if(popDisk)popDisk.textContent=diskStr;
        const popBar=$('#pop-disk-bar');if(popBar){popBar.style.width=`${diskPct}%`;popBar.style.backgroundColor=diskBarColor;}
        const popDetail=$('#pop-disk-detail');if(popDetail)popDetail.textContent=diskDetailStr;
      }
      if(sys.temperature!==null&&sys.temperature!==undefined){
        const temp=Number(sys.temperature);
        const tempStr=temp.toFixed(1);
        const isWarm=temp>=75;
        const tempBadge=$('#perf-temp-badge'),tempVal=$('#perf-temp-val');if(tempBadge&&tempVal){tempBadge.classList.remove('hidden');tempVal.textContent=tempStr;tempBadge.classList.toggle('warning',isWarm);}
        const diagTempCard=$('#diag-temp-card'),diagTemp=$('#diag-temp');if(diagTempCard&&diagTemp){diagTempCard.classList.remove('hidden');diagTemp.textContent=`${tempStr} °C`;}
        const topbarTempSeg=$('#topbar-temp-seg'),topbarTempVal=$('#topbar-temp-val');if(topbarTempSeg&&topbarTempVal){topbarTempSeg.classList.remove('hidden');topbarTempVal.textContent=`${tempStr}°C`;}
        const popTempBadge=$('#pop-temp-badge'),popTempVal=$('#pop-temp-val');if(popTempBadge&&popTempVal){popTempBadge.classList.remove('hidden');popTempVal.textContent=tempStr;popTempBadge.classList.toggle('warning',isWarm);}
      }else{
        $('#perf-temp-badge')?.classList.add('hidden');
        $('#diag-temp-card')?.classList.add('hidden');
        $('#topbar-temp-seg')?.classList.add('hidden');
        $('#pop-temp-badge')?.classList.add('hidden');
      }
      if(sys.uptime_seconds){
        const upStr=formatUptime(sys.uptime_seconds);
        const upEl=$('#perf-uptime'),diagUp=$('#diag-uptime'),popUp=$('#pop-uptime-val');
        if(upEl)upEl.textContent=upStr;
        if(diagUp)diagUp.textContent=upStr;
        if(popUp)popUp.textContent=upStr;
      }
    }
    function mountedMediaName(status,items){return status?.mounted_name||items.find(item=>typeof item!=='string'&&(item.mounted||item.attached))?.name||'';}
    function writableMountedDisk(status,name){return !!name&&status?.read_only===false&&status?.media_type==='disk';}
    function syncMediaBusy(){const locked=mediaBusy||mediaServerBusy;$$('.media-operation').forEach(control=>control.disabled=locked||control.dataset.mediaLocked==='true');$$('#media-upload input,#media-upload button').forEach(control=>control.disabled=mediaBusy);$('#media-refresh').disabled=mediaBusy;$('#media-list').setAttribute('aria-busy',String(locked));}
    async function runMediaAction(action,success){if(mediaBusy||mediaServerBusy)return;mediaBusy=true;syncMediaBusy();try{await action();toast(success);await refreshMedia();}catch(error){toast(error.message,true);await refreshMedia();}finally{mediaBusy=false;syncMediaBusy();}}
    function renderMediaStatus(status,items){const box=$('#media-status'),name=mountedMediaName(status,items),state=status?.state||(name?'attached':'detached'),mounted=!!name;mediaServerBusy=['attaching','detaching','ejecting','busy'].includes(state);box.className=`media-status${mounted?' mounted':''}${mediaServerBusy?' busy':''}${state==='error'?' error':''}`;box.replaceChildren();const dot=document.createElement('span');dot.className='status-dot';const copy=document.createElement('span');copy.className='media-status-copy';const title=document.createElement('strong');title.textContent=name||mediaStateLabel(state)||'未挂载';const detail=document.createElement('span');if(mounted){const parts=[mediaTypeLabel(status?.media_type),status?.read_only===false?'读写':'只读'];const stateText=mediaStateLabel(state);if(stateText&&stateText!=='已挂载')parts.unshift(stateText);detail.textContent=parts.join(' · ');}else detail.textContent=state==='error'?'请检查 LUN 配置':'选择镜像即可挂载';copy.append(title,detail);box.append(dot,copy);if(mounted){const actions=document.createElement('span');actions.className='media-status-actions';const detach=document.createElement('button');detach.type='button';detach.className='secondary media-operation';detach.textContent='弹出';detach.addEventListener('click',()=>{if(writableMountedDisk(status,name)&&!confirm('请先在目标机中弹出此 U 盘。确认已经弹出并继续吗？'))return;runMediaAction(()=>request('/api/media/detach',{method:'POST',body:JSON.stringify({force:false})}),'介质已弹出');});actions.append(detach);if(status?.forced_eject_supported){const force=document.createElement('button');force.type='button';force.className='danger media-operation';force.textContent='强制弹出';force.title='强制弹出可能损坏镜像文件系统';force.addEventListener('click',()=>{if(!confirm('强制弹出可能损坏镜像文件系统，确定继续吗？'))return;runMediaAction(()=>request('/api/media/detach',{method:'POST',body:JSON.stringify({force:true})}),'介质已强制弹出');});actions.append(force);}box.append(actions);}syncMediaBusy();}
    function renderMediaItem(item,mountedName){const data=typeof item==='string'?{name:item,size:null,mounted:false}:item,name=data.name||data.path||'',isIso=name.toLowerCase().endsWith('.iso'),mounted=!!(data.mounted||data.attached||name===mountedName),anotherMounted=!!mountedName&&!mounted,saved=mediaChoices.get(name)||{},defaultType=isIso?'cdrom':'disk',type=mounted?(mediaStatus.media_type||saved.media_type||defaultType):(saved.media_type||defaultType),readOnly=isIso||type==='cdrom'?true:mounted?mediaStatus.read_only!==false:(saved.read_only??(bootstrap?.config?.media?.read_only_by_default??false));const row=document.createElement('article');row.className='media-item';const head=document.createElement('div');head.className='media-item-head';const nameBox=document.createElement('span');nameBox.className='media-item-name';const title=document.createElement('strong');title.textContent=name;title.title=name;const meta=document.createElement('span');meta.className='media-item-meta';const size=formatBytes(data.size);if(size)meta.append(document.createTextNode(size));if(mounted){const badge=document.createElement('span');badge.className='media-badge';badge.textContent='已挂载';meta.append(badge);}nameBox.append(title,meta);head.append(nameBox);const controls=document.createElement('div');controls.className='media-item-controls';const select=document.createElement('select');select.className='media-type-select media-operation';select.setAttribute('aria-label',`${name} 的介质模式`);[['auto','自动'],['cdrom','光驱'],['disk','U 盘']].forEach(([value,label])=>{const option=document.createElement('option');option.value=value;option.textContent=label;select.append(option);});select.value=type;select.dataset.mediaLocked=String(mounted);const readonlyLabel=document.createElement('label');readonlyLabel.className='media-readonly';const readonly=document.createElement('input');readonly.type='checkbox';readonly.checked=readOnly;readonly.dataset.mediaLocked=String(isIso||mounted||type==='cdrom');readonly.className='media-operation';
readonly.setAttribute('aria-label',`${name} 只读`);readonlyLabel.append(readonly,document.createTextNode('只读'));if(isIso)readonlyLabel.title='ISO 始终以只读方式挂载';select.addEventListener('change',()=>{if(select.value==='cdrom'){readonly.checked=true;readonly.dataset.mediaLocked='true';}else readonly.dataset.mediaLocked=String(isIso||mounted);mediaChoices.set(name,{media_type:select.value,read_only:readonly.checked});syncMediaBusy();});readonly.addEventListener('change',()=>mediaChoices.set(name,{media_type:select.value,read_only:readonly.checked}));controls.append(select,readonlyLabel);if(!mounted){const attach=document.createElement('button');attach.type='button';attach.className='primary media-attach media-operation';attach.textContent='挂载';attach.dataset.mediaLocked=String(anotherMounted);if(anotherMounted)attach.title='请先弹出当前介质';attach.addEventListener('click',()=>{const payload={name,media_type:select.value,read_only:isIso||select.value==='cdrom'?true:readonly.checked};mediaChoices.set(name,{media_type:payload.media_type,read_only:payload.read_only});runMediaAction(()=>request('/api/media/attach',{method:'POST',body:JSON.stringify(payload)}),'镜像已挂载');});controls.append(attach);}row.append(head,controls);return row;}
    function renderMedia(data){const box=$('#media-list'),items=mediaItems(data),status=data?.status||{};mediaStatus=status;const mountedName=mountedMediaName(status,items);renderMediaStatus(status,items);box.replaceChildren();if(!items.length){box.textContent='暂无镜像';syncMediaBusy();return;}items.forEach(item=>box.append(renderMediaItem(item,mountedName)));syncMediaBusy();}
    async function refreshMedia(){const ownsBusy=!mediaBusy;if(ownsBusy){mediaBusy=true;syncMediaBusy();}try{renderMedia(await request('/api/media'));}catch(error){mediaServerBusy=false;$('#media-list').textContent=error.message;const status=$('#media-status');status.className='media-status error';status.innerHTML='<span class="status-dot"></span><span class="media-status-copy"><strong>状态不可用</strong></span>';}finally{if(ownsBusy){mediaBusy=false;syncMediaBusy();}}}
    $('#media-refresh').addEventListener('click',refreshMedia);
    $('#media-upload').addEventListener('submit',event=>{event.preventDefault();if(mediaBusy)return;const form=event.currentTarget,bar=$('#upload-bar'),progress=bar.parentElement,xhr=new XMLHttpRequest();mediaBusy=true;syncMediaBusy();xhr.open('POST','/api/media/upload');xhr.upload.onprogress=e=>{if(!e.lengthComputable)return;const value=Math.round(e.loaded/e.total*100);bar.style.width=`${value}%`;progress.setAttribute('aria-valuenow',String(value));};const finish=async(success,message)=>{bar.style.width='0';progress.setAttribute('aria-valuenow','0');if(success){toast('镜像上传完成');form.reset();await refreshMedia();}else toast(message||'上传失败',true);mediaBusy=false;syncMediaBusy();};xhr.onload=()=>{let message=xhr.responseText;try{const data=JSON.parse(message);message=data.error||data.message||message;}catch{}finish(xhr.status>=200&&xhr.status<300,message);};xhr.onerror=()=>finish(false,'上传连接失败');xhr.onabort=()=>finish(false,'上传已取消');xhr.send(new FormData(form));});

    function base64UrlToBuffer(base64url) {
      if (!base64url) return new ArrayBuffer(0);
      if (base64url instanceof ArrayBuffer) return base64url;
      if (ArrayBuffer.isView(base64url)) return base64url.buffer;
      const padding = '='.repeat((4 - (base64url.length % 4)) % 4);
      const base64 = (base64url + padding).replace(/-/g, '+').replace(/_/g, '/');
      const rawData = atob(base64);
      const outputArray = new Uint8Array(rawData.length);
      for (let i = 0; i < rawData.length; ++i) {
        outputArray[i] = rawData.charCodeAt(i);
      }
      return outputArray.buffer;
    }
    function bufferToBase64Url(buffer) {
      if (!buffer) return '';
      const bytes = buffer instanceof Uint8Array ? buffer : new Uint8Array(buffer);
      let binary = '';
      for (let i = 0; i < bytes.byteLength; i++) {
        binary += String.fromCharCode(bytes[i]);
      }
      return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
    }
    function isIpAddress(hostname) {
      if (!hostname || hostname === 'localhost') return false;
      return /^(\d{1,3}\.){3}\d{1,3}$/.test(hostname) || hostname.startsWith('[') || hostname.includes(':');
    }
    function isPasskeySupported() {
      return !!(window.isSecureContext && window.PublicKeyCredential);
    }
    async function registerPasskey() {
      if (!isPasskeySupported()) {
        toast(t('passkey_insecure'), true);
        return;
      }
      if (isIpAddress(window.location.hostname)) {
        toast(t('passkey_ip_warn'), true);
        return;
      }
      const defaultName = navigator.userAgent.includes('Macintosh') ? 'Mac (Touch ID)' :
                          navigator.userAgent.includes('iPhone') ? 'iPhone (Face ID)' :
                          navigator.userAgent.includes('iPad') ? 'iPad (Touch/Face ID)' :
                          navigator.userAgent.includes('Windows') ? 'Windows Hello' : 'Passkey Device';
      const name = prompt(t('passkey_prompt_name'), defaultName);
      if (name === null) return;
      const passkeyName = name.trim() || defaultName;

      const btn = $('#passkey-add-btn');
      const oldText = btn?.textContent || '';
      if (btn) { btn.disabled = true; btn.textContent = '...'; }

      try {
        const startRes = await request('/api/passkey/register/start', {
          method: 'POST',
          body: JSON.stringify({ name: passkeyName })
        });
        const { challenge_id, options } = startRes;
        const pk = { ...options.publicKey };
        pk.challenge = base64UrlToBuffer(pk.challenge);
        pk.user = { ...pk.user, id: base64UrlToBuffer(pk.user.id) };
        if (Array.isArray(pk.excludeCredentials) && pk.excludeCredentials.length > 0) {
          pk.excludeCredentials = pk.excludeCredentials.map(c => ({
            ...c,
            id: base64UrlToBuffer(c.id)
          }));
        } else {
          delete pk.excludeCredentials;
        }
        if (pk.authenticatorSelection) {
          pk.authenticatorSelection = {
            ...pk.authenticatorSelection,
            residentKey: 'preferred'
          };
        }

        const cred = await navigator.credentials.create({ publicKey: pk });
        if (!cred) throw new Error('未能建立通行密鑰');

        const credentialPayload = {
          id: cred.id,
          rawId: bufferToBase64Url(cred.rawId),
          response: {
            clientDataJSON: bufferToBase64Url(cred.response.clientDataJSON),
            attestationObject: bufferToBase64Url(cred.response.attestationObject)
          },
          type: cred.type,
          extensions: cred.getClientExtensionResults ? cred.getClientExtensionResults() : {}
        };

        await request('/api/passkey/register/finish', {
          method: 'POST',
          body: JSON.stringify({
            challenge_id,
            name: passkeyName,
            credential: credentialPayload
          })
        });

        toast(t('passkey_added_success'));
        await refreshPasskeys();
      } catch (err) {
        if (err.name !== 'NotAllowedError') {
          toast(err.message || 'Passkey 绑定失败', true);
        }
      } finally {
        if (btn) { btn.disabled = false; btn.textContent = oldText; }
      }
    }

    let conditionalAbortController = null;
    let explicitPasskeyBusy = false;
    let conditionalUiStarted = false;

    async function loginWithPasskey(isConditional = false) {
      if (!isPasskeySupported()) {
        if (!isConditional) toast(t('passkey_insecure'), true);
        return;
      }
      if (isIpAddress(window.location.hostname)) {
        if (!isConditional) toast(t('passkey_ip_warn'), true);
        return;
      }

      const btn = $('#passkey-login-btn');
      const errorBox = $('#login-error');
      if (errorBox && !isConditional) errorBox.textContent = '';

      if (isConditional) {
        if (explicitPasskeyBusy) return;
        if (conditionalAbortController) {
          try { conditionalAbortController.abort(); } catch (_) {}
          conditionalAbortController = null;
        }
        conditionalAbortController = new AbortController();
      } else {
        if (explicitPasskeyBusy) return;
        if (conditionalAbortController) {
          try { conditionalAbortController.abort(); } catch (_) {}
          conditionalAbortController = null;
        }
        explicitPasskeyBusy = true;
        if (btn) btn.disabled = true;
      }

      try {
        const startRes = await request('/api/passkey/login/start', { method: 'POST' });
        const { challenge_id, options } = startRes;
        const pk = { ...options.publicKey };
        pk.challenge = base64UrlToBuffer(pk.challenge);
        if (Array.isArray(pk.allowCredentials) && pk.allowCredentials.length > 0) {
          pk.allowCredentials = pk.allowCredentials.map(c => ({
            ...c,
            id: base64UrlToBuffer(c.id)
          }));
        } else {
          delete pk.allowCredentials;
        }

        const getOptions = { publicKey: pk };
        if (isConditional) {
          getOptions.mediation = 'conditional';
          if (conditionalAbortController) {
            getOptions.signal = conditionalAbortController.signal;
          }
        }

        const cred = await navigator.credentials.get(getOptions);
        if (!cred) return;

        const credentialPayload = {
          id: cred.id,
          rawId: bufferToBase64Url(cred.rawId),
          response: {
            clientDataJSON: bufferToBase64Url(cred.response.clientDataJSON),
            authenticatorData: bufferToBase64Url(cred.response.authenticatorData),
            signature: bufferToBase64Url(cred.response.signature),
            userHandle: cred.response.userHandle ? bufferToBase64Url(cred.response.userHandle) : null
          },
          type: cred.type,
          extensions: cred.getClientExtensionResults ? cred.getClientExtensionResults() : {}
        };

        await request('/api/passkey/login/finish', {
          method: 'POST',
          body: JSON.stringify({
            challenge_id,
            credential: credentialPayload
          })
        });

        bootstrap = await request('/api/bootstrap');
        showState(bootstrap.authenticated ? 'main' : 'login');
      } catch (err) {
        if (err.name !== 'AbortError' && err.name !== 'NotAllowedError') {
          if (!isConditional) {
            if (errorBox) errorBox.textContent = err.message || 'Passkey 登录失败';
            else toast(err.message || 'Passkey 登录失败', true);
          }
        }
      } finally {
        if (isConditional) {
          conditionalAbortController = null;
        } else {
          explicitPasskeyBusy = false;
          if (btn) btn.disabled = false;
        }
      }
    }

    async function refreshPasskeys() {
      const container = $('#passkey-items');
      const emptyHint = $('#passkey-empty-hint');
      const inspectorHint = $('#passkey-inspector-hint');
      const addBtn = $('#passkey-add-btn');

      if (isIpAddress(window.location.hostname)) {
        if (inspectorHint) {
          inspectorHint.textContent = t('passkey_ip_warn');
          inspectorHint.classList.remove('hidden');
        }
        if (addBtn) addBtn.disabled = true;
      } else if (!isPasskeySupported()) {
        if (inspectorHint) {
          inspectorHint.textContent = t('passkey_insecure');
          inspectorHint.classList.remove('hidden');
        }
        if (addBtn) addBtn.disabled = true;
      } else {
        if (inspectorHint) inspectorHint.classList.add('hidden');
        if (addBtn) addBtn.disabled = false;
      }

      try {
        const res = await request('/api/passkey/list');
        const list = res.passkeys || [];
        if (!container) return;
        container.replaceChildren();

        if (list.length === 0) {
          emptyHint?.classList.remove('hidden');
          container?.classList.add('hidden');
        } else {
          emptyHint?.classList.add('hidden');
          container?.classList.remove('hidden');

          list.forEach(pk => {
            const item = document.createElement('div');
            item.className = 'media-item';
            item.style.padding = '8px 10px';
            item.style.marginBottom = '6px';

            const head = document.createElement('div');
            head.className = 'media-item-head';

            const nameBox = document.createElement('span');
            nameBox.className = 'media-item-name';

            const title = document.createElement('strong');
            title.textContent = pk.name || 'Passkey';

            const meta = document.createElement('span');
            meta.className = 'media-item-meta';
            const dateStr = pk.created_at_unix_seconds ? new Date(pk.created_at_unix_seconds * 1000).toLocaleDateString() : '';
            meta.textContent = dateStr;

            nameBox.append(title, meta);
            head.append(nameBox);

            const controls = document.createElement('div');
            controls.className = 'media-item-controls';

            const delBtn = document.createElement('button');
            delBtn.type = 'button';
            delBtn.className = 'danger';
            delBtn.style.minHeight = '28px';
            delBtn.style.padding = '0 8px';
            delBtn.style.fontSize = '12px';
            delBtn.textContent = t('passkey_delete');
            delBtn.addEventListener('click', async () => {
              if (!confirm(t('passkey_delete_confirm'))) return;
              try {
                await request(`/api/passkey/${encodeURIComponent(pk.id)}`, { method: 'DELETE' });
                toast(t('passkey_delete') + ' OK');
                await refreshPasskeys();
              } catch (e) {
                toast(e.message, true);
              }
            });

            controls.append(delBtn);
            item.append(head, controls);
            container.append(item);
          });
        }
      } catch (err) {
        // Silently ignore if not authorized
      }
    }

    function updateLoginPasskeyUI() {
      const container = $('#passkey-login-container');
      const insecureHint = $('#passkey-insecure-hint');
      const hasPasskeys = !!bootstrap.has_passkeys;

      if (isIpAddress(window.location.hostname)) {
        if (insecureHint) {
          insecureHint.textContent = t('passkey_ip_warn');
          insecureHint.classList.remove('hidden');
        }
        if (container) container.classList.add('hidden');
        return;
      }

      if (!isPasskeySupported()) {
        if (hasPasskeys && insecureHint) {
          insecureHint.textContent = t('passkey_insecure');
          insecureHint.classList.remove('hidden');
        }
        if (container) container.classList.add('hidden');
        return;
      }

      if (insecureHint) insecureHint.classList.add('hidden');

      if (hasPasskeys) {
        if (container) container.classList.remove('hidden');

        // Trigger Conditional UI (Autofill) once if available
        if (!conditionalUiStarted && window.PublicKeyCredential && PublicKeyCredential.isConditionalMediationAvailable) {
          PublicKeyCredential.isConditionalMediationAvailable().then(available => {
            if (available && !conditionalUiStarted) {
              conditionalUiStarted = true;
              loginWithPasskey(true);
            }
          }).catch(() => {});
        }
      } else {
        if (container) container.classList.add('hidden');
      }
    }

    $('#passkey-login-btn')?.addEventListener('click', () => loginWithPasskey(false));
    $('#passkey-add-btn')?.addEventListener('click', registerPasskey);

    const settingsDialog = $('#settings-dialog');
    function applyRenderPixelated(enabled) {
      $('#console')?.classList.toggle('render-pixelated', enabled);
      setSetting('render_pixelated', enabled);
    }
    function syncSettingsDialogUi() {
      if (!settingsDialog) return;
      const hidCheckbox = $('#setting-suppress-hid-timeout');
      if (hidCheckbox) hidCheckbox.checked = getSetting('suppress_hid_timeout', true);
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
    }
    function openSettingsDialog() {
      if (!settingsDialog) return;
      syncSettingsDialogUi();
      try {
        if (typeof settingsDialog.showModal === 'function') {
          settingsDialog.showModal();
        } else if (typeof settingsDialog.show === 'function') {
          settingsDialog.show();
        }
      } catch(_) {
        try { settingsDialog.show?.(); } catch(__) {}
      }
    }
    function closeSettingsDialog() {
      if (!settingsDialog) return;
      try { settingsDialog.close?.(); } catch(_) {}
    }
    $('#settings-toggle')?.addEventListener('click', openSettingsDialog);
    $('#settings-inspector-btn')?.addEventListener('click', openSettingsDialog);
    $$('[data-settings-close]', settingsDialog).forEach(btn => btn.addEventListener('click', closeSettingsDialog));
    settingsDialog?.addEventListener('click', event => {
      if (event.target === settingsDialog) closeSettingsDialog();
    });
    $('#setting-suppress-hid-timeout')?.addEventListener('change', e => {
      setSetting('suppress_hid_timeout', e.target.checked);
      toast(t('settings_saved', '设置已保存'));
    });
    $('#setting-capture-pulse')?.addEventListener('change', e => {
      setSetting('capture_pulse', e.target.checked);
      toast(t('settings_saved', '设置已保存'));
    });
    $('#setting-release-toast')?.addEventListener('change', e => {
      setSetting('release_key_toast', e.target.checked);
      toast(t('settings_saved', '设置已保存'));
    });
    $('#setting-render-pixelated')?.addEventListener('change', e => {
      applyRenderPixelated(e.target.checked);
      toast(t('settings_saved', '设置已保存'));
    });
    $('#setting-auto-inspector')?.addEventListener('change', e => {
      setSetting('auto_expand_inspector', e.target.checked);
      toast(t('settings_saved', '设置已保存'));
    });
    $('#setting-proxy-protocol')?.addEventListener('change', async e => {
      const select = e.target;
      const mode = select.value;
      const previousMode = unwrapConfig(bootstrap).server?.proxy_protocol || 'disabled';
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
      } catch (err) {
        select.value = previousMode;
        toast(err.message || '保存失败', true);
      } finally {
        select.disabled = false;
      }
    });
    $('#setting-reset-btn')?.addEventListener('click', () => {
      if (!confirm(t('settings_reset_confirm', '确定要恢复所有全局偏好设置为默认值吗？'))) return;
      Object.keys(SETTING_DEFAULTS).forEach(key => {
        try { localStorage.removeItem(`wingman_setting_${key}`); } catch (_) {}
      });
      syncSettingsDialogUi();
      applyRenderPixelated(SETTING_DEFAULTS.render_pixelated);
      toast(t('settings_reset_done', '已恢复默认设置'));
    });

    applyTheme(getPreferredTheme());
    applyRenderPixelated(getSetting('render_pixelated', true));
    try{const savedLang=localStorage.getItem('wingman_lang');if(savedLang&&['zh-CN','zh-TW','en'].includes(savedLang))setLanguage(savedLang);else setLanguage('zh-CN');}catch(_){setLanguage('zh-CN');}
    consumeSetupToken();
    start();
  })();
