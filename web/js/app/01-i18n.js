  const I18N = {
    'zh-CN': {
      power: '电源',
      power_status: '电源状态',
      short_press: '短按电源开机/关机',
      reset_pc: '复位重启 (Reset)',
      force_off: '强制关机 (长按 5 秒)',
      keyboard: '键盘',
      inspector: '控制面板',
      inspector_close: '收起',
      diag_title: '连接与设备诊断',
      diag_transport: '视频传输链路',
      diag_h264_failure: 'H.264 回退原因',
      diag_resolution: '采集画面分辨率',
      diag_input: '键鼠控制模式',
      diag_power_led: '被控机电源 LED',
      diag_client_ip: '客户端来源地址',
      diag_reconnect: '重连视频',
      diag_scan: '扫描设备',
      tab_remote_screen: '远程画面',
      tab_terminal: '终端',
      release_keys: '释放按键',
      reconnect: '重连',
      clear: '清空',
      mode_fit: '适应',
      pan_mode: '平移',
      fullscreen: '全屏',
      exit_fullscreen: '退出全屏',
      mode_prefix: '画面比例: ',
      tab_control: '控制',
      tab_video: '视频',
      tab_devices: '设备',
      tab_media: '介质',
      remote_control: '远程控制',
      forward_input: '转发键鼠',
      short_press_btn: '短按电源',
      reset_btn: '复位',
      force_off_btn: '强制关机',
      refresh: '刷新',
      preset_desktop: '桌面优化 (1080p 60fps 8M)',
      preset_bios: 'BIOS / UEFI (720p 30fps MJPEG)',
      preset_bandwidth: '低带宽省流 (720p 25fps 2M)',
      apply: '应用',
      rescan: '重新检测',
      save: '保存',
      upload: '上传',
      logout: '退出登录',
      virtual_keyboard: '虚拟键盘',
      close: '收起',
      input_paused: '输入已暂停',
      input_captured: '相对鼠标已捕获',
      input_click_to_capture: '点击画面捕获鼠标',
      input_forwarding: '键鼠正在转发',
      input_released: '已释放所有按键与鼠标锁定',
      theme_dark: '深色模式',
      theme_light: '浅色模式',
      theme_system: '跟随系统',
      switch_theme: '切换外观主题',
      connecting: '正在连接…',
      video_connecting: '正在连接视频…',
      video_paused: '视频已暂停',
      device_offline: '设备离线',
      collapse_inspector: '收起面板',
      open_inspector: '控制面板',
      lang_name: '简',
      passkey_title: 'Passkey 通行密钥',
      passkey_desc: '绑定 Apple Touch ID / Face ID，无需密码快速登录',
      passkey_add: '+ 绑定此设备',
      passkey_signin: '使用 Passkey (Touch ID / Face ID) 登录',
      passkey_none: '尚未绑定任何 Passkey',
      passkey_delete: '删除',
      passkey_prompt_name: '为新 Passkey 命名（例如：MacBook Pro、iPhone）：',
      passkey_insecure: 'Passkey 需要在安全连接（HTTPS 或 localhost）下使用',
      passkey_ip_warn: 'WebAuthn 规范要求使用域名（如 localhost 或 wingman.local），不支持 IP 地址',
      passkey_added_success: 'Passkey 绑定成功！',
      passkey_delete_confirm: '确定要删除此 Passkey 吗？',
      settings: '设置',
      settings_title: '全局偏好设置',
      settings_desc: '配置交互反馈、通知拦截与全局渲染偏好',
      settings_group_input: '交互与通知',
      logout_failed: '退出失败，会话仍然有效，请重试',
      input_release_failed: '释放按键失败，被控机可能仍有按键处于按下状态',
      settings_suppress_hid: '屏蔽 HID 写入超时警告',
      settings_suppress_hid_desc:
        '被控端未开机等情况下，键鼠写入超时（504）不再弹出提示；设备未就绪等其他错误仍会提示',
      settings_capture_pulse: '接管边缘微光脉冲',
      settings_capture_pulse_desc: '激活键鼠转发时，在远端画面边缘显示光晕视觉反馈',
      settings_release_toast: '按键释放提示',
      settings_release_toast_desc: '执行「释放按键」或切换控制权时弹出状态提示',
      settings_group_display: '画面与渲染',
      settings_render_pixelated: '像素锐利渲染 (Pixelated)',
      settings_render_pixelated_desc: '近邻采样字符边缘无模糊滤镜，适合 BIOS 与纯文本终端',
      settings_auto_inspector: '宽屏默认展开面板',
      settings_auto_inspector_desc: '屏幕宽度充足时，进入控制台默认展开右侧控制面板',
      settings_group_account: '账户与安全',
      sessions_title: '登录会话',
      sessions_desc: '已登录的设备；下线会立即断开该设备上的视频、终端与键鼠连接',
      sessions_revoke_others: '让其他设备全部下线',
      sessions_loading: '正在读取登录会话…',
      sessions_empty: '当前没有登录会话',
      sessions_unknown_device: '未知设备',
      sessions_unknown_ip: '未知地址',
      sessions_current: '当前会话',
      sessions_revoke: '下线',
      sessions_logout: '退出登录',
      sessions_revoke_confirm: '确定要让该设备下线吗？该设备上的连接会立即断开。',
      sessions_logout_confirm: '退出登录会结束当前会话，确定继续吗？',
      sessions_revoked: '已下线',
      sessions_revoke_failed: '下线失败',
      sessions_error: '无法读取登录会话',
      sessions_revoke_others_confirm: '确定要让其他所有设备下线吗？其他设备上的连接会立即断开。',
      sessions_revoke_others_failed: '其他设备下线失败',
      session_method_password: '密码登录',
      session_method_passkey: 'Passkey 登录',
      session_method_setup: '初始化登录',
      session_method_unknown: '未知方式',
      audit_title: '审计日志',
      audit_desc: '最近 50 条安全相关操作，仅保存在设备本机',
      audit_loading: '正在读取审计日志…',
      audit_empty: '暂无审计记录',
      audit_error: '无法读取审计日志',
      audit_event_login_succeeded: '登录成功',
      audit_event_login_failed: '登录失败',
      audit_event_logout: '退出登录',
      audit_event_session_revoked: '会话下线',
      audit_event_sessions_revoked_others: '其他设备下线',
      audit_event_power_action: '电源操作',
      audit_event_gpio_test: 'GPIO 测试',
      audit_event_config_changed: '配置变更',
      audit_event_passkey_added: '添加 Passkey',
      audit_event_passkey_removed: '删除 Passkey',
      audit_event_setup_completed: '初始化完成',
      settings_group_network: '服务端网络（立即对所有连接生效）',
      ppv2_confirm_required: '“强制要求”会立即拒绝所有未经过 PPv2 代理的连接，包括你当前的直连访问。确认网页只通过已配置 PPv2 的反向代理访问后再继续。',
      ppv2_confirm_auto: '“自动检测”会信任连接开头的 PPv2 头。只应在监听端口仅对可信代理开放时启用。继续？',
      ppv2_probe_failed: '新连接已无法访问本服务。请通过已配置 PPv2 的代理访问，或在设备上把 config.json 的 server.proxy_protocol 改回 disabled 后重启服务。',
      settings_ppv2: 'PROXY Protocol v2 (PPv2)',
      settings_ppv2_desc: '解析并透传反向代理 (HAProxy / Nginx) 转发的真实客户端 IP',
      proxy_disabled: '关闭',
      proxy_auto: '自动检测',
      proxy_required: '强制要求',
      settings_group_system: '系统与重置',
      settings_reset_btn: '恢复默认设置',
      settings_reset_desc: '清除所有保存在本机的偏好设置并还原为初始默认值',
      settings_reset_action: '还原',
      settings_reset_confirm: '确定要恢复所有全局偏好设置为默认值吗？',
      settings_reset_done: '已恢复默认设置',
      settings_saved: '设置已保存',
      keyboard_leds_title: '键盘锁定状态',
      keyboard_leds_desc: '远程主机键盘锁定指示灯（点击快速切换）',
      host_performance: '主控机性能',
      cpu_usage: 'CPU',
      cpu_cores: '核',
      load: '负载',
      load_average: '系统负载',
      memory_usage: '内存',
      mem_short: '内存',
      disk_usage: '存储',
      temperature: '主机温度',
      uptime: '运行时间',
      full_diagnostics: '完整诊断 ↗',
      perf_hud_title: '主控机性能 (点击查看详情)',
      led_num: '数字锁定 (Num Lock)',
      led_caps: '大写锁定 (Caps Lock)',
      led_scroll: '滚动锁定 (Scroll Lock)',
    },
    'zh-TW': {
      power: '電源',
      power_status: '電源狀態',
      short_press: '短按電源開機/關機',
      reset_pc: '重置重啟 (Reset)',
      force_off: '強制關機 (長按 5 秒)',
      keyboard: '鍵盤',
      inspector: '控制面板',
      inspector_close: '收起',
      diag_title: '連線與設備診斷',
      diag_transport: '視訊傳輸鏈路',
      diag_h264_failure: 'H.264 回退原因',
      diag_resolution: '採集畫面解析度',
      diag_input: '鍵鼠控制模式',
      diag_power_led: '被控機電源 LED',
      diag_client_ip: '客戶端來源位址',
      diag_reconnect: '重新連線視訊',
      diag_scan: '掃描設備',
      tab_remote_screen: '遠端畫面',
      tab_terminal: '終端機',
      release_keys: '釋放按鍵',
      reconnect: '重新連線',
      clear: '清除',
      mode_fit: '適應',
      pan_mode: '平移',
      fullscreen: '全螢幕',
      exit_fullscreen: '退出全螢幕',
      mode_prefix: '畫面比例: ',
      tab_control: '控制',
      tab_video: '視訊',
      tab_devices: '設備',
      tab_media: '媒體',
      remote_control: '遠端控制',
      forward_input: '轉發鍵鼠',
      short_press_btn: '短按電源',
      reset_btn: '重置',
      force_off_btn: '強制關機',
      refresh: '重新整理',
      preset_desktop: '桌面最佳化 (1080p 60fps 8M)',
      preset_bios: 'BIOS / UEFI (720p 30fps MJPEG)',
      preset_bandwidth: '低頻寬省流 (720p 25fps 2M)',
      apply: '套用',
      rescan: '重新檢測',
      save: '儲存',
      upload: '上傳',
      logout: '登出',
      virtual_keyboard: '虛擬鍵盤',
      close: '收起',
      input_paused: '輸入已暫停',
      input_captured: '相對滑鼠已捕獲',
      input_click_to_capture: '點擊畫面捕獲滑鼠',
      input_forwarding: '鍵鼠正在轉發',
      input_released: '已釋放所有按鍵與滑鼠鎖定',
      theme_dark: '深色模式',
      theme_light: '淺色模式',
      theme_system: '跟隨系統',
      switch_theme: '切換外觀主題',
      connecting: '正在連線…',
      video_connecting: '正在連線視訊…',
      video_paused: '視訊已暫停',
      device_offline: '設備離線',
      collapse_inspector: '收起面板',
      open_inspector: '控制面板',
      lang_name: '繁',
      passkey_title: 'Passkey 通行密鑰',
      passkey_desc: '綁定 Apple Touch ID / Face ID，無需密碼快速登入',
      passkey_add: '+ 綁定此設備',
      passkey_signin: '使用 Passkey (Touch ID / Face ID) 登入',
      passkey_none: '尚未綁定任何 Passkey',
      passkey_delete: '刪除',
      passkey_prompt_name: '為新 Passkey 命名（例如：MacBook Pro、iPhone）：',
      passkey_insecure: 'Passkey 需要在安全連線（HTTPS 或 localhost）下使用',
      passkey_ip_warn: 'WebAuthn 規範要求使用網域名稱（如 localhost 或 wingman.local），不支援 IP 位址',
      passkey_added_success: 'Passkey 綁定成功！',
      passkey_delete_confirm: '確定要刪除此 Passkey 嗎？',
      settings: '設定',
      settings_title: '全局偏好設定',
      settings_desc: '配置交互反饋、通知攔截與全局渲染偏好',
      settings_group_input: '交互與通知',
      logout_failed: '登出失敗，工作階段仍然有效，請重試',
      input_release_failed: '釋放按鍵失敗，被控機可能仍有按鍵處於按下狀態',
      settings_suppress_hid: '屏蔽 HID 寫入逾時警告',
      settings_suppress_hid_desc:
        '被控端未開機等情況下，鍵鼠寫入逾時（504）不再彈出提示；裝置未就緒等其他錯誤仍會提示',
      settings_capture_pulse: '接管邊緣微光脈衝',
      settings_capture_pulse_desc: '激活鍵鼠轉發時，在遠端畫面邊緣顯示光暈視覺反饋',
      settings_release_toast: '按鍵釋放提示',
      settings_release_toast_desc: '執行「釋放按鍵」或切換控制權時彈出狀態提示',
      settings_group_display: '畫面與渲染',
      settings_render_pixelated: '像素銳利渲染 (Pixelated)',
      settings_render_pixelated_desc: '近鄰採樣字符邊緣無模糊濾鏡，適合 BIOS 與純文本終端',
      settings_auto_inspector: '寬螢幕預設展開面板',
      settings_auto_inspector_desc: '螢幕寬度充足時，進入控制台預設展開右側控制面板',
      settings_group_account: '帳戶與安全',
      sessions_title: '登入工作階段',
      sessions_desc: '已登入的裝置；離線會立即中斷該裝置上的視訊、終端與鍵鼠連線',
      sessions_revoke_others: '讓其他裝置全部離線',
      sessions_loading: '正在讀取登入工作階段…',
      sessions_empty: '目前沒有登入工作階段',
      sessions_unknown_device: '未知裝置',
      sessions_unknown_ip: '未知位址',
      sessions_current: '目前工作階段',
      sessions_revoke: '離線',
      sessions_logout: '登出',
      sessions_revoke_confirm: '確定要讓該裝置離線嗎？該裝置上的連線會立即中斷。',
      sessions_logout_confirm: '登出會結束目前工作階段，確定繼續嗎？',
      sessions_revoked: '已離線',
      sessions_revoke_failed: '離線失敗',
      sessions_error: '無法讀取登入工作階段',
      sessions_revoke_others_confirm: '確定要讓其他所有裝置離線嗎？其他裝置上的連線會立即中斷。',
      sessions_revoke_others_failed: '其他裝置離線失敗',
      session_method_password: '密碼登入',
      session_method_passkey: 'Passkey 登入',
      session_method_setup: '初始化登入',
      session_method_unknown: '未知方式',
      audit_title: '稽核日誌',
      audit_desc: '最近 50 筆安全相關操作，僅儲存在裝置本機',
      audit_loading: '正在讀取稽核日誌…',
      audit_empty: '暫無稽核記錄',
      audit_error: '無法讀取稽核日誌',
      audit_event_login_succeeded: '登入成功',
      audit_event_login_failed: '登入失敗',
      audit_event_logout: '登出',
      audit_event_session_revoked: '工作階段離線',
      audit_event_sessions_revoked_others: '其他裝置離線',
      audit_event_power_action: '電源操作',
      audit_event_gpio_test: 'GPIO 測試',
      audit_event_config_changed: '設定變更',
      audit_event_passkey_added: '新增 Passkey',
      audit_event_passkey_removed: '刪除 Passkey',
      audit_event_setup_completed: '初始化完成',
      settings_group_network: '伺服器網路（立即對所有連線生效）',
      ppv2_confirm_required: '「強制要求」會立即拒絕所有未經過 PPv2 代理的連線，包括你目前的直連存取。確認網頁只透過已設定 PPv2 的反向代理存取後再繼續。',
      ppv2_confirm_auto: '「自動偵測」會信任連線開頭的 PPv2 標頭。只應在監聽埠僅對可信代理開放時啟用。繼續？',
      ppv2_probe_failed: '新連線已無法存取本服務。請透過已設定 PPv2 的代理存取，或在裝置上把 config.json 的 server.proxy_protocol 改回 disabled 後重新啟動服務。',
      settings_ppv2: 'PROXY Protocol v2 (PPv2)',
      settings_ppv2_desc: '解析並透傳反向代理 (HAProxy / Nginx) 轉發的真實客戶端 IP',
      proxy_disabled: '關閉',
      proxy_auto: '自動檢測',
      proxy_required: '強制要求',
      settings_group_system: '系統與重設',
      settings_reset_btn: '恢復預設設定',
      settings_reset_desc: '清除所有保存在本機的偏好設定並還原為初始預設值',
      settings_reset_action: '還原',
      settings_reset_confirm: '確定要恢復所有全局偏好設定為預設值嗎？',
      settings_reset_done: '已恢復預設設定',
      settings_saved: '設定已儲存',
      keyboard_leds_title: '鍵盤鎖定狀態',
      keyboard_leds_desc: '遠端主機鍵盤鎖定指示燈（點擊快速切換）',
      host_performance: '主控機效能',
      cpu_usage: 'CPU',
      cpu_cores: '核',
      load: '負載',
      load_average: '系統負載',
      memory_usage: '記憶體',
      mem_short: '記憶體',
      disk_usage: '儲存',
      temperature: '主機溫度',
      uptime: '運行時間',
      full_diagnostics: '完整診斷 ↗',
      perf_hud_title: '主控機效能 (點擊查看詳情)',
      led_num: '數字鎖定 (Num Lock)',
      led_caps: '大寫鎖定 (Caps Lock)',
      led_scroll: '捲動鎖定 (Scroll Lock)',
    },
    en: {
      power: 'Power',
      power_status: 'Power Status',
      short_press: 'Power On / Off (Short Press)',
      reset_pc: 'System Reset (Reset)',
      force_off: 'Force Power Off (Hold 5s)',
      keyboard: 'Keyboard',
      inspector: 'Inspector',
      inspector_close: 'Collapse',
      diag_title: 'Diagnostics',
      diag_transport: 'Video Transport',
      diag_h264_failure: 'H.264 Fallback Reason',
      diag_resolution: 'Capture Resolution',
      diag_input: 'Input Mode',
      diag_power_led: 'Target Power LED',
      diag_client_ip: 'Client Address',
      diag_reconnect: 'Reconnect Video',
      diag_scan: 'Scan Devices',
      tab_remote_screen: 'Remote Screen',
      tab_terminal: 'Terminal',
      release_keys: 'Release Keys',
      reconnect: 'Reconnect',
      clear: 'Clear',
      mode_fit: 'Fit',
      pan_mode: 'Pan',
      fullscreen: 'Fullscreen',
      exit_fullscreen: 'Exit Fullscreen',
      mode_prefix: 'Display Mode: ',
      tab_control: 'Control',
      tab_video: 'Video',
      tab_devices: 'Devices',
      tab_media: 'Media',
      remote_control: 'Remote Control',
      forward_input: 'Forward Input',
      short_press_btn: 'Power',
      reset_btn: 'Reset',
      force_off_btn: 'Force Off',
      refresh: 'Refresh',
      preset_desktop: 'Desktop (1080p 60fps 8M)',
      preset_bios: 'BIOS / UEFI (720p 30fps MJPEG)',
      preset_bandwidth: 'Low Bandwidth (720p 25fps 2M)',
      apply: 'Apply',
      rescan: 'Rescan',
      save: 'Save',
      upload: 'Upload',
      logout: 'Log Out',
      virtual_keyboard: 'Virtual Keyboard',
      close: 'Close',
      input_paused: 'Input Paused',
      input_captured: 'Relative Mouse Captured',
      input_click_to_capture: 'Click to Capture Mouse',
      input_forwarding: 'Input Active',
      input_released: 'All keys and pointer locks released',
      theme_dark: 'Dark Mode',
      theme_light: 'Light Mode',
      theme_system: 'System Theme',
      switch_theme: 'Toggle Theme',
      connecting: 'Connecting…',
      video_connecting: 'Connecting video…',
      video_paused: 'Video paused',
      device_offline: 'Device offline',
      collapse_inspector: 'Collapse',
      open_inspector: 'Inspector',
      lang_name: 'EN',
      passkey_title: 'Passkey Credentials',
      passkey_desc: 'Sign in with Apple Touch ID / Face ID or Windows Hello',
      passkey_add: '+ Add This Device',
      passkey_signin: 'Sign in with Passkey',
      passkey_none: 'No Passkeys registered yet',
      passkey_delete: 'Delete',
      passkey_prompt_name: 'Enter a name for this Passkey (e.g. MacBook Pro, iPhone):',
      passkey_insecure: 'Passkey requires a secure context (HTTPS or localhost)',
      passkey_ip_warn:
        'WebAuthn requires a domain name (e.g. localhost or wingman.local), IP addresses not supported',
      passkey_added_success: 'Passkey registered successfully!',
      passkey_delete_confirm: 'Are you sure you want to delete this Passkey?',
      settings: 'Settings',
      settings_title: 'Global Preferences',
      settings_desc: 'Configure interaction feedback, alert silencing, and rendering preferences',
      settings_group_input: 'Interaction & Alerts',
      logout_failed: 'Log out failed; the session is still active. Please retry',
      input_release_failed: 'Releasing keys failed; keys may still be held on the target',
      settings_suppress_hid: 'Silence HID Timeout Warnings',
      settings_suppress_hid_desc:
        'Hide HID write timeout (504) popups, e.g. when the target is powered off; other errors such as "device not ready" are still shown',
      settings_capture_pulse: 'Input Capture Edge Glow',
      settings_capture_pulse_desc:
        'Display a subtle blue edge glow feedback when activating remote keyboard/mouse capture',
      settings_release_toast: 'Key Release Notification',
      settings_release_toast_desc:
        'Show toast notification when releasing held keys or toggling remote input',
      settings_group_display: 'Display & Rendering',
      settings_render_pixelated: 'Pixelated Sharp Scaling',
      settings_render_pixelated_desc:
        'Disable bilinear blur filtering for crisp characters in BIOS and text terminals',
      settings_auto_inspector: 'Auto-expand Inspector on Wide Screens',
      settings_auto_inspector_desc: 'Automatically keep the right-hand Inspector open on large screens',
      settings_group_account: 'Account & security',
      sessions_title: 'Sign-in sessions',
      sessions_desc: 'Signed-in devices. Revoking immediately disconnects that device’s video, terminal and input.',
      sessions_revoke_others: 'Sign out all other devices',
      sessions_loading: 'Loading sign-in sessions…',
      sessions_empty: 'No sign-in sessions',
      sessions_unknown_device: 'Unknown device',
      sessions_unknown_ip: 'Unknown address',
      sessions_current: 'Current session',
      sessions_revoke: 'Revoke',
      sessions_logout: 'Log Out',
      sessions_revoke_confirm: 'Revoke this device? Its connections will be disconnected immediately.',
      sessions_logout_confirm: 'Logging out ends the current session. Continue?',
      sessions_revoked: 'Session revoked',
      sessions_revoke_failed: 'Could not revoke the session',
      sessions_error: 'Could not load sign-in sessions',
      sessions_revoke_others_confirm: 'Sign out every other device? Their connections will be disconnected immediately.',
      sessions_revoke_others_failed: 'Could not sign out other devices',
      session_method_password: 'Password',
      session_method_passkey: 'Passkey',
      session_method_setup: 'Setup',
      session_method_unknown: 'Unknown method',
      audit_title: 'Audit log',
      audit_desc: 'The 50 most recent security-relevant actions, stored only on this device',
      audit_loading: 'Loading audit log…',
      audit_empty: 'No audit entries',
      audit_error: 'Could not load the audit log',
      audit_event_login_succeeded: 'Sign-in succeeded',
      audit_event_login_failed: 'Sign-in failed',
      audit_event_logout: 'Logged out',
      audit_event_session_revoked: 'Session revoked',
      audit_event_sessions_revoked_others: 'Other sessions revoked',
      audit_event_power_action: 'Power action',
      audit_event_gpio_test: 'GPIO test',
      audit_event_config_changed: 'Configuration changed',
      audit_event_passkey_added: 'Passkey added',
      audit_event_passkey_removed: 'Passkey removed',
      audit_event_setup_completed: 'Setup completed',
      settings_group_network: 'Server network (applies to all connections immediately)',
      ppv2_confirm_required: '"Required" immediately rejects every connection that does not come through a PPv2 proxy, including your current direct session. Continue only if this page is reached exclusively through a PPv2-enabled reverse proxy.',
      ppv2_confirm_auto: '"Auto" trusts a PPv2 header at the start of any connection. Enable it only when the listening port is reachable solely by trusted proxies. Continue?',
      ppv2_probe_failed: 'New connections can no longer reach this service. Access it through the PPv2 proxy, or set server.proxy_protocol back to disabled in config.json on the device and restart the service.',
      settings_ppv2: 'PROXY Protocol v2 (PPv2)',
      settings_ppv2_desc: 'Parse and forward real client IPs from reverse proxies (HAProxy / NGINX)',
      proxy_disabled: 'Disabled',
      proxy_auto: 'Auto Detect',
      proxy_required: 'Required',
      settings_group_system: 'System & Reset',
      settings_reset_btn: 'Reset to Defaults',
      settings_reset_desc: 'Clear all locally stored preferences and restore initial default values',
      settings_reset_action: 'Reset',
      settings_reset_confirm: 'Are you sure you want to reset all preferences to defaults?',
      settings_reset_done: 'Preferences reset to defaults',
      settings_saved: 'Settings saved',
      keyboard_leds_title: 'Keyboard Lock LEDs',
      keyboard_leds_desc: 'Remote host lock indicators (click to toggle)',
      host_performance: 'Host Performance',
      cpu_usage: 'CPU Usage',
      cpu_cores: 'Cores',
      load: 'Load',
      load_average: 'Load Average',
      memory_usage: 'Memory',
      mem_short: 'RAM',
      disk_usage: 'Storage',
      temperature: 'Temperature',
      uptime: 'Uptime',
      full_diagnostics: 'Full Diagnostics ↗',
      perf_hud_title: 'Host Performance (Click to view details)',
      led_num: 'Num Lock',
      led_caps: 'Caps Lock',
      led_scroll: 'Scroll Lock',
    },
  };
  let currentLang = 'zh-CN';
  function t(key, fallback = '') {
    return I18N[currentLang]?.[key] ?? I18N['zh-CN']?.[key] ?? fallback ?? key;
  }
  // Phrase translation for everything without a data-i18n key: static markup
  // and text that scripts write at runtime. Every text node and attribute
  // remembers its source (Simplified Chinese) text and the value last written
  // here; when script replaces the value, that becomes the new source. A
  // MutationObserver translates runtime writes as they happen.
  const TRANSLATED_ATTRS = ['title', 'aria-label', 'placeholder', 'data-help'];
  const CJK = /[\u4e00-\u9fff]/;
  const phraseSources = new WeakMap();
  // Keys containing `${}` are templates: each placeholder matches any text,
  // which is carried into the translation in order.
  const phrasePatterns = {};
  function patternsFor(lang) {
    if (!phrasePatterns[lang]) {
      const escape = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      phrasePatterns[lang] = Object.entries(PHRASES[lang] || {})
        .filter(([source]) => source.includes('${}'))
        // Longest (most specific) template first, so a short template cannot
        // swallow a longer sentence through its last placeholder.
        .sort(([a], [b]) => b.length - a.length)
        .map(([source, target]) => [
          new RegExp(`^${source.split('${}').map(escape).join('(.+?)')}$`),
          target,
        ]);
    }
    return phrasePatterns[lang];
  }
  // Translate a Simplified Chinese source string for the current language.
  function tr(source) {
    if (currentLang === 'zh-CN') return source;
    const exact = PHRASES[currentLang]?.[source];
    if (exact) return exact;
    for (const [pattern, target] of patternsFor(currentLang)) {
      const match = source.match(pattern);
      if (match) {
        let index = 1;
        // Placeholder values are often phrases themselves (e.g. a mode name).
        return target.replace(/\$\{\}/g, () => tr(match[index++] ?? ''));
      }
    }
    return source;
  }
  // User data (file names, device paths, account/host names, diagnostics) is
  // never translated even when it happens to equal a known phrase.
  const NO_TRANSLATE =
    'script,style,pre,code,#terminal-host,[data-no-i18n],.media-item-name,#session-user,#session-ip,#diag-client-ip';
  function untranslatable(el) {
    return !el || !!el.closest(`${NO_TRANSLATE},[data-i18n]`);
  }
  function translatePhrase(holder, key, current, write) {
    const records = phraseSources.get(holder) || new Map();
    let record = records.get(key);
    if (!record || record.applied !== current) record = { raw: current, applied: current };
    const source = record.raw.trim();
    const next = source ? record.raw.replace(source, tr(source)) : record.raw;
    if (next !== current) write(next);
    record.applied = next;
    records.set(key, record);
    phraseSources.set(holder, records);
  }
  function translateTextNode(node) {
    if (untranslatable(node.parentElement)) return;
    const known = phraseSources.get(node)?.get('text');
    if (!known && !CJK.test(node.nodeValue)) return;
    translatePhrase(node, 'text', node.nodeValue, (next) => (node.nodeValue = next));
  }
  function translateAttributes(el) {
    // data-i18n keys only own an element's text; its attributes still go
    // through phrase translation.
    if (!el || el.closest('script,style,[data-no-i18n],.media-item-name') || el.parentElement?.closest('#terminal-host'))
      return;
    for (const attr of TRANSLATED_ATTRS) {
      const value = el.getAttribute(attr);
      if (value === null) continue;
      if (!phraseSources.get(el)?.get(attr) && !CJK.test(value)) continue;
      translatePhrase(el, attr, value, (next) => el.setAttribute(attr, next));
    }
  }
  function translateTree(root) {
    if (root.nodeType === Node.TEXT_NODE) return translateTextNode(root);
    if (root.nodeType !== Node.ELEMENT_NODE) return;
    translateAttributes(root);
    root.querySelectorAll('*').forEach(translateAttributes);
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) translateTextNode(node);
  }
  const phraseObserver = new MutationObserver((mutations) => {
    for (const m of mutations) {
      if (m.type === 'characterData') translateTextNode(m.target);
      else if (m.type === 'attributes') translateAttributes(m.target);
      else m.addedNodes.forEach(translateTree);
    }
  });
  function collectStaticPhrases() {
    translateTree(document.body);
    phraseObserver.observe(document.body, {
      subtree: true,
      childList: true,
      characterData: true,
      attributes: true,
      attributeFilter: TRANSLATED_ATTRS,
    });
  }
  function translateStaticPhrases() {
    translateTree(document.body);
  }
  function updateI18nElements() {
    translateStaticPhrases();
    $$('[data-i18n]').forEach((el) => {
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
    const numTitle = t('led_num', 'Num Lock'),
      capsTitle = t('led_caps', 'Caps Lock'),
      scrlTitle = t('led_scroll', 'Scroll Lock');
    $('#led-num')?.setAttribute('title', numTitle);
    $('#led-caps')?.setAttribute('title', capsTitle);
    $('#led-scroll')?.setAttribute('title', scrlTitle);
    $('#topbar-perf-btn')?.setAttribute('title', t('perf_hud_title', '主控机性能 (点击查看详情)'));
    const langLabel = $('#lang-current-label');
    if (langLabel) langLabel.textContent = I18N[currentLang]?.lang_name || '简';
    $$('[data-lang]').forEach((btn) => btn.classList.toggle('active', btn.dataset.lang === currentLang));
  }
  function setLanguage(lang) {
    currentLang = ['zh-CN', 'zh-TW', 'en'].includes(lang) ? lang : 'zh-CN';
    try {
      localStorage.setItem('wingman_lang', currentLang);
    } catch (_) {}
    document.documentElement.lang = currentLang;
    updateI18nElements();
    applyTheme(getPreferredTheme());
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
      refreshSessions();
      refreshAudit();
    } else if (!loginForm.classList.contains('hidden')) {
      updateLoginPasskeyUI();
    }
  }
