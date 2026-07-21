pub static INDEX_HTML: &str = r##"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover">
  <meta name="color-scheme" content="dark">
  <meta http-equiv="Cache-Control" content="no-store, max-age=0">
  <meta http-equiv="Pragma" content="no-cache">
  <link rel="icon" href="data:,">
  <title>WingmanKVM</title>
  <style>
    :root{font-family:Inter,"PingFang SC","Microsoft YaHei",system-ui,sans-serif;color:#e9eef7;background:#0b0e13;line-height:1.45;--panel:#131821;--panel2:#181e29;--line:#293140;--muted:#94a0b2;--accent:#77a7ff;--ok:#66d49a;--danger:#ff7b86}
    *{box-sizing:border-box}body{margin:0;min-width:320px;min-height:100vh;background:radial-gradient(circle at 50% -20%,#1b2940 0,transparent 42%),#0b0e13}button,input,select{font:inherit}button,input,select{color:inherit;background:#111720;border:1px solid #303a4b;border-radius:8px}button{padding:.56rem .8rem;cursor:pointer}button:hover{border-color:#668aca;background:#192335}button:disabled{opacity:.5;cursor:not-allowed}.primary{background:#3975dc;border-color:#4b86ee}.danger{color:#ffdce0;border-color:#6b343d}.ghost{background:transparent}.hidden{display:none!important}.muted{color:var(--muted)}.error{color:#ffadb4;min-height:1.4em}.ok{color:var(--ok)}
    #auth-view{min-height:100vh;display:grid;place-items:center;padding:24px}.auth-card{width:min(680px,100%);padding:28px;background:rgba(19,24,33,.96);border:1px solid var(--line);border-radius:16px;box-shadow:0 20px 60px #0008}.brand{display:flex;align-items:center;gap:12px}.mark{display:grid;place-items:center;width:38px;height:38px;border-radius:10px;background:#3575e5;font-weight:800}.brand h1{font-size:1.15rem;margin:0}.brand p{margin:2px 0 0;font-size:.82rem;color:var(--muted)}h2{font-size:1.15rem;margin:24px 0 8px}.grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:12px}.field{display:grid;gap:5px;font-size:.82rem;color:#bac4d4}.field input,.field select{width:100%;padding:.65rem}.wide{grid-column:1/-1}.check{display:flex;align-items:center;gap:8px;color:#c7d0dd;font-size:.85rem}.check input{accent-color:#4f89ee}.form-actions{display:flex;align-items:center;gap:12px;margin-top:18px}.hint{font-size:.8rem;color:var(--muted)}
    #app{min-height:100vh;display:grid;grid-template-rows:auto 1fr}.topbar{position:sticky;top:0;z-index:30;display:flex;align-items:center;gap:8px;padding:8px 12px;background:#10151deF;border-bottom:1px solid var(--line);backdrop-filter:blur(12px)}.topbar .brand{margin-right:8px}.key-strip{display:flex;gap:4px;overflow-x:auto;scrollbar-width:none}.key-strip button{padding:.38rem .55rem;min-width:38px;font-size:.78rem;white-space:nowrap}.spacer{flex:1}.status-dot{width:8px;height:8px;border-radius:50%;background:#697386}.status-dot.online{background:var(--ok);box-shadow:0 0 10px #66d49a99}.icon-button{white-space:nowrap}
    .shell{display:grid;grid-template-columns:minmax(0,1fr) 310px;min-height:0}.workspace{position:relative;overflow:hidden;min-height:calc(100vh - 57px);background-color:#080a0e;background-image:linear-gradient(#ffffff08 1px,transparent 1px),linear-gradient(90deg,#ffffff08 1px,transparent 1px);background-size:24px 24px}.console{position:absolute;left:24px;top:24px;width:min(960px,calc(100% - 48px));min-width:320px;min-height:230px;resize:both;overflow:hidden;background:#050608;border:1px solid #354054;border-radius:12px;box-shadow:0 20px 55px #000b}.console-head{height:38px;display:flex;align-items:center;padding:0 10px;background:#171c25;border-bottom:1px solid #2a3240;cursor:move;user-select:none}.console-head strong{font-size:.82rem}.console-head span{margin-left:auto;font-size:.74rem;color:var(--muted)}#video-viewport{position:absolute;inset:38px 0 0;overflow:hidden;display:grid;place-items:center;outline:none;touch-action:none;cursor:default}#video-viewport.remote{cursor:crosshair;box-shadow:inset 0 0 0 2px #4f89ee}#video-feed{display:block;max-width:100%;max-height:100%;width:100%;height:100%;object-fit:contain;user-select:none;-webkit-user-drag:none}.mode-native #video-feed{width:auto;height:auto;max-width:none;max-height:none}.mode-fill #video-feed{object-fit:fill}.video-message{position:absolute;padding:8px 11px;border-radius:8px;background:#080b10d9;color:#aab5c5;font-size:.8rem;pointer-events:none}
    aside{overflow:auto;max-height:calc(100vh - 57px);padding:12px;background:#10151d;border-left:1px solid var(--line)}details{margin-bottom:10px;background:var(--panel);border:1px solid var(--line);border-radius:10px}summary{padding:11px 12px;cursor:pointer;font-weight:650;font-size:.88rem}details>div{padding:0 12px 12px}.side-grid{display:grid;grid-template-columns:1fr 1fr;gap:9px}.side-grid .field{font-size:.75rem}.side-actions{display:flex;gap:8px;align-items:center;margin-top:11px;flex-wrap:wrap}.device-results,.media-list{margin-top:10px;padding:8px;max-height:150px;overflow:auto;white-space:pre-wrap;background:#0c1016;border:1px solid #252d3a;border-radius:7px;font:11px/1.45 ui-monospace,monospace;color:#aeb9c9}.power-row{display:grid;grid-template-columns:1fr 1fr;gap:8px}.power-row form,.power-row button{width:100%}.upload-progress{height:4px;margin-top:8px;background:#252c38;border-radius:2px;overflow:hidden}.upload-progress i{display:block;width:0;height:100%;background:#5d91ec}.session-row{display:flex;justify-content:space-between;align-items:center;font-size:.8rem;color:var(--muted)}
    dialog{color:inherit;background:#151a23;border:1px solid #394457;border-radius:14px;box-shadow:0 25px 80px #000c;padding:0;width:min(760px,calc(100% - 24px))}dialog::backdrop{background:#05070bc9;backdrop-filter:blur(3px)}.dialog-head{display:flex;align-items:center;padding:13px 15px;border-bottom:1px solid var(--line)}.dialog-head h3{margin:0;font-size:1rem}.dialog-head button{margin-left:auto}.keyboard{padding:14px;display:grid;gap:7px}.keyboard-row{display:flex;justify-content:center;gap:6px}.keyboard button{min-width:42px;height:42px;padding:5px}.keyboard .grow{min-width:180px}.keyboard .active{background:#315f9f}
    #toast{position:fixed;left:50%;bottom:24px;z-index:100;translate:-50% 16px;padding:9px 14px;border:1px solid #3a4659;border-radius:9px;background:#161d28;opacity:0;pointer-events:none;transition:.2s}#toast.show{opacity:1;translate:-50% 0}
    @media(max-width:900px){.shell{grid-template-columns:1fr}.workspace{min-height:62vh}.console{left:12px;top:12px;width:calc(100% - 24px)}aside{max-height:none;border-left:0;border-top:1px solid var(--line)}}@media(max-width:600px){.grid{grid-template-columns:1fr}.wide{grid-column:auto}.topbar{flex-wrap:wrap}.key-strip{order:3;width:100%}.shell{padding-top:0}.auth-card{padding:20px}}
  </style>
</head>
<body>
  <section id="auth-view">
    <div class="auth-card">
      <div class="brand"><div class="mark">W</div><div><h1>WingmanKVM</h1><p>轻量 ARM 远程控制台</p></div></div>
      <div id="boot-panel"><h2>正在连接设备…</h2><p class="muted">正在读取初始化状态。</p></div>
      <form id="login-form" class="hidden">
        <h2>登录控制台</h2><p class="muted">请输入管理员凭据。</p>
        <div class="grid"><label class="field wide">账号<input name="username" autocomplete="username" required></label><label class="field wide">密码<input name="password" type="password" autocomplete="current-password" required></label></div>
        <div class="form-actions"><button class="primary" type="submit">登录</button><span id="login-error" class="error"></span></div>
      </form>
      <form id="setup-form" class="hidden">
        <h2>首次设置</h2><p class="muted">创建管理员并确认自动检测到的硬件。稍后仍可在侧栏修改。</p>
        <div class="grid">
          <label class="field wide">初始化令牌<input name="setup_token" type="password" autocomplete="off" required><span class="hint">令牌显示在 WingmanKVM 首次启动日志中。</span></label>
          <label class="field">管理员账号<input name="username" autocomplete="username" required></label><label class="field">强密码<input name="password" type="password" autocomplete="new-password" minlength="12" required></label>
          <label class="field">视频设备<input name="video_device" placeholder="自动检测，如 /dev/video0"></label><label class="field">键盘 Gadget<input name="keyboard_device" placeholder="自动检测，如 /dev/hidg0"></label>
          <label class="field">鼠标 Gadget<input name="mouse_device" placeholder="自动检测，如 /dev/hidg1"></label><label class="field">GPIO 芯片<input name="gpio_chip" placeholder="如 gpiochip1"></label>
          <label class="field">GPIO 线路<input name="gpio_line" type="number" min="0" placeholder="如 7"></label><label class="field">镜像目录<input name="image_directory" placeholder="如 /var/lib/wingmankvm/images"></label>
          <label class="field wide">虚拟介质 LUN 目录<input name="lun_path" placeholder="自动检测，如 /sys/kernel/config/usb_gadget/.../lun.0"></label>
          <label class="check"><input name="power_enabled" type="checkbox">启用电源控制</label><label class="check"><input name="media_enabled" type="checkbox">启用虚拟介质</label>
        </div>
        <div class="form-actions"><button class="primary" type="submit">保存并进入</button><button id="setup-scan" type="button">自动检测</button><span id="setup-error" class="error"></span></div>
      </form>
    </div>
  </section>

  <main id="app" class="hidden">
    <header class="topbar">
      <div class="brand"><div class="mark">W</div><div><h1>WingmanKVM</h1></div></div>
      <div class="key-strip" aria-label="特殊按键">
        <button type="button" data-key="Escape">Esc</button><button type="button" data-key="Delete">Del</button>
        <button type="button" data-key="F1">F1</button><button type="button" data-key="F2">F2</button><button type="button" data-key="F3">F3</button><button type="button" data-key="F4">F4</button><button type="button" data-key="F5">F5</button><button type="button" data-key="F6">F6</button><button type="button" data-key="F7">F7</button><button type="button" data-key="F8">F8</button><button type="button" data-key="F9">F9</button><button type="button" data-key="F10">F10</button><button type="button" data-key="F11">F11</button><button type="button" data-key="F12">F12</button>
      </div>
      <div class="spacer"></div><span id="status-dot" class="status-dot"></span><span id="status-label" class="muted">连接中</span>
      <button id="keyboard-toggle" class="icon-button" type="button">⌨ 虚拟键盘</button><button id="mode-button" class="icon-button" type="button">画面：适应</button><button id="fullscreen" class="icon-button" type="button">全屏</button>
    </header>
    <div class="shell">
      <section id="workspace" class="workspace">
        <div id="console" class="console mode-fit">
          <div id="console-head" class="console-head"><strong>远程画面</strong><span id="input-state">输入已暂停</span></div>
          <div id="video-viewport" tabindex="-1" aria-label="远程视频与鼠标控制区域">
            <img id="video-feed" alt="远程设备视频" draggable="false"><span id="video-message" class="video-message">正在连接视频…</span>
          </div>
        </div>
      </section>
      <aside>
        <details open><summary>远程控制</summary><div>
          <label class="check"><input id="remote-input" type="checkbox">转发键盘和鼠标</label><p class="hint">鼠标事件只在视频窗口内发送；离开页面会立即释放全部按键。</p>
          <div class="power-row"><form method="post" action="/power"><input type="hidden" name="duration" value="0.5"><button type="submit">短按电源</button></form><form method="post" action="/power" onsubmit="return confirm('确定长按电源 5 秒吗？这可能强制关机。')"><input type="hidden" name="duration" value="5"><button class="danger" type="submit">长按 5 秒</button></form></div>
        </div></details>
        <details open><summary>视频设置</summary><div>
          <form id="video-config" class="side-grid">
            <label class="field wide">采集设备<input name="device" placeholder="/dev/video0"></label><label class="field">宽度<input name="width" type="number" min="160"></label><label class="field">高度<input name="height" type="number" min="120"></label><label class="field">帧率<input name="frames_per_second" type="number" min="1" max="240"></label><label class="field">画面处理<select name="encoding"><option value="mjpeg_passthrough">MJPEG 直通</option><option value="transcode_jpeg">JPEG 压缩</option></select></label><label class="field wide">JPEG 质量 <output id="quality-value">80</output><input name="jpeg_quality" type="range" min="20" max="100" value="80"></label>
            <div class="side-actions wide"><button class="primary" type="submit">应用视频设置</button></div>
          </form>
        </div></details>
        <details><summary>设备与 GPIO</summary><div>
          <form id="device-config" class="side-grid"><label class="field wide">键盘设备<input name="keyboard_device"></label><label class="field wide">鼠标设备<input name="mouse_device"></label><label class="check wide"><input name="power_enabled" type="checkbox">启用电源控制</label><label class="field">GPIO 芯片<input name="gpio_chip"></label><label class="field">GPIO 线路<input name="gpio_line" type="number" min="0"></label><div class="side-actions wide"><button id="scan-devices" type="button">扫描设备</button><button class="primary" type="submit">保存</button></div></form><pre id="device-results" class="device-results">尚未扫描</pre>
        </div></details>
        <details><summary>虚拟介质</summary><div>
          <form id="media-config" class="side-grid"><label class="check wide"><input name="enabled" type="checkbox">启用虚拟介质</label><label class="field wide">LUN 目录<input name="lun_path" placeholder="/sys/kernel/config/usb_gadget/…/lun.0"></label><label class="field wide">镜像目录<input name="image_directory" placeholder="/var/lib/wingmankvm/images"></label><div class="side-actions wide"><button class="primary" type="submit">保存介质设置</button></div></form>
          <form id="media-upload"><label class="field">上传 ISO / IMG<input name="file" type="file" accept=".iso,.img" required></label><div class="side-actions"><button type="submit">上传</button><button id="media-refresh" type="button">刷新列表</button></div><div class="upload-progress"><i id="upload-bar"></i></div></form>
          <div id="media-list" class="media-list">正在读取…</div>
        </div></details>
        <div class="session-row"><span id="session-user">管理员</span><button id="logout" class="ghost" type="button">退出登录</button></div>
      </aside>
    </div>
  </main>

  <dialog id="keyboard-dialog"><div class="dialog-head"><h3>虚拟键盘</h3><button type="button" data-close>关闭</button></div><div id="virtual-keyboard" class="keyboard"></div></dialog>
  <div id="toast" role="status"></div>
  <script>
  (() => {
    'use strict';
    const $ = (s, root = document) => root.querySelector(s);
    const $$ = (s, root = document) => [...root.querySelectorAll(s)];
    const authView = $('#auth-view'), app = $('#app'), setupForm = $('#setup-form'), loginForm = $('#login-form');
    const viewport = $('#video-viewport'), feed = $('#video-feed'), consoleBox = $('#console');
    let bootstrap = {}, remoteWanted = false, pageActive = !document.hidden, reconnectTimer = 0, reconnectDelay = 500;
    let toastTimer = 0, mouseX = 0, mouseY = 0, mouseBusy = false, mouseTimer = 0, lastMouseSend = 0;

    function toast(message, bad = false) { const el = $('#toast'); el.textContent = message; el.style.borderColor = bad ? '#7c3942' : ''; el.classList.add('show'); clearTimeout(toastTimer); toastTimer = setTimeout(() => el.classList.remove('show'), 2600); }
    async function request(url, options = {}) {
      const headers = new Headers(options.headers || {}); if (options.body && !(options.body instanceof FormData) && !headers.has('content-type')) headers.set('content-type', 'application/json');
      const response = await fetch(url, {...options, headers, cache: 'no-store', credentials: 'same-origin'});
      const type = response.headers.get('content-type') || ''; const data = type.includes('json') ? await response.json() : await response.text();
      if (!response.ok) throw new Error((data && (data.error || data.message)) || data || `请求失败 (${response.status})`); return data;
    }
    const value = (form, name) => new FormData(form).get(name)?.toString().trim() || '';
    const optionalNumber = v => v === '' ? null : Number(v);
    function hardwareFrom(form) { return {video_device:value(form,'video_device')||null,keyboard_device:value(form,'keyboard_device')||null,mouse_device:value(form,'mouse_device')||null,gpio_chip:form.elements.power_enabled.checked?(value(form,'gpio_chip')||null):null,gpio_line:form.elements.power_enabled.checked?optionalNumber(value(form,'gpio_line')):null,lun_path:form.elements.media_enabled.checked?(value(form,'lun_path')||null):null,image_directory:value(form,'image_directory')||null}; }
    function showState(state) {
      $('#boot-panel').classList.add('hidden'); setupForm.classList.toggle('hidden', state !== 'setup'); loginForm.classList.toggle('hidden', state !== 'login'); authView.classList.toggle('hidden', state === 'main'); app.classList.toggle('hidden', state !== 'main');
      if (state === 'main') { applyBootstrap(); connectVideo(); refreshStatus(); if(bootstrap.capabilities?.mass_storage)refreshMedia();else $('#media-list').textContent='虚拟介质尚未配置'; }
      else { stopInput(); disconnectVideo(); }
    }
    async function start() {
      try { bootstrap = await request('/api/bootstrap'); const token=setupForm.elements.setup_token; token.required=bootstrap.token_required !== false; token.closest('.field').classList.toggle('hidden',bootstrap.setup_required && bootstrap.token_required === false); showState(bootstrap.setup_required ? 'setup' : bootstrap.authenticated ? 'main' : 'login'); }
      catch (error) { $('#boot-panel h2').textContent = '无法连接 WingmanKVM'; $('#boot-panel p').textContent = error.message; }
    }
    setupForm.addEventListener('submit', async event => {
      event.preventDefault(); const button = $('button[type=submit]', setupForm), error = $('#setup-error'); button.disabled = true; error.textContent = '';
      try { const payload = {username:value(setupForm,'username'), password:value(setupForm,'password'), setup_token:value(setupForm,'setup_token'), ...hardwareFrom(setupForm)}; await request('/api/setup',{method:'POST',body:JSON.stringify(payload)}); bootstrap = await request('/api/bootstrap'); showState('main'); }
      catch (e) { error.textContent = e.message; } finally { button.disabled = false; }
    });
    loginForm.addEventListener('submit', async event => {
      event.preventDefault(); const button = $('button[type=submit]', loginForm), error = $('#login-error'); button.disabled = true; error.textContent = '';
      try { await request('/api/login',{method:'POST',body:JSON.stringify({username:value(loginForm,'username'),password:value(loginForm,'password')})}); bootstrap = await request('/api/bootstrap'); showState('main'); }
      catch (e) { error.textContent = e.message; } finally { button.disabled = false; }
    });
    $('#logout').addEventListener('click', async () => { stopInput(); try { await request('/api/logout',{method:'POST'}); } finally { bootstrap.authenticated=false; showState('login'); } });

    function unwrapConfig(source) { return source?.config || source || {}; }
    function applyBootstrap() {
      const config = unwrapConfig(bootstrap), video = config.video || {}, hid = config.hid || {}, power = config.power || {}, media = config.media || {};
      const vf = $('#video-config'), df = $('#device-config'), mf = $('#media-config');
      for (const name of ['device','width','height','frames_per_second','encoding','jpeg_quality']) if (video[name] != null && vf.elements[name]) vf.elements[name].value = video[name];
      for (const name of ['keyboard_device','mouse_device']) if (hid[name] != null) df.elements[name].value = hid[name];
      df.elements.power_enabled.checked = !!power.enabled; if (power.gpio_chip != null) df.elements.gpio_chip.value = power.gpio_chip; if (power.gpio_line != null) df.elements.gpio_line.value = power.gpio_line;
      mf.elements.enabled.checked = !!media.enabled; if (media.lun_path != null) mf.elements.lun_path.value = media.lun_path; if (media.lun_file != null && !media.lun_path) mf.elements.lun_path.value = media.lun_file; if (media.image_directory != null) mf.elements.image_directory.value = media.image_directory;
      $$('.power-row button').forEach(button=>button.disabled=bootstrap.capabilities?.gpio_power===false);
      $('#quality-value').textContent = vf.elements.jpeg_quality.value; $('#session-user').textContent = bootstrap.username || bootstrap.user?.username || '管理员';
    }
    $('#video-config').elements.jpeg_quality.addEventListener('input', event => $('#quality-value').textContent = event.target.value);
    $('#video-config').addEventListener('submit', async event => {
      event.preventDefault(); const f=event.currentTarget, payload={video:{device:value(f,'device')||null,width:optionalNumber(value(f,'width')),height:optionalNumber(value(f,'height')),frames_per_second:optionalNumber(value(f,'frames_per_second')),encoding:value(f,'encoding'),jpeg_quality:Number(value(f,'jpeg_quality'))}};
      try { await request('/api/config',{method:'POST',body:JSON.stringify(payload)}); toast('视频设置已应用'); connectVideo(true); } catch(e){ toast(e.message,true); }
    });
    $('#device-config').addEventListener('submit', async event => {
      event.preventDefault(); const f=event.currentTarget, payload={hid:{keyboard_device:value(f,'keyboard_device')||null,mouse_device:value(f,'mouse_device')||null},power:{enabled:f.elements.power_enabled.checked,gpio_chip:value(f,'gpio_chip')||null,gpio_line:optionalNumber(value(f,'gpio_line'))}};
      try { await request('/api/config',{method:'POST',body:JSON.stringify(payload)}); toast('设备设置已保存'); } catch(e){toast(e.message,true);}
    });
    $('#media-config').addEventListener('submit', async event => {
      event.preventDefault(); const f=event.currentTarget, payload={media:{enabled:f.elements.enabled.checked,lun_path:value(f,'lun_path')||null,image_directory:value(f,'image_directory')||null}};
      try { await request('/api/config',{method:'POST',body:JSON.stringify(payload)}); toast('虚拟介质设置已保存'); await refreshMedia(); } catch(e){toast(e.message,true);}
    });
    async function scanDevices(target = $('#device-results')) {
      target.textContent = '扫描中…'; const setupVisible=!setupForm.classList.contains('hidden'); try { const data=await request(setupVisible?'/api/setup/devices':'/api/devices/scan',{method:'POST'}); target.textContent=JSON.stringify(data,null,2); fillDetected(data); return data; } catch(e){target.textContent=e.message;toast(e.message,true);}
    }
    function firstPath(v) { if (typeof v === 'string') return v; if (Array.isArray(v)) return firstPath(v[0]); return v?.path || v?.device || ''; }
    function fillDetected(data) {
      const root=data?.devices||data||{}, setup=!setupForm.classList.contains('hidden')?setupForm:null, device=$('#device-config');
      const video=firstPath(root.video || root.video_devices), keyboard=firstPath(root.keyboard || root.keyboards), mouse=firstPath(root.mouse || root.mice);
      if(setup){ if(video)setup.elements.video_device.value=video;if(keyboard)setup.elements.keyboard_device.value=keyboard;if(mouse)setup.elements.mouse_device.value=mouse;if(root.gpio_chip)setup.elements.gpio_chip.value=firstPath(root.gpio_chip);if(root.gpio_line!=null)setup.elements.gpio_line.value=root.gpio_line; }
      if(device){if(keyboard)device.elements.keyboard_device.value=keyboard;if(mouse)device.elements.mouse_device.value=mouse;}
    }
    $('#scan-devices').addEventListener('click',()=>scanDevices());
    $('#setup-scan').addEventListener('click',async()=>{ const out=document.createElement('pre'); out.className='device-results wide'; setupForm.querySelector('.grid').append(out); await scanDevices(out); setTimeout(()=>out.remove(),10000); });

    function connectVideo(force=false) { if (!force && feed.src) return; clearTimeout(reconnectTimer); $('#video-message').classList.remove('hidden'); feed.src=`/video_feed?t=${Date.now()}`; }
    function disconnectVideo(){ clearTimeout(reconnectTimer); feed.removeAttribute('src'); }
    feed.addEventListener('load',()=>{ reconnectDelay=500; $('#video-message').classList.add('hidden'); });
    feed.addEventListener('error',()=>{ $('#video-message').textContent='视频断开，正在重连…'; $('#video-message').classList.remove('hidden'); feed.removeAttribute('src'); clearTimeout(reconnectTimer); reconnectTimer=setTimeout(()=>{if(!app.classList.contains('hidden'))connectVideo(true);},reconnectDelay); reconnectDelay=Math.min(reconnectDelay*2,10000); });
    let modeIndex=0; const modes=[['mode-fit','画面：适应'],['mode-native','画面：原始'],['mode-fill','画面：拉伸']];
    $('#mode-button').addEventListener('click',()=>{consoleBox.classList.remove(modes[modeIndex][0]);modeIndex=(modeIndex+1)%modes.length;consoleBox.classList.add(modes[modeIndex][0]);$('#mode-button').textContent=modes[modeIndex][1];});
    $('#fullscreen').addEventListener('click',async()=>{try{if(document.fullscreenElement)await document.exitFullscreen();else await consoleBox.requestFullscreen();}catch(e){toast(e.message,true);}});

    let drag=null; $('#console-head').addEventListener('pointerdown',event=>{if(event.button!==0)return;const r=consoleBox.getBoundingClientRect(),w=$('#workspace').getBoundingClientRect();drag={x:event.clientX-r.left,y:event.clientY-r.top,wx:w.left,wy:w.top};event.currentTarget.setPointerCapture(event.pointerId);});
    $('#console-head').addEventListener('pointermove',event=>{if(!drag)return;const area=$('#workspace').getBoundingClientRect();const left=Math.max(0,Math.min(event.clientX-area.left-drag.x,area.width-consoleBox.offsetWidth));const top=Math.max(0,Math.min(event.clientY-area.top-drag.y,area.height-consoleBox.offsetHeight));consoleBox.style.left=`${left}px`;consoleBox.style.top=`${top}px`;});
    $('#console-head').addEventListener('pointerup',()=>drag=null); $('#console-head').addEventListener('pointercancel',()=>drag=null);

    function interactiveTarget(target){ return !!target.closest('input,textarea,select,button,a,[contenteditable="true"],dialog'); }
    function inputEnabled(){ return remoteWanted && pageActive && !app.classList.contains('hidden'); }
    function setRemote(enabled){remoteWanted=enabled;$('#remote-input').checked=enabled;viewport.classList.toggle('remote',inputEnabled());$('#input-state').textContent=inputEnabled()?'远控输入已开启':'输入已暂停';if(!enabled)releaseAll();}
    $('#remote-input').addEventListener('change',event=>setRemote(event.target.checked));
    async function sendKey(key, event={}) { if(!inputEnabled())return; await request('/api/key',{method:'POST',body:JSON.stringify({key,ctrl:!!event.ctrlKey,shift:!!event.shiftKey,alt:!!event.altKey,meta:!!event.metaKey,hold_ms:key.startsWith('F')?100:25})}); }
    document.addEventListener('keydown',event=>{if(!inputEnabled()||interactiveTarget(event.target)||event.repeat)return;const key=normalizeKey(event);if(!key)return;event.preventDefault();sendKey(key,event).catch(e=>toast(e.message,true));});
    function normalizeKey(event){const code=event.code;if(/^Key[A-Z]$/.test(code)||/^Digit[0-9]$/.test(code)||/^F(?:[1-9]|1[0-2])$/.test(code)||['Escape','Delete','Enter','Tab','Backspace','Space','ArrowUp','ArrowDown','ArrowLeft','ArrowRight','Home','End','PageUp','PageDown','Insert','Minus','Equal','BracketLeft','BracketRight','Backslash','Semicolon','Quote','Backquote','Comma','Period','Slash','CapsLock'].includes(code))return code;return null;}
    $$('.key-strip [data-key]').forEach(button=>button.addEventListener('click',()=>sendKey(button.dataset.key).catch(e=>toast(e.message,true))));
    function releaseAll(){ mouseX=mouseY=0; if(app.classList.contains('hidden')||bootstrap.authenticated===false)return; request('/api/input/release-all',{method:'POST',keepalive:true}).catch(()=>{}); }
    function stopInput(){pageActive=false;remoteWanted=false;$('#remote-input').checked=false;viewport.classList.remove('remote');$('#input-state').textContent='输入已暂停';releaseAll();}
    function resumeInput(){pageActive=!document.hidden;viewport.classList.remove('remote');$('#input-state').textContent='输入已暂停';}
    document.addEventListener('visibilitychange',()=>document.hidden?stopInput():resumeInput());window.addEventListener('blur',stopInput);window.addEventListener('focus',resumeInput);window.addEventListener('pagehide',stopInput);

    viewport.addEventListener('pointermove',event=>{if(!inputEnabled())return;mouseX+=event.movementX;mouseY+=event.movementY;scheduleMouse();});
    viewport.addEventListener('pointerdown',event=>{if(!inputEnabled()||event.button>2)return;event.preventDefault();viewport.focus({preventScroll:true});const button=event.button===0?1:event.button===2?2:4;request('/api/mouse/click',{method:'POST',body:JSON.stringify({button})}).catch(e=>toast(e.message,true));});
    viewport.addEventListener('contextmenu',event=>{if(inputEnabled())event.preventDefault();});
    viewport.addEventListener('wheel',event=>{if(!inputEnabled())return;event.preventDefault();const wheel=Math.max(-127,Math.min(127,Math.round(-event.deltaY/40)||Math.sign(-event.deltaY)));request('/api/mouse/scroll',{method:'POST',body:JSON.stringify({wheel})}).catch(e=>toast(e.message,true));},{passive:false});
    function scheduleMouse(){if(mouseBusy||mouseTimer||(!mouseX&&!mouseY))return;const wait=Math.max(0,16-(performance.now()-lastMouseSend));mouseTimer=setTimeout(flushMouse,wait);}
    async function flushMouse(){mouseTimer=0;if(mouseBusy||!inputEnabled())return;const dx=Math.max(-127,Math.min(127,mouseX)),dy=Math.max(-127,Math.min(127,mouseY));if(!dx&&!dy)return;mouseX-=dx;mouseY-=dy;mouseBusy=true;lastMouseSend=performance.now();try{await request('/api/mouse/move',{method:'POST',body:JSON.stringify({dx,dy})});}catch(e){toast(e.message,true);}finally{mouseBusy=false;scheduleMouse();}}

    const rows=[['Escape','F1','F2','F3','F4','F5','F6','F7','F8','F9','F10','F11','F12','Delete'],['`','1','2','3','4','5','6','7','8','9','0','-','=','Backspace'],['Tab','Q','W','E','R','T','Y','U','I','O','P','[',']','\\'],['CapsLock','A','S','D','F','G','H','J','K','L',';','\'','Enter'],['Shift','Z','X','C','V','B','N','M',',','.','/','Shift'],['Control','Meta','Alt','Spacebar','Alt','Meta','Control']];
    const vk=$('#virtual-keyboard');rows.forEach(row=>{const line=document.createElement('div');line.className='keyboard-row';row.forEach(key=>{const b=document.createElement('button');b.type='button';b.textContent=key==='Spacebar'?'空格':key;b.dataset.key=key;if(key==='Spacebar')b.className='grow';b.addEventListener('click',()=>sendKey(key).catch(e=>toast(e.message,true)));line.append(b);});vk.append(line);});
    const keyboardDialog=$('#keyboard-dialog');$('#keyboard-toggle').addEventListener('click',()=>keyboardDialog.showModal());$('[data-close]',keyboardDialog).addEventListener('click',()=>keyboardDialog.close());

    async function refreshStatus(){if(app.classList.contains('hidden'))return;try{const status=await request('/api/status');const online=status.video?.state==='ready';$('#status-dot').classList.toggle('online',online);$('#status-label').textContent=online?(status.video?.frames_per_second?`${Math.round(status.video.frames_per_second)} FPS`:'在线'):(status.video?.message||'设备离线');}catch{$('#status-dot').classList.remove('online');$('#status-label').textContent='连接失败';}setTimeout(refreshStatus,3000);}
    function mediaItems(data){return Array.isArray(data)?data:(data?.items||data?.images||[]);}
    async function refreshMedia(){const box=$('#media-list');try{const data=await request('/api/media'),items=mediaItems(data);box.replaceChildren();if(!items.length){box.textContent='暂无镜像';return;}items.forEach(item=>{const row=document.createElement('div');row.style.marginBottom='8px';const name=typeof item==='string'?item:(item.name||item.path),mounted=!!(item.mounted||item.attached);const text=document.createElement('span');text.textContent=`${name}${mounted?' · 已挂载':''} `;const button=document.createElement('button');button.type='button';button.textContent=mounted?'卸载':'挂载';button.addEventListener('click',async()=>{try{await request(mounted?'/api/media/detach':'/api/media/attach',{method:'POST',body:JSON.stringify(mounted?{force:false}:{name,read_only:true})});await refreshMedia();}catch(e){toast(e.message,true);}});row.append(text,button);box.append(row);});}catch(e){box.textContent=e.message;}}
    $('#media-refresh').addEventListener('click',refreshMedia);
    $('#media-upload').addEventListener('submit',event=>{event.preventDefault();const f=event.currentTarget,bar=$('#upload-bar'),xhr=new XMLHttpRequest();xhr.open('POST','/api/media/upload');xhr.upload.onprogress=e=>{if(e.lengthComputable)bar.style.width=`${e.loaded/e.total*100}%`;};xhr.onload=()=>{bar.style.width='0';if(xhr.status>=200&&xhr.status<300){toast('镜像上传完成');f.reset();refreshMedia();}else toast(xhr.responseText||'上传失败',true);};xhr.onerror=()=>toast('上传连接失败',true);xhr.send(new FormData(f));});
    start();
  })();
  </script>
</body>
</html>"##;
