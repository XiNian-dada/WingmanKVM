pub static INDEX_HTML: &str = r##"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover">
  <meta name="color-scheme" content="light dark">
  <meta http-equiv="Cache-Control" content="no-store, max-age=0">
  <meta http-equiv="Pragma" content="no-cache">
  <link rel="icon" type="image/svg+xml" href="/assets/icon.svg"><link rel="alternate icon" href="/favicon.ico">
  <link rel="stylesheet" href="/assets/xterm.css">
  <title>WingmanKVM</title>
  <script>
    (function(){
      try{
        var t=localStorage.getItem('wingman_theme')||'system';
        var d=t==='dark'||(t==='system'&&window.matchMedia('(prefers-color-scheme:dark)').matches);
        if(d)document.documentElement.setAttribute('data-theme','dark');
        else if(t==='light')document.documentElement.setAttribute('data-theme','light');
      }catch(_){}
    })();
  </script>
  <style>
    :root{font-family:Inter,"PingFang SC","Microsoft YaHei",system-ui,-apple-system,sans-serif;color:var(--ink);background:var(--soft);line-height:1.45;font-feature-settings:"ss01","ss02";--ink:#171717;--ink-contrast:#fff;--body:#4d4d4d;--muted:#888;--line:#ebebeb;--line-strong:#d4d4d4;--canvas:#fff;--soft:#fafafa;--soft-2:#f5f5f5;--blue:#0070f3;--red:#ee0000;--red-soft:#fff1f1;--amber:#ab570a;--switch-bg:#d4d4d4;--shadow-2:0 1px 1px #00000005,0 2px 2px #0000000a,0 0 0 1px #0000000d;--shadow-4:0 2px 2px #00000008,0 8px 16px -4px #0000000d,0 0 0 1px #00000012;--shadow-5:0 1px 1px #00000005,0 8px 16px -4px #0000000a,0 24px 32px -8px #00000016,0 0 0 1px #00000012}
    :root[data-theme="dark"]{color:var(--ink);background:var(--soft);--ink:#f0f0f0;--ink-contrast:#0d0d0d;--body:#a8a8a8;--muted:#757575;--line:#2d2d2d;--line-strong:#444;--canvas:#141414;--soft:#0d0d0d;--soft-2:#1e1e1e;--blue:#3291ff;--red:#ff5555;--red-soft:#2d1012;--amber:#f5a623;--switch-bg:#383838;--shadow-2:0 1px 1px #00000040,0 2px 2px #00000060,0 0 0 1px #ffffff12;--shadow-4:0 2px 2px #00000050,0 8px 16px -4px #00000080,0 0 0 1px #ffffff14;--shadow-5:0 1px 1px #00000060,0 8px 16px -4px #000000a0,0 24px 32px -8px #000000d0,0 0 0 1px #ffffff18}
    :root[data-theme="dark"] .topbar,:root[data-theme="dark"] .command-bar{background:rgba(20,20,20,0.88)}
    :root[data-theme="dark"] .workspace{background:radial-gradient(circle at 50% 8%,#1c1c1c 0,#131313 42%,#0a0a0a 100%)}
    :root[data-theme="dark"] .console-head,:root[data-theme="dark"] .dropdown-menu,:root[data-theme="dark"] .diagnostics-popover,:root[data-theme="dark"] #toast,:root[data-theme="dark"] .keyboard-sheet,:root[data-theme="dark"] #keyboard-dialog,:root[data-theme="dark"] .settings-modal{background:var(--canvas);color:var(--ink)}
    :root[data-theme="dark"] .auth-shell{background:rgba(20,20,20,0.85);border-color:#ffffff18}
    :root[data-theme="dark"] .auth-story{background:linear-gradient(145deg,#181818ea,#121212f0)}
    :root[data-theme="dark"] #auth-view::before{opacity:.22}
    :root[data-theme="dark"] #auth-view::after{background:linear-gradient(to bottom,#00000015 0,#0d0d0dcc 50%,#0d0d0d 76%)}
    :root[data-theme="dark"] .auth-guide-item,:root[data-theme="dark"] .feature-chip{background:rgba(30,30,30,0.8);border-color:var(--line)}
    :root[data-theme="dark"] .guide-back,:root[data-theme="dark"] .guide-help{background:var(--soft-2);color:var(--muted);border-color:var(--line-strong)}
    :root[data-theme="dark"] .diagnostics-card{background:var(--soft-2)}
    :root[data-theme="dark"] .tab-button.active,:root[data-theme="dark"] .workspace-tab.active{background:var(--canvas);color:var(--ink)}
    :root[data-theme="dark"] .video-message{background:rgba(20,20,20,0.92);border-color:#ffffff1a}
    :root[data-theme="dark"] .input-badge.active,:root[data-theme="dark"] .media-badge{color:#60a5fa;background:#172554}
    :root[data-theme="dark"] .led-badge{background:var(--soft);color:var(--muted);border-color:var(--line)}
    :root[data-theme="dark"] .led-badge:hover{background:var(--soft-2);color:var(--ink);border-color:var(--line-strong)}
    :root[data-theme="dark"] .led-badge.active{color:#34d399;background:#064e3b33;border-color:rgba(52,211,153,0.4);box-shadow:0 0 0 1px rgba(52,211,153,0.2)}
    :root[data-theme="dark"] .led-badge.active .led-dot{background:#34d399;box-shadow:0 0 7px #34d399}
    :root[data-theme="dark"] .primary{color:#0d0d0d;background:#f0f0f0;border-color:#f0f0f0}
    :root[data-theme="dark"] .primary:hover{background:#fff}
    :root[data-theme="dark"] #inspector-toggle.active{color:#0d0d0d;background:#f0f0f0;border-color:#f0f0f0}
    :root[data-theme="dark"] #inspector-toggle.active:hover{color:#000;background:#fff;border-color:#fff}
    :root[data-theme="dark"] .mark,:root[data-theme="dark"] .user-pill,:root[data-theme="dark"] .session-avatar{color:#0d0d0d;background:#f0f0f0}
    :root[data-theme="dark"] .upload-progress{background:var(--soft-2)}
    :root[data-theme="dark"] .danger{color:var(--red);background:var(--canvas);border-color:#551c1c}
    :root[data-theme="dark"] .danger:hover{background:var(--red-soft);border-color:#772525}
    :root[data-theme="dark"] .keyboard button{background:var(--soft-2);border-color:var(--line);color:var(--ink)}
    :root[data-theme="dark"] .keyboard button:hover{background:var(--canvas);border-color:var(--line-strong)}
    :root[data-theme="dark"] .keyboard .active{color:#0d0d0d;background:#f0f0f0}
    @media(prefers-color-scheme:dark){
      :root:not([data-theme="light"]){color:var(--ink);background:var(--soft);--ink:#f0f0f0;--ink-contrast:#0d0d0d;--body:#a8a8a8;--muted:#757575;--line:#2d2d2d;--line-strong:#444;--canvas:#141414;--soft:#0d0d0d;--soft-2:#1e1e1e;--blue:#3291ff;--red:#ff5555;--red-soft:#2d1012;--amber:#f5a623;--switch-bg:#383838;--shadow-2:0 1px 1px #00000040,0 2px 2px #00000060,0 0 0 1px #ffffff12;--shadow-4:0 2px 2px #00000050,0 8px 16px -4px #00000080,0 0 0 1px #ffffff14;--shadow-5:0 1px 1px #00000060,0 8px 16px -4px #000000a0,0 24px 32px -8px #000000d0,0 0 0 1px #ffffff18}
      :root:not([data-theme="light"]) .topbar,:root:not([data-theme="light"]) .command-bar{background:rgba(20,20,20,0.88)}
      :root:not([data-theme="light"]) .workspace{background:radial-gradient(circle at 50% 8%,#1c1c1c 0,#131313 42%,#0a0a0a 100%)}
      :root:not([data-theme="light"]) .console-head,:root:not([data-theme="light"]) .dropdown-menu,:root:not([data-theme="light"]) .diagnostics-popover,:root:not([data-theme="light"]) #toast,:root:not([data-theme="light"]) .keyboard-sheet,:root:not([data-theme="light"]) #keyboard-dialog,:root:not([data-theme="light"]) .settings-modal{background:var(--canvas);color:var(--ink)}
      :root:not([data-theme="light"]) .auth-shell{background:rgba(20,20,20,0.85);border-color:#ffffff18}
      :root:not([data-theme="light"]) .auth-story{background:linear-gradient(145deg,#181818ea,#121212f0)}
      :root:not([data-theme="light"]) #auth-view::before{opacity:.22}
      :root:not([data-theme="light"]) #auth-view::after{background:linear-gradient(to bottom,#00000015 0,#0d0d0dcc 50%,#0d0d0d 76%)}
      :root:not([data-theme="light"]) .auth-guide-item,:root:not([data-theme="light"]) .feature-chip{background:rgba(30,30,30,0.8);border-color:var(--line)}
      :root:not([data-theme="light"]) .guide-back,:root:not([data-theme="light"]) .guide-help{background:var(--soft-2);color:var(--muted);border-color:var(--line-strong)}
      :root:not([data-theme="light"]) .diagnostics-card{background:var(--soft-2)}
      :root:not([data-theme="light"]) .tab-button.active,:root:not([data-theme="light"]) .workspace-tab.active{background:var(--canvas);color:var(--ink)}
      :root:not([data-theme="light"]) .video-message{background:rgba(20,20,20,0.92);border-color:#ffffff1a}
      :root:not([data-theme="light"]) .input-badge.active,:root:not([data-theme="light"]) .media-badge{color:#60a5fa;background:#172554}
      :root:not([data-theme="light"]) .led-badge{background:var(--soft);color:var(--muted);border-color:var(--line)}
      :root:not([data-theme="light"]) .led-badge:hover{background:var(--soft-2);color:var(--ink);border-color:var(--line-strong)}
      :root:not([data-theme="light"]) .led-badge.active{color:#34d399;background:#064e3b33;border-color:rgba(52,211,153,0.4);box-shadow:0 0 0 1px rgba(52,211,153,0.2)}
      :root:not([data-theme="light"]) .led-badge.active .led-dot{background:#34d399;box-shadow:0 0 7px #34d399}
      :root:not([data-theme="light"]) .primary{color:#0d0d0d;background:#f0f0f0;border-color:#f0f0f0}
      :root:not([data-theme="light"]) .primary:hover{background:#fff}
      :root:not([data-theme="light"]) #inspector-toggle.active{color:#0d0d0d;background:#f0f0f0;border-color:#f0f0f0}
      :root:not([data-theme="light"]) #inspector-toggle.active:hover{color:#000;background:#fff;border-color:#fff}
      :root:not([data-theme="light"]) .mark,:root:not([data-theme="light"]) .user-pill,:root:not([data-theme="light"]) .session-avatar{color:#0d0d0d;background:#f0f0f0}
      :root:not([data-theme="light"]) .upload-progress{background:var(--soft-2)}
      :root:not([data-theme="light"]) .danger{color:var(--red);background:var(--canvas);border-color:#551c1c}
      :root:not([data-theme="light"]) .danger:hover{background:var(--red-soft);border-color:#772525}
      :root:not([data-theme="light"]) .keyboard button{background:var(--soft-2);border-color:var(--line);color:var(--ink)}
      :root:not([data-theme="light"]) .keyboard button:hover{background:var(--canvas);border-color:var(--line-strong)}
      :root:not([data-theme="light"]) .keyboard .active{color:#0d0d0d;background:#f0f0f0}
    }
    .theme-switching,.theme-switching body,.theme-switching .topbar,.theme-switching .command-bar,.theme-switching .console,.theme-switching .console-head,.theme-switching .inspector,.theme-switching .dropdown-menu,.theme-switching .diagnostics-popover,.theme-switching .settings-modal,.theme-switching .keyboard-sheet,.theme-switching .auth-shell{transition:background-color 180ms ease,border-color 180ms ease,color 180ms ease,box-shadow 180ms ease!important}
    *{box-sizing:border-box}html,body{margin:0;min-width:320px;min-height:100%;background:var(--soft)}body{min-height:100vh;min-height:100dvh}::selection{color:var(--ink-contrast);background:var(--ink)}button,input,select{font:inherit}button{cursor:pointer}button,input,select{color:var(--ink)}button:focus-visible,input:focus-visible,select:focus-visible,[tabindex]:focus-visible{outline:0;box-shadow:0 0 0 2px var(--canvas),0 0 0 4px var(--blue)}button:disabled{color:#a1a1a1;background:var(--soft-2);cursor:not-allowed}.hidden{display:none!important}.muted{color:var(--muted)}.ok{color:var(--blue)}.error{display:block;min-height:20px;color:var(--red);font-size:13px}.mono{font-family:"Geist Mono",ui-monospace,SFMono-Regular,Menlo,Monaco,monospace}.eyebrow{margin:0 0 12px;color:var(--muted);font:12px/16px "Geist Mono",ui-monospace,SFMono-Regular,Menlo,monospace;letter-spacing:.02em;text-transform:uppercase}
    button:active:not(:disabled),.tab-button:active,.preset-chip:active,.switch:active{transform:scale(0.96)}
    .brand{display:inline-flex;align-items:center;min-width:0;line-height:0;text-decoration:none;flex-shrink:0}.brand-logo-full{display:block;height:24px;width:auto;aspect-ratio:1589/252;fill:currentColor;color:var(--ink);transition:color 160ms cubic-bezier(.2,.8,.2,1),fill 160ms cubic-bezier(.2,.8,.2,1)}.brand-logo-icon{display:none;height:22px;width:auto;aspect-ratio:348/252;fill:currentColor;color:var(--ink);transition:color 160ms cubic-bezier(.2,.8,.2,1),fill 160ms cubic-bezier(.2,.8,.2,1)}.mark{position:relative;display:grid;place-items:center;width:34px;height:34px;flex:0 0 auto;border-radius:50%;color:var(--ink-contrast);background:var(--ink);font-size:13px;font-weight:600;letter-spacing:-.4px;transition:background-color 160ms cubic-bezier(.2,.8,.2,1),color 160ms cubic-bezier(.2,.8,.2,1)}.brand-copy{min-width:0}.brand h1{margin:0;font-size:15px;font-weight:600;line-height:20px;letter-spacing:-.35px;color:var(--ink)}.brand p{margin:1px 0 0;color:var(--muted);font:11px/15px "Geist Mono",ui-monospace,monospace;white-space:nowrap}    .primary,.secondary,.ghost,.danger,.icon-button,.key-strip button,.tab-button{min-height:36px;padding:0 14px;border:1px solid var(--line);border-radius:999px;background:var(--canvas);font-size:14px;font-weight:500;transition:background-color 160ms cubic-bezier(.2,.8,.2,1),border-color 160ms cubic-bezier(.2,.8,.2,1),color 160ms cubic-bezier(.2,.8,.2,1),transform 140ms cubic-bezier(.2,.8,.2,1)}.primary{color:var(--ink-contrast);background:var(--ink);border-color:var(--ink)}.primary:hover{background:#000}.secondary:hover,.ghost:hover,.icon-button:hover,.key-strip button:hover{background:var(--soft-2);border-color:var(--line-strong)}.ghost{border-color:transparent;background:transparent}.danger{color:var(--red);background:var(--canvas);border-color:#f2c7c7}.danger:hover{background:var(--red-soft);border-color:#efaaaa}.wide-button{width:100%;min-height:44px}.field{display:grid;gap:7px;color:var(--body);font-size:13px}.field>span:first-child,.field-label{font-weight:500;color:var(--ink)}.field input,.field select{width:100%;height:40px;padding:0 12px;border:1px solid var(--line);border-radius:6px;background:var(--canvas);font-size:14px;box-shadow:0 1px 1px #00000004;transition:border-color 160ms,box-shadow 160ms}.field input:hover,.field select:hover{border-color:var(--line-strong)}.field input::placeholder{color:#a1a1a1}.field input[type=range]{height:24px;padding:0;border:0;box-shadow:none;accent-color:var(--ink)}.hint{color:var(--muted);font-size:12px;line-height:17px}.grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:16px}.wide{grid-column:1/-1}.check{display:flex;align-items:flex-start;gap:10px;color:var(--body);font-size:13px;line-height:20px}.check input{width:16px;height:16px;margin:2px 0 0;accent-color:var(--ink)}.form-actions{display:flex;align-items:center;gap:10px;margin-top:24px;flex-wrap:wrap}.form-actions .error{flex:1 1 100%}.setup-readiness{flex:1;color:var(--muted);font-size:12px}.section-rule{height:1px;margin:24px 0;background:var(--line)}
    #auth-view{position:relative;isolation:isolate;display:grid;place-items:center;min-height:100vh;min-height:100dvh;padding:32px;overflow:hidden;background:var(--soft)}#auth-view::before{content:"";position:absolute;z-index:-2;left:50%;top:-38vw;width:min(1300px,110vw);height:min(900px,82vw);transform:translateX(-50%);background:radial-gradient(circle at 18% 58%,#00dfd8 0,transparent 28%),radial-gradient(circle at 43% 32%,#007cf0 0,transparent 31%),radial-gradient(circle at 67% 46%,#ff0080 0,transparent 31%),radial-gradient(circle at 83% 67%,#f9cb28 0,transparent 28%);filter:blur(42px);opacity:.34;animation:mesh-drift 24s ease-in-out infinite alternate}#auth-view::after{content:"";position:absolute;z-index:-1;inset:0;background:linear-gradient(to bottom,#ffffff20 0,#fafafacc 50%,#fafafa 76%)}.auth-shell{display:grid;grid-template-columns:minmax(320px,.78fr) minmax(520px,1.22fr);width:min(1180px,100%);min-height:680px;overflow:hidden;border:1px solid #ffffffb8;border-radius:20px;background:#ffffffd9;box-shadow:var(--shadow-5);backdrop-filter:blur(18px)}.auth-story{position:relative;display:flex;flex-direction:column;justify-content:space-between;min-height:100%;padding:48px;border-right:1px solid var(--line);background:linear-gradient(145deg,#ffffffdc,#fafafae8)}.auth-story-copy{position:relative;z-index:1;margin:auto 0}.auth-story h2{max-width:430px;margin:0;font-size:44px;font-weight:600;line-height:1.03;letter-spacing:-2.1px}.auth-story h2 span{color:var(--muted)}.auth-guide{margin:auto 0;scroll-margin-top:16px;animation:step-in 180ms ease}.auth-guide .eyebrow{margin-top:26px}.auth-guide h2{font-size:34px;line-height:1.08;letter-spacing:-1.4px}.auth-guide-list{display:grid;gap:10px;margin-top:24px}.auth-guide-item{padding:13px 14px;border:1px solid var(--line);border-radius:9px;background:#ffffffa8}.auth-guide-item strong{display:block;font-size:13px;font-weight:500}.auth-guide-item span{display:block;margin-top:4px;color:var(--body);font-size:12px;line-height:18px}.guide-back{min-height:32px;padding:0 11px;border:1px solid var(--line);border-radius:999px;background:var(--canvas);font-size:12px}.guide-help{display:inline-grid;place-items:center;width:28px;height:28px;margin:-4px 0 -4px 3px;padding:0;border:1px solid var(--line-strong);border-radius:50%;color:var(--muted);background:var(--canvas);font:600 11px/1 ui-monospace,monospace;vertical-align:2px}.guide-help:hover{color:var(--ink);border-color:var(--ink)}.capability-heading{display:flex;align-items:center;gap:2px}.capability-heading .guide-help{margin-left:1px}.auth-story-text{max-width:430px;margin:22px 0 0;color:var(--body);font-size:16px;line-height:25px}.feature-list{display:grid;grid-template-columns:repeat(3,1fr);gap:10px;margin-top:42px}.feature-chip{padding:12px;border:1px solid var(--line);border-radius:8px;background:#ffffffb8}.feature-chip strong{display:block;font:12px/16px "Geist Mono",ui-monospace,monospace;font-weight:400}.feature-chip span{display:block;margin-top:5px;color:var(--muted);font-size:11px}.auth-foot{display:flex;justify-content:space-between;color:var(--muted);font:11px/16px "Geist Mono",ui-monospace,monospace}.auth-panel{display:flex;align-items:center;padding:48px;background:var(--canvas)}.auth-card{width:100%;max-width:660px;margin:auto}.auth-card h2{margin:0;font-size:30px;font-weight:600;line-height:38px;letter-spacing:-1.1px}.auth-lead{margin:8px 0 28px;color:var(--body);font-size:14px;line-height:21px}.auth-form.narrow{max-width:420px;margin:auto}.auth-form .field input{height:44px}.auth-form .primary{min-height:44px}.form-heading{margin-bottom:28px}.boot-orbit{width:34px;height:34px;margin-bottom:22px;border:1px solid var(--line);border-top-color:var(--ink);border-radius:50%;animation:spin .8s linear infinite}.setup-progress{display:grid;grid-template-columns:repeat(2,1fr);gap:12px;margin:0 0 30px}.progress-step{position:relative;padding-top:12px;border-top:2px solid var(--line);color:var(--muted);font:11px/16px "Geist Mono",ui-monospace,monospace}.progress-step.active{border-color:var(--ink);color:var(--ink)}.progress-step.done{border-color:var(--blue);color:var(--body)}.setup-step{display:none;animation:step-in 180ms ease}.setup-step.active{display:block}.setup-step h3{margin:0 0 6px;font-size:20px;font-weight:600;letter-spacing:-.6px}.setup-step-copy{margin:0 0 22px;color:var(--muted);font-size:13px}.setup-scan-card{display:flex;align-items:center;justify-content:space-between;gap:16px;margin-bottom:18px;padding:16px;border:1px solid var(--line);border-radius:10px;background:var(--soft)}.setup-scan-card strong{display:block;font-size:14px;font-weight:500}.setup-detection-grid{display:grid;grid-template-columns:repeat(2,1fr);gap:10px}.setup-detection-grid .capability{min-width:0}.setup-detection-grid .capability p{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.setup-detection-grid select{width:100%;height:34px;margin-top:10px;padding:0 9px;border:1px solid var(--line);border-radius:6px;background:var(--canvas);font-size:12px}.setup-advanced{margin-top:16px;border-top:1px solid var(--line)}.setup-advanced summary{padding:16px 0;color:var(--body);cursor:pointer;font-size:13px}.setup-advanced[open] summary{margin-bottom:4px}.capability{padding:16px;border:1px solid var(--line);border-radius:8px;background:var(--canvas)}.capability .status-dot{margin-bottom:12px}.capability strong{display:block;font-size:14px;font-weight:500}.capability p{margin:5px 0 0;color:var(--muted);font-size:12px}.status-dot.warning{background:var(--amber)}.setup-option-fields{margin:0 0 12px;padding:14px;border:1px solid var(--line);border-radius:8px;background:var(--soft)}.auth-choice{display:flex;justify-content:space-between;gap:18px;padding:16px 0;border-top:1px solid var(--line)}.auth-choice strong{display:block;font-size:14px;font-weight:500}.auth-choice p{margin:4px 0 0;color:var(--muted);font-size:12px}.device-results,.media-list{margin-top:12px;padding:12px;max-height:170px;overflow:auto;white-space:pre-wrap;border:1px solid var(--line);border-radius:8px;background:var(--soft);color:var(--body);font:11px/1.55 "Geist Mono",ui-monospace,monospace}
    .auth-shell{max-height:calc(100dvh - 64px)}.auth-panel{min-height:0;max-height:calc(100dvh - 64px);overflow:auto}
    #app{height:100vh;height:100dvh;display:grid;grid-template-rows:64px minmax(0,1fr);overflow:hidden;background:var(--soft)}.topbar{position:relative;z-index:40;color:var(--ink);display:grid;grid-template-columns:minmax(220px,1fr) auto minmax(220px,1fr);align-items:center;gap:16px;padding:0 20px;border-bottom:1px solid var(--line);background:#ffffffed;backdrop-filter:blur(14px)}.topbar-center{display:flex;align-items:center;gap:9px;height:32px;padding:0 12px;border:1px solid var(--line);border-radius:999px;background:var(--soft);font-size:12px}.status-dot{display:inline-block;width:7px;height:7px;flex:0 0 auto;border-radius:50%;background:#a1a1a1}.status-dot.online{background:var(--blue)}.topbar-actions{display:flex;justify-content:flex-end;align-items:center;gap:8px}.icon-button{min-height:34px;padding:0 12px;border-radius:6px;white-space:nowrap}.user-pill{display:flex;align-items:center;justify-content:center;width:32px;height:32px;border-radius:50%;color:var(--ink-contrast);background:var(--ink);font-size:12px;transition:background-color 160ms cubic-bezier(.2,.8,.2,1),color 160ms cubic-bezier(.2,.8,.2,1)}.shell{display:grid;grid-template-columns:minmax(0,1fr) 360px;min-height:0;transition:grid-template-columns 200ms cubic-bezier(.2,.8,.2,1);will-change:grid-template-columns}#app:not(.inspector-open) .shell{grid-template-columns:minmax(0,1fr) 0}.workspace{position:relative;min-width:0;min-height:0;overflow:hidden;background:radial-gradient(circle at 50% 8%,#fff 0,#f7f7f7 42%,#f3f3f3 100%)}.workspace::after{content:"";position:absolute;inset:auto 0 0;height:1px;background:var(--line)}.command-bar{position:absolute;z-index:15;top:18px;left:24px;right:24px;display:flex;align-items:center;gap:10px;height:44px;padding:5px 6px 5px 10px;border:1px solid var(--line);border-radius:10px;background:#ffffffeb;box-shadow:var(--shadow-2);backdrop-filter:blur(12px)}.remote-control{display:flex;align-items:center;gap:9px;padding-right:10px;border-right:1px solid var(--line);white-space:nowrap;font-size:12px;font-weight:500}.switch{position:relative;width:32px;height:18px;flex:0 0 auto}.switch input{position:absolute;inset:0;z-index:1;margin:0;opacity:0;cursor:pointer}.switch i{position:absolute;inset:0;border-radius:999px;background:var(--switch-bg);transition:background 160ms}.switch i::after{content:"";position:absolute;top:2px;left:2px;width:14px;height:14px;border-radius:50%;background:#fff;box-shadow:0 1px 3px #0003;transition:transform 160ms;will-change:transform;transform:translate3d(0,0,0)}.switch input:checked+i::after{transform:translate3d(14px,0,0)}.switch input:focus-visible+i{box-shadow:0 0 0 2px #fff,0 0 0 4px var(--blue)}.key-strip{display:flex;min-width:0;gap:5px;overflow-x:auto;scrollbar-width:none}.key-strip::-webkit-scrollbar{display:none}.key-strip button{min-width:38px;min-height:32px;padding:0 9px;border-radius:6px;font:12px/16px "Geist Mono",ui-monospace,monospace;white-space:nowrap}.mobile-only{display:none}.command-spacer{flex:1}.command-meta{color:var(--muted);font:11px/16px "Geist Mono",ui-monospace,monospace;white-space:nowrap}.console{position:absolute;z-index:5;left:24px;top:24px;width:calc(100% - 48px);height:calc(100% - 48px);min-width:360px;min-height:240px;resize:both;overflow:hidden;border:1px solid var(--line-strong);border-radius:12px;background:#050505;box-shadow:0 2px 2px #0000000a,0 12px 24px -8px #0000001a,0 32px 52px -20px #00000024;transition:border-radius 200ms cubic-bezier(.2,.8,.2,1),border-color 160ms,box-shadow 160ms,background-color 160ms}.console:hover{border-color:#bdbdbd}.console.dragging{transition:none!important}:fullscreen .console,#console:fullscreen{position:fixed!important;inset:0!important;left:0!important;top:0!important;width:100vw!important;height:100vh!important;max-width:none!important;max-height:none!important;border:0!important;border-radius:0!important;box-shadow:none!important;z-index:9999!important}:fullscreen #video-viewport,#console:fullscreen #video-viewport{inset:0!important}:fullscreen #console-head,#console:fullscreen #console-head{position:absolute;top:0;left:50%;transform:translateX(-50%) translateY(-100%);width:min(880px,94vw);border-radius:0 0 10px 10px;border:1px solid var(--line);border-top:0;box-shadow:var(--shadow-5);background:var(--canvas);z-index:100;opacity:0;transition:transform 200ms cubic-bezier(.2,.8,.2,1),opacity 200ms ease;pointer-events:none}:fullscreen #console-head:hover,:fullscreen #console-head:focus-within,:fullscreen #console-head.revealed,#console:fullscreen #console-head:hover,#console:fullscreen #console-head:focus-within,#console:fullscreen #console-head.revealed{transform:translateX(-50%) translateY(0);opacity:1;pointer-events:auto}.console-head{height:46px;display:flex;align-items:center;gap:12px;padding:0 14px;border-bottom:1px solid var(--line);background:var(--canvas);cursor:move;user-select:none}.console-title{font-size:13px;font-weight:500}.console-meta{color:var(--muted);font:11px/16px "Geist Mono",ui-monospace,monospace}.console-actions{display:flex;align-items:center;gap:6px;margin-left:auto}.console-actions button{min-height:28px;padding:0 9px;font-size:11px}.input-badge{display:inline-flex;align-items:center;gap:6px;min-height:24px;padding:0 9px;border-radius:999px;background:var(--soft);color:var(--muted);font-size:11px}.input-badge::before{content:"";width:6px;height:6px;border-radius:50%;background:#a1a1a1}.input-badge.active{color:#0761d1;background:#edf6ff}.input-badge.active::before{background:var(--blue)}.keyboard-led-strip{display:inline-flex;align-items:center;gap:4px}.inspector-led-strip{display:flex;gap:8px;margin-top:10px}.inspector-led-strip .led-badge{flex:1;justify-content:center;min-height:32px;font-size:11px}.led-badge{display:inline-flex;align-items:center;gap:5px;min-height:24px;padding:0 7px;border:1px solid var(--line);border-radius:6px;background:var(--soft);color:var(--muted);font:500 11px/16px "Geist Mono",ui-monospace,monospace;cursor:pointer;transition:all 140ms ease;user-select:none}.led-badge:hover{border-color:var(--line-strong);background:var(--soft-2);color:var(--ink)}.led-dot{display:inline-block;width:6px;height:6px;border-radius:50%;background:#888;transition:background-color 160ms,box-shadow 160ms}.led-badge.active{color:#065f46;background:#ecfdf5;border-color:rgba(16,185,129,0.5);box-shadow:0 0 0 1px rgba(16,185,129,0.25)}.led-badge.active .led-dot{background:#10b981;box-shadow:0 0 6px rgba(16,185,129,0.85)}#video-viewport{position:absolute;inset:46px 0 0;display:grid;place-items:center;overflow:hidden;outline:none;touch-action:auto;cursor:default;background:#050505;contain:strict;isolation:isolate;transform:translateZ(0);will-change:transform}#video-viewport.remote{cursor:crosshair;touch-action:none;box-shadow:inset 0 0 0 2px var(--blue)}#video-viewport.remote.just-captured{animation:capture-pulse 400ms ease-out}@keyframes capture-pulse{0%{box-shadow:inset 0 0 0 4px var(--blue)}50%{box-shadow:inset 0 0 0 7px rgba(0,112,243,0.6)}100%{box-shadow:inset 0 0 0 2px var(--blue)}}#video-feed{display:block;width:100%;height:100%;max-width:100%;max-height:100%;object-fit:contain;user-select:none;-webkit-user-drag:none;transform:translateZ(0);backface-visibility:hidden}.mode-native #video-feed{width:auto;height:auto;max-width:none;max-height:none}.mode-fill #video-feed{object-fit:fill}.render-pixelated #video-feed{image-rendering:pixelated}.video-message{position:absolute;max-width:calc(100% - 32px);padding:10px 13px;border:1px solid #ffffff24;border-radius:8px;background:#ffffffef;color:var(--body);box-shadow:var(--shadow-4);font-size:12px;pointer-events:none}
    .inspector{position:relative;z-index:30;display:grid;grid-template-rows:auto auto minmax(0,1fr) auto;min-width:0;overflow:hidden;border-left:1px solid var(--line);background:var(--canvas);transition:opacity 180ms,transform 200ms cubic-bezier(.2,.8,.2,1);will-change:transform,opacity;contain:layout;transform:translate3d(0,0,0)}#app:not(.inspector-open) .inspector{opacity:0;pointer-events:none;transform:translate3d(24px,0,0)}.inspector-head{display:flex;align-items:flex-start;justify-content:space-between;padding:20px 20px 14px}.inspector-head h2{margin:0;font-size:17px;font-weight:600;letter-spacing:-.45px}.inspector-head p{margin:3px 0 0;color:var(--muted);font:11px/16px "Geist Mono",ui-monospace,monospace}.inspector-tabs{display:grid;grid-template-columns:repeat(4,1fr);gap:3px;margin:0 16px 12px;padding:3px;border-radius:8px;background:var(--soft-2)}.tab-button{min-width:0;min-height:30px;padding:0 6px;border:0;border-radius:6px;background:transparent;color:var(--muted);font-size:12px}.tab-button.active{color:var(--ink);background:var(--canvas);box-shadow:0 1px 2px #0000000c,0 0 0 1px #00000008}.inspector-body{overflow:auto;border-top:1px solid var(--line)}.inspector-panel{padding:20px}.panel-kicker{margin:0 0 5px;color:var(--muted);font:11px/16px "Geist Mono",ui-monospace,monospace;text-transform:uppercase}.panel-title{margin:0;font-size:18px;font-weight:600;letter-spacing:-.5px}.panel-copy{margin:6px 0 20px;color:var(--body);font-size:13px;line-height:19px}.panel-section{padding:18px 0;border-top:1px solid var(--line)}.panel-section:first-of-type{padding-top:0;border-top:0}.control-card{display:flex;align-items:center;justify-content:space-between;gap:14px;padding:15px;border:1px solid var(--line);border-radius:10px;background:var(--soft)}.control-card strong{display:block;font-size:13px;font-weight:500}.control-card p{margin:3px 0 0;color:var(--muted);font-size:11px}.power-row{display:grid;grid-template-columns:1fr;gap:8px}.power-row form,.power-row button{width:100%}.power-row .primary{min-height:42px}.danger.power-danger{min-height:40px;background:var(--canvas)}.side-grid{display:grid;grid-template-columns:1fr 1fr;gap:13px}.side-grid .field{font-size:12px}.settings-heading{grid-column:1/-1;margin-top:8px;padding-top:16px;border-top:1px solid var(--line)}.settings-heading:first-child{margin-top:12px;padding-top:0;border-top:0}.settings-heading strong{display:block;font:600 11px/16px "Geist Mono",ui-monospace,monospace;letter-spacing:.08em;text-transform:uppercase}.settings-heading span{display:block;margin-top:3px;color:var(--muted);font-size:11px;line-height:16px}.side-actions{display:flex;align-items:center;gap:8px;margin-top:15px;flex-wrap:wrap}.side-actions.wide{grid-column:1/-1}.side-actions .primary{min-height:38px}.side-advanced{margin-top:16px;border-top:1px solid var(--line)}.side-advanced.wide{grid-column:1/-1;margin-top:0}.side-advanced summary{padding:14px 0;color:var(--body);cursor:pointer;font-size:12px}.side-advanced[open] summary{margin-bottom:10px}.device-overview{margin-top:16px}.device-overview .capability{padding:13px}.device-overview .capability p{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.upload-zone{margin-top:14px;padding:18px;border:1px dashed var(--line-strong);border-radius:10px;background:var(--soft);text-align:center}.upload-zone .field{display:block}.upload-zone input[type=file]{height:auto;margin-top:10px;padding:8px;background:var(--canvas)}.upload-progress{height:3px;margin-top:12px;overflow:hidden;border-radius:3px;background:#e5e5e5}.upload-progress i{display:block;width:0;height:100%;background:var(--ink);transition:width 160ms}.media-list{max-height:240px;background:var(--canvas)}.media-list>div{display:flex;align-items:center;justify-content:space-between;gap:8px;padding:8px 0;border-bottom:1px solid var(--line)}.media-list>div:last-child{border-bottom:0}.session-row{display:flex;align-items:center;justify-content:space-between;padding:13px 16px calc(13px + env(safe-area-inset-bottom));border-top:1px solid var(--line);color:var(--muted);font-size:12px}.session-identity{display:flex;align-items:center;gap:9px}.session-avatar{display:grid;place-items:center;width:26px;height:26px;border-radius:50%;color:var(--ink-contrast);background:var(--ink);font-size:10px;transition:background-color 160ms cubic-bezier(.2,.8,.2,1),color 160ms cubic-bezier(.2,.8,.2,1)}
    .media-status{display:flex;align-items:center;gap:10px;margin-top:14px;padding:12px;border:1px solid var(--line);border-radius:10px;background:var(--soft)}.media-status>.status-dot{margin:0 2px}.media-status.mounted>.status-dot{background:var(--blue)}.media-status.busy>.status-dot{background:var(--amber)}.media-status.error>.status-dot{background:var(--red)}.media-status-copy{min-width:0;flex:1}.media-status-copy strong,.media-status-copy span{display:block;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.media-status-copy strong{font-size:13px;font-weight:500}.media-status-copy span{margin-top:2px;color:var(--muted);font-size:11px}.media-status-actions{display:flex;gap:6px;flex:0 0 auto}.media-status-actions button{min-height:30px;padding:0 9px;font-size:11px}.media-list{max-height:300px;padding:0;white-space:normal;background:var(--canvas);font:inherit}.media-item{padding:12px;border-bottom:1px solid var(--line)}.media-item:last-child{border-bottom:0}.media-item-head{display:flex;align-items:flex-start;justify-content:space-between;gap:8px}.media-item-name{min-width:0}.media-item-name strong{display:block;overflow:hidden;text-overflow:ellipsis;color:var(--ink);font-size:12px;font-weight:500;white-space:nowrap}.media-item-meta{display:flex;align-items:center;gap:6px;margin-top:3px;color:var(--muted);font:10px/15px "Geist Mono",ui-monospace,monospace}.media-badge{padding:1px 6px;border-radius:999px;color:#0761d1;background:#edf6ff}.media-item-controls{display:grid;grid-template-columns:minmax(88px,1fr) auto auto;align-items:center;gap:6px;margin-top:10px}.media-type-select{min-width:0;height:32px;padding:0 7px;border:1px solid var(--line);border-radius:6px;background:var(--canvas);font-size:11px}.media-readonly{display:flex;align-items:center;gap:5px;height:32px;padding:0 7px;border:1px solid var(--line);border-radius:6px;color:var(--body);font-size:11px;white-space:nowrap}.media-readonly input{width:14px;height:14px;margin:0;accent-color:var(--ink)}.media-attach{min-height:32px;padding:0 10px;border-radius:6px;font-size:11px}
    dialog{max-width:none;padding:0;border:0;color:var(--ink);background:transparent}.keyboard-sheet{position:fixed;left:50%;bottom:24px;width:min(940px,calc(100vw - 48px));transform:translate3d(-50%,0,0);overflow:hidden;border:1px solid var(--line);border-radius:14px;background:var(--canvas);box-shadow:var(--shadow-5);will-change:transform}dialog::backdrop{background:#00000032;backdrop-filter:blur(4px)}.dialog-head{display:flex;align-items:center;padding:13px 16px;border-bottom:1px solid var(--line)}.dialog-head h3{margin:0;font-size:14px;font-weight:600}.dialog-head p{margin:0 0 0 10px;color:var(--muted);font:11px/16px "Geist Mono",ui-monospace,monospace}.dialog-head button{margin-left:auto}.keyboard{display:grid;gap:7px;padding:16px;overflow-x:auto}.keyboard-row{display:flex;justify-content:center;gap:6px;min-width:max-content}.keyboard button{min-width:44px;height:44px;padding:5px;border:1px solid var(--line);border-radius:6px;background:var(--soft);font:12px/16px "Geist Mono",ui-monospace,monospace}.keyboard button:hover{background:var(--soft-2);border-color:var(--line-strong)}.keyboard .grow{min-width:180px}.keyboard .active{color:var(--ink-contrast);background:var(--ink)}#toast{position:fixed;z-index:100;left:50%;bottom:24px;max-width:calc(100vw - 32px);padding:10px 14px;border:1px solid var(--line);border-radius:8px;background:var(--canvas);color:var(--ink);box-shadow:var(--shadow-4);opacity:0;pointer-events:none;transform:translate3d(-50%,12px,0);transition:opacity 180ms,transform 180ms;will-change:transform,opacity}#toast.show{opacity:1;transform:translate3d(-50%,0,0)}
    @keyframes spin{to{transform:rotate(360deg)}}@keyframes step-in{from{opacity:0;transform:translate3d(8px,0,0)}to{opacity:1;transform:none}}@keyframes mesh-drift{to{transform:translate3d(-48%,5%,0) scale(1.04)}}@keyframes dropdown-drop{from{opacity:0;transform:translate3d(0,-6px,0) scale(0.98)}to{opacity:1;transform:translate3d(0,0,0) scale(1)}}@keyframes popover-drop{from{opacity:0;transform:translate3d(-50%,-6px,0) scale(0.98)}to{opacity:1;transform:translate3d(-50%,0,0) scale(1)}}
    #app{grid-template-rows:64px minmax(0,1fr) auto}#keyboard-dialog{position:relative;inset:auto;align-self:end;width:100%;max-width:none;max-height:min(34dvh,310px);margin:0;overflow:hidden;border:0;border-top:1px solid var(--line);border-radius:0;background:var(--canvas)}#keyboard-dialog::backdrop{display:none}.keyboard-sheet{position:static;left:auto;bottom:auto;width:100%;transform:none;border:0;border-radius:0;box-shadow:0 -8px 24px -16px #0003}.keyboard-sheet .dialog-head{min-height:46px;padding:9px 16px}.keyboard{max-height:calc(min(34dvh,310px) - 46px);gap:4px;padding:10px}.keyboard-row{gap:4px}.keyboard button{min-width:40px;height:36px;font-size:11px}.keyboard .grow{min-width:150px}
    .console{left:24px;top:24px;width:calc(100% - 48px);height:calc(100% - 48px);max-width:none;max-height:none}.inspector-keys{display:grid;grid-template-columns:repeat(7,minmax(0,1fr));gap:6px;overflow:visible;margin-bottom:10px}.inspector-keys button{width:100%;min-width:0;min-height:34px;padding:0 4px}
    h2[tabindex="-1"]:focus{outline:0;box-shadow:none}
    @media(prefers-reduced-motion:reduce){*,*::before,*::after{scroll-behavior:auto!important;animation-duration:.01ms!important;animation-iteration-count:1!important;transition-duration:.01ms!important}}
    .workspace-tabs{display:flex;gap:3px;padding:3px;border-radius:7px;background:var(--soft-2)}.workspace-tab{min-height:28px;padding:0 10px;border:0;border-radius:5px;background:transparent;color:var(--muted);font-size:11px}.workspace-tab.active{color:var(--ink);background:var(--canvas);box-shadow:0 1px 2px #0001}.terminal-actions{display:flex;gap:8px;margin-top:10px}.terminal-window{position:absolute;inset:46px 0 0;overflow:hidden;background:#111;outline:0;cursor:text}.terminal-window[hidden]{display:none}.terminal-host{width:100%;height:100%;overflow:hidden}.terminal-host .xterm{height:100%;padding:14px}.terminal-host .xterm-viewport{scrollbar-color:#555 #111}.terminal-host .xterm-screen{outline:none}
    .gpio-led-row{display:flex;align-items:center;gap:8px;margin-top:10px;min-height:32px;padding:0 2px;color:var(--body);font-size:12px}.gpio-led-row .status-dot{margin-right:1px}.gpio-led-label{font:11px/16px "Geist Mono",ui-monospace,monospace;color:var(--ink)}.gpio-led-state{min-width:0;flex:1;color:var(--muted);font-size:11px}.gpio-led-row .ghost{min-height:28px;padding:0 7px;font-size:11px}.gpio-led-row .status-dot.online{background:var(--blue)}.gpio-led-row .status-dot.warning{background:var(--amber)}.gpio-led-row .status-dot.error{background:var(--red)}
    #inspector-close{display:inline-flex;align-items:center;gap:4px;min-height:28px;padding:0 9px;font-size:12px;color:var(--muted);border:1px solid transparent;border-radius:6px;background:transparent;cursor:pointer}
    #inspector-close:hover{color:var(--ink);background:var(--soft-2);border-color:var(--line)}
    .inspector-float-handle{position:absolute;z-index:25;right:0;top:50%;transform:translateY(-50%);display:none;align-items:center;gap:5px;padding:9px 10px 9px 8px;border:1px solid var(--line-strong);border-right:0;border-radius:8px 0 0 8px;background:var(--canvas);color:var(--ink);box-shadow:var(--shadow-4);font-size:12px;font-weight:500;cursor:pointer;transition:transform 160ms,background-color 160ms}
    .inspector-float-handle:hover{background:var(--soft-2);transform:translateY(-50%) translateX(-2px)}
    #app:not(.inspector-open) .inspector-float-handle{display:inline-flex}
    #inspector-toggle.active{color:var(--ink-contrast);background:var(--ink);border-color:var(--ink)}
    #inspector-toggle.active:hover{background:#000;border-color:#000;color:#fff}
    button.topbar-center{cursor:pointer;transition:background-color 160ms,border-color 160ms}
    button.topbar-center:hover{background:var(--soft-2);border-color:var(--line-strong)}
    button.topbar-center svg{opacity:.6;transition:transform 160ms}
    button.topbar-center[aria-expanded="true"] svg{transform:rotate(180deg)}
    .power-menu-container{position:relative}
    #power-menu-toggle{display:inline-flex;align-items:center;gap:6px}
    #power-menu-toggle svg{opacity:.6}
    .lang-menu-container{position:relative}
    #lang-toggle{display:inline-flex;align-items:center;gap:5px;font-size:12px;font-weight:600}
    #lang-toggle svg{opacity:.7}
    #theme-toggle{display:inline-flex;align-items:center;justify-content:center;width:34px;padding:0}
    #theme-toggle svg{opacity:.8}
    .dropdown-menu{position:absolute;right:0;top:calc(100% + 8px);z-index:90;min-width:210px;padding:6px;border:1px solid var(--line);border-radius:10px;background:var(--canvas);box-shadow:var(--shadow-5);display:grid;gap:3px;transform-origin:top right;animation:dropdown-drop 140ms cubic-bezier(.16,1,.3,1);will-change:transform,opacity;transform:translate3d(0,0,0)}
    .dropdown-menu[hidden]{display:none!important}
    .dropdown-header{padding:6px 10px 8px;border-bottom:1px solid var(--line);color:var(--muted);font:11px/16px "Geist Mono",ui-monospace,monospace}
    .dropdown-item{display:flex;align-items:center;gap:8px;width:100%;min-height:34px;padding:6px 10px;border:0;border-radius:6px;background:transparent;color:var(--ink);font-size:12px;text-align:left;cursor:pointer;transition:background-color 140ms}
    .dropdown-item:hover{background:var(--soft-2)}
    .dropdown-item.danger{color:var(--red)}
    .dropdown-item.danger:hover{background:var(--red-soft)}
    .diagnostics-popover{position:fixed;left:50%;top:64px;transform:translate3d(-50%,0,0);z-index:90;width:min(440px,calc(100vw - 32px));padding:18px;border:1px solid var(--line);border-radius:14px;background:var(--canvas);box-shadow:var(--shadow-5);animation:popover-drop 150ms cubic-bezier(.16,1,.3,1);will-change:transform,opacity;contain:layout}
    .diagnostics-popover[hidden]{display:none!important}
    .diagnostics-head{display:flex;align-items:center;justify-content:space-between;margin-bottom:12px}
    .diagnostics-head h3{margin:0;font-size:14px;font-weight:600}
    .diagnostics-grid{display:grid;grid-template-columns:1fr 1fr;gap:8px;margin-bottom:14px}
    .diagnostics-card{padding:9px 11px;border:1px solid var(--line);border-radius:8px;background:var(--soft-2)}
    .diagnostics-card span{display:block;font:10px/14px "Geist Mono",ui-monospace,monospace;color:var(--muted);text-transform:uppercase}
    .diagnostics-card strong{display:block;margin-top:2px;font-size:12px;font-weight:500;color:var(--ink);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
    .diagnostics-actions{display:flex;gap:8px;justify-content:flex-end}
    .quick-keys{display:flex;align-items:center;gap:4px;margin-left:6px}
    .icon-button-sm{min-height:28px;padding:0 8px;border:1px solid var(--line);border-radius:6px;background:var(--canvas);color:var(--body);font:11px/1 "Geist Mono",ui-monospace,monospace;cursor:pointer;transition:background-color 140ms,border-color 140ms,color 140ms,transform 140ms}
    .icon-button-sm:hover{background:var(--soft-2);border-color:var(--line-strong);color:var(--ink)}
    .icon-button-sm.danger{color:var(--red);border-color:#f2c7c7}
    .icon-button-sm.danger:hover{background:var(--red-soft);border-color:#efaaaa}
    #input-state.clickable{cursor:pointer;transition:background-color 140ms,box-shadow 140ms}
    #input-state.clickable:hover{box-shadow:0 0 0 1px var(--line-strong)}
    .video-preset-chips{display:flex;gap:6px;flex-wrap:wrap;margin:0 0 14px}
    .preset-chip{min-height:28px;padding:0 10px;border:1px solid var(--line);border-radius:999px;background:var(--soft);color:var(--body);font-size:11px;font-weight:500;cursor:pointer;transition:background-color 140ms,border-color 140ms,color 140ms,transform 120ms}
    .preset-chip:hover{background:var(--soft-2);border-color:var(--line-strong);color:var(--ink)}
    .preset-chip:active{transform:scale(0.96)}
    #settings-dialog{position:fixed;inset:0;width:100vw;height:100dvh;max-width:none;max-height:none;margin:0;padding:0;border:0;background:transparent;display:grid;place-items:center;overflow:hidden;outline:none;z-index:90;contain:strict;isolation:isolate}
    #settings-dialog:not([open]){display:none}
    #settings-dialog::backdrop{background:#00000055;backdrop-filter:blur(4px);-webkit-backdrop-filter:blur(4px);animation:settings-backdrop-fade 160ms ease forwards;will-change:opacity}
    @keyframes settings-backdrop-fade{from{opacity:0}to{opacity:1}}
    @keyframes settings-modal-pop{from{opacity:0;transform:translate3d(0,8px,0) scale(0.96)}to{opacity:1;transform:translate3d(0,0,0) scale(1)}}
    .settings-modal{position:relative;width:min(540px,calc(100vw - 32px));max-height:min(86dvh,700px);display:flex;flex-direction:column;overflow:hidden;border:1px solid var(--line);border-radius:14px;background:var(--canvas);color:var(--ink);box-shadow:0 16px 36px -10px rgba(0,0,0,0.3),0 0 0 1px var(--line);animation:settings-modal-pop 180ms cubic-bezier(.16,1,.3,1) both;will-change:transform,opacity;transform:translate3d(0,0,0);contain:layout}
    @media(prefers-reduced-motion:reduce){.settings-modal{animation:none;will-change:auto}#settings-dialog::backdrop{animation:none}}
    .settings-dialog-body{padding:16px 20px 22px;overflow-y:auto;display:flex;flex-direction:column;gap:18px}
    .settings-group{display:flex;flex-direction:column;gap:9px}
    .settings-group-title{font:600 11px/16px "Geist Mono",ui-monospace,monospace;color:var(--muted);text-transform:uppercase;letter-spacing:.08em;padding-bottom:4px;border-bottom:1px solid var(--line)}
    .settings-item{display:flex;align-items:center;justify-content:space-between;gap:14px;padding:11px 13px;border:1px solid var(--line);border-radius:9px;background:var(--soft)}
    .settings-item-copy{display:flex;flex-direction:column;gap:2px}
    .settings-item-copy strong{font-size:13px;font-weight:500;color:var(--ink)}
    .settings-item-copy span{font-size:11px;color:var(--muted);line-height:15px}
    .power-menu-icon{display:inline-block;vertical-align:middle;flex-shrink:0}
    .inspector-backdrop{display:none}
    @media(max-width:1040px){
      .auth-shell{grid-template-columns:360px minmax(0,1fr)}
      .auth-story{padding:36px}
      .auth-story h2{font-size:38px}
      .feature-list{grid-template-columns:1fr}
      .feature-chip:nth-child(3){display:none}
      .topbar{padding:0 16px}
      .topbar-center{max-width:180px}
      .console{left:16px;top:16px;width:calc(100% - 32px);height:calc(100% - 32px)}
    }
    @media(max-width:768px){
      #app{grid-template-rows:50px minmax(0,1fr) auto!important}
      .topbar{display:flex!important;align-items:center!important;justify-content:space-between!important;height:50px!important;padding:0 12px!important;gap:8px!important}
      .brand{display:inline-flex!important;align-items:center!important;gap:0!important;min-width:0!important;flex-shrink:0!important}.brand-logo-full{display:none!important}.brand-logo-icon{display:block!important;height:22px!important;width:auto!important;aspect-ratio:348/252!important}
      .mark{width:28px!important;height:28px!important;font-size:11px!important;flex-shrink:0!important}
      .brand-copy{display:flex!important;align-items:center!important;min-width:0!important}
      .brand h1{font-size:14px!important;letter-spacing:-.3px!important;white-space:nowrap!important;overflow:hidden!important;text-overflow:ellipsis!important}
      .brand p{display:none!important}
      .topbar-center{display:inline-flex!important;align-items:center!important;gap:5px!important;height:26px!important;padding:0 8px!important;font-size:11px!important;border-radius:999px!important;flex-shrink:0!important}
      #status-label{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
      .topbar-actions{display:flex!important;align-items:center!important;height:32px!important;gap:6px!important;flex-shrink:0!important}
      .power-menu-container{position:relative;display:inline-flex!important;align-items:center!important;height:32px!important;margin:0!important}
      .topbar-actions .icon-button{min-height:32px;height:32px}
      #lang-toggle{display:none!important}
      #theme-toggle{display:none!important}
      #settings-toggle{display:none!important}
      .user-pill{display:none!important}
      #power-menu-toggle{display:inline-flex!important;align-items:center!important;justify-content:center!important;width:auto!important;min-width:44px!important;height:32px!important;min-height:32px!important;padding:0 10px!important;gap:6px!important;border-radius:8px!important;flex-shrink:0!important;box-sizing:border-box!important}
      #power-menu-toggle span[data-i18n="power"]{display:none!important}
      #power-menu-toggle svg.power-chevron{display:none!important}
      #power-menu-toggle .status-dot{margin:0!important}
      #power-menu-toggle .power-menu-icon{margin:0!important}
      #keyboard-toggle{display:inline-flex!important;align-items:center!important;justify-content:center!important;width:34px!important;min-width:34px!important;height:32px!important;min-height:32px!important;padding:0!important;border-radius:8px!important;flex-shrink:0!important;box-sizing:border-box!important}
      #keyboard-toggle span[data-i18n="keyboard"]{display:none!important}
      #keyboard-toggle svg{margin:0!important}
      #inspector-toggle{display:inline-flex!important;align-items:center!important;justify-content:center!important;height:32px!important;min-height:32px!important;padding:0 13px!important;border-radius:8px!important;font-size:12px!important;font-weight:500!important;line-height:1!important;white-space:nowrap!important;flex-shrink:0!important;box-sizing:border-box!important}
      .shell,#app:not(.inspector-open) .shell{display:block}
      .workspace{height:100%;min-height:0;background:#050505;overflow:hidden}
      .console{position:absolute!important;inset:0!important;left:0!important;top:0!important;width:100%!important;height:100%!important;min-width:0!important;min-height:0!important;max-width:none!important;max-height:none!important;border:0!important;border-radius:0!important;box-shadow:none!important;resize:none!important}
      .console-head{height:44px!important;padding:0 12px!important;display:flex!important;align-items:center!important;justify-content:space-between!important;gap:8px!important;background:var(--canvas)!important;border-bottom:1px solid var(--line)!important;cursor:default!important;user-select:none!important}
      .workspace-tabs{display:inline-flex!important;gap:3px!important;padding:3px!important;border-radius:7px!important;background:var(--soft-2)!important;flex-shrink:0!important}
      .workspace-tab{white-space:nowrap!important;word-break:keep-all!important;min-height:28px!important;padding:0 10px!important;font-size:11px!important;font-weight:500!important;border-radius:5px!important;flex-shrink:0!important}
      .window-dots{display:none!important}
      .quick-keys{display:none!important}
      #video-transport-label{display:none!important}
      .console-actions{display:flex!important;align-items:center!important;gap:6px!important;margin-left:auto!important;flex-shrink:0!important}
      .console-actions .keyboard-led-strip{display:none!important}
      .console-actions #input-release{display:none!important}
      .console-actions #mode-button{display:none!important}
      #input-state{white-space:nowrap!important;word-break:keep-all!important;display:inline-flex!important;align-items:center!important;gap:5px!important;min-height:28px!important;padding:0 10px!important;border-radius:999px!important;font-size:11px!important;flex-shrink:0!important}
      .console-actions .secondary{min-height:28px!important;height:28px!important;padding:0 8px!important;font-size:11px!important;border-radius:6px!important}
      #video-viewport{inset:44px 0 0!important;background:#050505!important}
      .inspector{position:fixed!important;z-index:85!important;left:0!important;right:0!important;bottom:0!important;width:100%!important;max-width:100%!important;height:min(84dvh,720px)!important;border:0!important;border-top:1px solid var(--line)!important;border-radius:20px 20px 0 0!important;box-shadow:0 -12px 40px rgba(0,0,0,0.5),0 -1px 0 rgba(255,255,255,0.06)!important;background:var(--canvas)!important;transform:translateY(100%)!important;opacity:0;pointer-events:none;transition:transform 260ms cubic-bezier(.16,1,.3,1),opacity 200ms ease!important}
      #app.inspector-open .inspector{transform:translateY(0)!important;opacity:1!important;pointer-events:auto!important}
      .inspector-backdrop{display:block!important;position:fixed;inset:0;z-index:80;background:rgba(0,0,0,0.48);backdrop-filter:blur(4px);-webkit-backdrop-filter:blur(4px);opacity:0;pointer-events:none;transition:opacity 220ms ease}
      #app.inspector-open .inspector-backdrop{opacity:1;pointer-events:auto}
      .inspector-head::before{content:"";position:absolute;top:8px;left:50%;width:38px;height:4px;transform:translateX(-50%);border-radius:999px;background:var(--line-strong);opacity:.8}
      .inspector-head{padding:20px 16px 12px;border-bottom:1px solid var(--line)}
      .inspector-head h2{font-size:16px;font-weight:600}
      .inspector-tabs{margin:10px 14px 10px;padding:3px;gap:3px;background:var(--soft-2);border-radius:8px}
      .inspector-tabs .tab-button{min-height:32px;font-size:12px;font-weight:500}
      .inspector-panel{padding:14px 14px 20px}
      .control-card{padding:12px 14px;border-radius:10px}
      .inspector-keys{display:grid;grid-template-columns:repeat(auto-fill,minmax(44px,1fr));gap:6px}
      .inspector-keys button{min-height:36px;font-size:11px}
      .power-row{gap:8px}
      .power-row button{min-height:42px;font-size:13px}
      .side-grid{grid-template-columns:1fr;gap:10px}
      .side-actions.wide{grid-column:auto}
      .inspector-float-handle{display:none!important}
      .session-row{padding:12px 16px calc(12px + env(safe-area-inset-bottom,12px));border-top:1px solid var(--line);border-radius:0;background:var(--soft);font-size:11px}
      #keyboard-dialog{position:relative;inset:auto;align-self:end;width:100%;max-height:min(44dvh,320px);border-top:1px solid var(--line);background:var(--canvas)}
      .keyboard{padding:8px 10px;gap:4px}
      .keyboard-row{gap:4px}
      .keyboard button{min-width:34px;height:38px;font-size:11px}
      .keyboard .grow{min-width:110px}
      .diagnostics-popover{top:54px!important;left:10px!important;right:10px!important;transform:none!important;width:auto!important;max-width:none!important;padding:14px!important;border-radius:14px!important}
      .diagnostics-grid{grid-template-columns:1fr 1fr;gap:8px;margin-bottom:12px}
      .settings-modal{width:calc(100vw - 20px)!important;max-height:min(88dvh,660px)!important;border-radius:16px!important}
      .settings-dialog-body{padding:14px 14px 20px!important;gap:14px!important}
      .settings-item{padding:10px 12px!important;gap:10px!important}
      .dropdown-menu{right:4px;top:calc(100% + 6px);max-width:calc(100vw - 16px)}
      #auth-view{padding:0;overflow:auto;display:block}
      .auth-shell{display:block;width:100%;min-height:100dvh;max-height:none;border:0;border-radius:0;box-shadow:none;backdrop-filter:none}
      .auth-story{min-height:auto;padding:28px 20px 24px;border:0;border-bottom:1px solid var(--line)}
      .auth-story-copy{margin:24px 0 0}
      .auth-story h2{font-size:28px;line-height:1.15;letter-spacing:-1px}
      .auth-story-text{margin-top:10px;font-size:14px;line-height:22px}
      .feature-list,.auth-foot{display:none}
      .auth-panel{padding:28px 20px 48px;max-height:none;overflow:visible}
      .auth-form.narrow{max-width:none}
      .auth-card h2{font-size:24px;line-height:30px}
      .auth-lead{margin:6px 0 20px}
      .grid{grid-template-columns:1fr}
      .wide{grid-column:auto}
      .capability-grid,.setup-detection-grid{grid-template-columns:1fr}
      .setup-scan-card{align-items:flex-start;flex-direction:column}
      .form-actions{align-items:stretch}
      .form-actions button{min-height:44px}
      .form-actions .primary{order:-1;width:100%}
    }
    @media(max-width:440px){
      .topbar{padding:0 8px!important;gap:6px!important}
      .brand-logo-icon{height:20px!important}
      .topbar-center #status-label{display:none!important}
      .topbar-center{width:26px!important;height:26px!important;padding:0!important;justify-content:center!important;border-radius:50%!important}
      .topbar-center svg{display:none!important}
      #inspector-toggle{padding:0 10px!important}
      .console-actions{gap:4px!important}
      .console-actions .secondary{padding:0 6px!important}
      .inspector-panel{padding:12px 12px 18px!important}
      .dialog-head p{display:none!important}
    }
  </style>
  <style>
    #webrtc-feed{display:block;width:100%;height:100%;max-width:100%;max-height:100%;object-fit:contain;user-select:none;-webkit-user-drag:none;background:#050505;transform:translateZ(0);backface-visibility:hidden}
    #video-feed.video-surface,#webrtc-feed.video-surface{display:block}
    #video-feed.video-surface.hidden,#webrtc-feed.video-surface.hidden{display:none!important}
    .mode-native #webrtc-feed{width:auto;height:auto;max-width:none;max-height:none}
    .mode-fill #webrtc-feed{object-fit:fill}
    .render-pixelated #webrtc-feed{image-rendering:pixelated}
    #video-transport-label{min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
    .gpio-config-block{grid-column:1/-1;margin-top:5px;padding-top:14px;border-top:1px solid var(--line)}.gpio-config-title{display:flex;align-items:center;justify-content:space-between;gap:8px;margin-bottom:10px;color:var(--ink);font:600 11px/16px "Geist Mono",ui-monospace,monospace;letter-spacing:.04em}.gpio-inline-test{min-height:26px;padding:0 8px;font-size:10px;letter-spacing:0}.gpio-config-status{display:flex;align-items:center;gap:6px;color:var(--muted);font-size:10px;font-weight:400;letter-spacing:0}.gpio-config-status .status-dot{width:6px;height:6px}.gpio-config-fields{gap:10px}.gpio-config-fields .field{font-size:11px}.gpio-config-fields .field input,.gpio-config-fields .field select{height:36px;font-size:12px}.field-help{display:inline-grid;place-items:center;width:16px;height:16px;margin-left:4px;padding:0;border:1px solid var(--line-strong);border-radius:50%;color:var(--muted);background:var(--canvas);font:600 10px/1 "Geist Mono",ui-monospace,monospace;vertical-align:1px;cursor:help;transition:color 160ms,border-color 160ms,background-color 160ms}.field-help:hover,.field-help:focus-visible{color:var(--ink);border-color:var(--ink);background:var(--soft-2)}.field-help:focus-visible{outline:0;box-shadow:0 0 0 2px var(--canvas),0 0 0 4px var(--blue)}#field-tooltip{position:fixed;z-index:120;max-width:min(280px,calc(100vw - 24px));padding:8px 10px;border:1px solid #383838;border-radius:8px;color:#fff;background:#1d1d1d;box-shadow:0 8px 20px #0003;font-size:11px;line-height:17px;overflow-wrap:anywhere;pointer-events:none;opacity:0;translate:0 4px;transition:opacity 140ms,translate 140ms}#field-tooltip.visible{opacity:1;translate:0 0}#field-tooltip[hidden]{display:none}
  </style>
</head>
<body>
  <svg style="display:none;" aria-hidden="true">
    <defs>
      <symbol id="logo-full" viewBox="0 0 1589 252" fill="currentColor">
        <path fill="currentColor" fill-rule="evenodd" d="M594.21 103.58C592.82 104.56 593.01 106.51 593.00 108.00 C592.98 111.33 593.00 114.67 593.00 118.00 C593.00 132.33 593.00 146.67 593.00 161.00 C593.00 169.00 593.00 177.00 593.00 185.00 C593.00 186.93 592.10 191.95 593.58 193.42 C594.37 194.22 596.00 193.99 597.00 194.00 C600.33 194.02 603.67 194.00 607.00 194.00 C609.74 194.00 615.47 195.08 617.79 193.42 C619.18 192.44 618.99 190.49 619.00 189.00 C619.02 185.67 619.00 182.33 619.00 179.00 C619.00 164.67 619.00 150.33 619.00 136.00 C619.00 132.03 618.76 127.96 619.04 124.00 C619.19 121.95 620.05 120.08 619.95 118.00 C619.84 115.96 619.05 114.07 619.01 112.00 C618.97 110.04 619.92 105.07 618.42 103.58 C617.63 102.78 616.00 103.01 615.00 103.00 C611.67 102.98 608.33 103.00 605.00 103.00 C602.26 103.00 596.53 101.92 594.21 103.58Z M233.01 102.58C230.02 104.15 230.00 107.05 230.00 110.00 C230.00 114.33 230.00 118.67 230.00 123.00 C230.00 138.18 234.13 159.84 221.00 170.84 C213.47 177.15 205.26 177.00 196.00 177.00 C187.33 177.00 178.67 177.00 170.00 177.00 C166.00 177.00 162.00 177.00 158.00 177.00 C155.73 177.00 153.19 176.79 151.38 178.42 C147.81 181.64 149.00 187.69 149.00 192.00 C149.00 200.67 149.00 209.33 149.00 218.00 C149.00 224.91 149.30 232.44 154.16 237.96 C161.23 245.97 174.51 244.00 184.00 244.00 C209.67 244.00 235.33 244.00 261.00 244.00 C277.00 244.00 293.00 244.00 309.00 244.00 C317.66 244.00 326.85 244.82 333.84 238.62 C340.55 232.69 340.00 223.06 340.00 215.00 C340.00 196.33 340.00 177.67 340.00 159.00 C340.00 149.33 340.00 139.67 340.00 130.00 C340.00 122.55 340.35 115.31 335.62 109.01 C329.83 101.29 320.54 102.00 312.00 102.00 C293.67 102.00 275.33 102.00 257.00 102.00 C251.67 102.00 246.33 102.00 241.00 102.00 C238.59 102.00 235.21 101.41 233.01 102.58Z M164.00 102.38C161.93 102.88 159.85 103.74 158.00 104.80 C148.03 110.51 149.00 122.15 149.00 132.00 C149.00 138.00 148.93 144.00 149.00 150.00 C149.04 153.74 150.08 157.24 154.00 158.58 C158.08 159.96 163.73 159.00 168.00 159.00 C177.53 159.00 187.66 159.98 197.00 158.15 C199.19 157.71 201.02 157.33 203.00 156.20 C212.49 150.77 212.00 140.44 212.00 131.00 C212.00 124.33 212.02 117.67 212.00 111.00 C211.99 107.05 211.20 103.85 207.00 102.42 C202.92 101.04 197.27 102.00 193.00 102.00 C186.67 102.00 180.33 102.00 174.00 102.00 C170.74 102.00 167.19 101.61 164.00 102.38Z M1147.00 101.37C1142.41 102.34 1138.01 103.92 1134.00 106.37 C1132.52 107.27 1130.01 109.91 1128.21 109.86 C1126.26 109.80 1127.42 104.68 1125.79 103.58 C1124.29 102.56 1121.71 103.00 1120.00 103.00 C1116.00 103.00 1112.00 103.00 1108.00 103.00 C1106.29 103.00 1103.71 102.56 1102.21 103.58 C1100.81 104.52 1101.01 106.54 1101.00 108.00 C1100.98 111.67 1101.00 115.33 1101.00 119.00 C1101.00 134.67 1101.00 150.33 1101.00 166.00 C1101.00 172.33 1101.00 178.67 1101.00 185.00 C1101.00 187.16 1100.30 190.91 1101.58 192.79 C1102.67 194.41 1105.34 194.00 1107.00 194.00 C1112.00 194.00 1117.00 194.02 1122.00 194.00 C1123.84 193.99 1126.16 194.13 1126.79 191.95 C1127.89 188.14 1127.00 182.95 1127.00 179.00 C1127.00 170.00 1127.00 161.00 1127.00 152.00 C1127.00 144.19 1127.11 136.90 1133.01 131.01 C1141.09 122.93 1157.65 122.56 1161.99 135.00 C1164.15 141.19 1164.00 147.54 1164.00 154.00 C1164.00 162.00 1164.00 170.00 1164.00 178.00 C1164.00 182.24 1163.03 187.86 1164.21 191.95 C1164.84 194.13 1167.16 193.99 1169.00 194.00 C1174.00 194.02 1179.00 194.00 1184.00 194.00 C1185.70 194.00 1188.28 194.40 1189.42 192.79 C1190.72 190.97 1190.00 187.11 1190.00 185.00 C1190.00 178.00 1190.00 171.00 1190.00 164.00 C1190.00 142.89 1193.15 117.91 1172.00 105.38 C1168.58 103.35 1164.79 102.56 1161.00 101.63 C1156.75 100.60 1151.29 100.46 1147.00 101.37Z M1035.00 101.37C1024.26 103.54 1014.05 106.93 1007.11 116.00 C1004.97 118.80 1000.30 124.28 1003.42 127.79 C1005.52 130.16 1012.05 129.83 1015.00 130.04 C1018.20 130.28 1021.59 131.83 1024.84 130.79 C1027.53 129.93 1029.28 125.36 1032.00 123.80 C1041.66 118.26 1058.01 121.03 1061.11 133.00 C1061.50 134.52 1062.28 136.40 1061.42 137.90 C1060.44 139.59 1058.59 139.04 1057.00 139.08 C1053.34 139.18 1049.75 140.00 1046.00 140.00 C1041.46 140.00 1036.45 139.17 1032.00 139.52 C1023.76 140.17 1014.88 142.71 1008.00 147.41 C992.29 158.15 996.81 181.02 1011.00 190.33 C1021.41 197.16 1036.43 197.47 1048.00 193.60 C1050.72 192.69 1053.60 191.55 1056.00 189.99 C1057.48 189.02 1059.17 187.23 1060.90 186.78 C1062.39 186.38 1062.13 188.15 1062.15 189.01 C1062.18 190.97 1062.96 193.20 1065.05 193.79 C1070.28 195.27 1077.57 194.02 1083.00 194.00 C1084.73 193.99 1087.24 194.15 1088.42 192.58 C1089.46 191.20 1089.00 188.60 1089.00 187.00 C1089.00 182.33 1089.00 177.67 1089.00 173.00 C1089.00 164.33 1089.00 155.67 1089.00 147.00 C1089.00 140.74 1089.22 134.09 1087.62 128.00 C1081.87 106.02 1055.53 97.21 1035.00 101.37Z M1033.00 157.69C1039.13 155.58 1047.56 157.00 1054.00 157.00 C1055.94 157.00 1059.09 156.43 1060.79 157.58 C1062.22 158.54 1062.00 160.57 1061.62 162.00 C1060.64 165.76 1058.17 169.35 1055.00 171.59 C1049.46 175.49 1040.55 177.32 1034.00 175.32 C1024.57 172.44 1022.74 161.22 1033.00 157.69Z M886.00 101.37C881.30 102.36 877.11 104.35 873.00 106.79 C871.69 107.56 869.79 109.86 868.21 109.85 C866.24 109.84 867.43 104.04 864.84 103.21 C863.09 102.65 860.81 103.00 859.00 103.00 C854.67 103.00 850.33 103.00 846.00 103.00 C843.98 103.00 840.93 102.56 840.21 105.05 C839.60 107.16 840.00 109.82 840.00 112.00 C840.00 116.67 840.00 121.33 840.00 126.00 C840.00 141.00 840.00 156.00 840.00 171.00 C840.00 176.00 840.00 181.00 840.00 186.00 C840.00 187.94 839.43 191.09 840.58 192.79 C841.67 194.41 844.34 194.00 846.00 194.00 C850.33 194.00 854.67 194.00 859.00 194.00 C860.71 194.00 863.29 194.44 864.79 193.42 C866.76 192.09 866.00 188.02 866.00 186.00 C866.00 169.95 864.05 152.61 868.12 137.00 C870.13 129.28 880.41 123.90 888.00 125.16 C895.96 126.47 899.94 132.67 901.15 140.00 C903.24 152.61 902.00 166.19 902.00 179.00 C902.00 182.45 900.60 189.87 902.58 192.79 C904.00 194.90 908.83 194.00 911.00 194.00 C915.00 194.00 919.00 194.02 923.00 194.00 C924.46 193.99 926.48 194.19 927.42 192.79 C928.30 191.50 928.00 189.47 928.00 188.00 C928.02 178.33 928.00 168.67 928.00 159.00 C928.00 152.27 927.54 145.57 929.11 139.00 C931.22 130.17 939.79 123.63 949.00 125.16 C958.77 126.77 963.00 136.06 963.00 145.00 C963.00 155.67 963.00 166.33 963.00 177.00 C963.00 181.54 961.95 187.58 963.21 191.95 C963.93 194.44 966.98 194.00 969.00 194.00 C974.33 194.00 979.67 194.11 985.00 193.99 C987.32 193.94 989.53 193.55 989.95 190.95 C991.36 182.12 990.00 171.96 990.00 163.00 C990.00 142.89 992.87 114.68 970.00 105.04 C965.75 103.24 961.56 101.89 957.00 101.15 C947.91 99.69 938.99 102.09 931.00 106.26 C928.50 107.57 926.19 109.37 924.00 111.14 C923.03 111.93 921.61 113.63 920.15 113.22 C918.84 112.84 917.86 110.95 916.96 110.04 C915.50 108.60 913.75 107.43 912.00 106.37 C904.52 101.80 894.69 99.53 886.00 101.37Z M766.00 101.37C756.22 103.44 746.63 108.24 740.16 116.00 C725.32 133.77 727.08 163.27 744.00 178.96 C753.31 187.59 765.95 189.03 778.00 189.00 C783.43 188.99 789.29 187.41 794.00 184.62 C795.52 183.72 798.93 180.17 800.79 180.78 C803.49 181.67 801.52 188.25 801.11 190.00 C799.18 198.21 794.44 204.38 786.00 206.63 C778.14 208.72 769.38 208.21 762.00 204.51 C759.42 203.22 754.81 197.12 752.00 197.78 C750.49 198.14 749.21 199.74 748.00 200.63 C744.76 203.00 741.22 204.97 738.00 207.38 C736.61 208.41 734.57 209.99 735.05 212.00 C735.43 213.58 736.92 214.89 738.01 216.00 C741.43 219.49 745.62 222.54 750.00 224.73 C758.06 228.76 767.15 230.00 776.00 230.00 C795.17 230.00 815.79 223.91 824.21 205.00 C830.31 191.30 828.00 174.69 828.00 160.00 C828.00 146.00 828.00 132.00 828.00 118.00 C828.00 114.67 828.02 111.33 828.00 108.00 C827.99 106.54 828.19 104.52 826.79 103.58 C825.29 102.56 822.71 103.00 821.00 103.00 C817.00 103.00 813.00 103.00 809.00 103.00 C807.23 103.00 804.90 102.62 803.21 103.21 C800.96 104.00 802.13 109.72 798.95 108.96 C796.07 108.27 793.75 105.50 791.00 104.38 C783.41 101.28 774.13 99.65 766.00 101.37Z M772.00 124.68C782.67 121.42 795.77 126.44 800.20 137.00 C805.02 148.48 797.78 159.80 787.00 164.21 C775.78 168.79 763.33 163.10 758.79 152.00 C754.34 141.10 760.65 128.14 772.00 124.68Z M678.00 101.37C673.29 102.36 668.99 103.95 665.00 106.68 C663.52 107.69 661.07 110.85 659.21 110.85 C657.17 110.85 658.54 104.76 656.79 103.58 C655.29 102.56 652.71 103.00 651.00 103.00 C646.67 103.00 642.33 103.00 638.00 103.00 C636.60 103.00 633.68 102.47 632.58 103.58 C631.62 104.53 632.00 106.79 632.00 108.00 C632.00 111.67 632.00 115.33 632.00 119.00 C632.00 134.33 632.00 149.67 632.00 165.00 C632.00 171.67 632.00 178.33 632.00 185.00 C632.00 187.16 631.30 190.91 632.58 192.79 C633.67 194.41 636.34 194.00 638.00 194.00 C643.00 194.00 648.00 194.02 653.00 194.00 C654.84 193.99 657.16 194.13 657.79 191.95 C658.89 188.14 658.00 182.95 658.00 179.00 C658.00 170.00 658.00 161.00 658.00 152.00 C658.00 144.60 658.49 136.53 664.01 131.01 C672.78 122.24 689.46 123.02 693.99 136.00 C696.11 142.07 695.00 146.87 695.00 153.00 C694.99 163.33 694.67 173.72 695.48 184.00 C695.66 186.35 694.57 188.61 695.21 190.99 C695.83 193.31 697.84 193.93 700.00 193.99 C704.99 194.12 710.01 194.00 715.00 194.00 C716.89 194.00 720.14 194.59 721.42 192.79 C722.72 190.97 722.00 187.11 722.00 185.00 C722.00 178.00 722.00 171.00 722.00 164.00 C722.00 143.43 725.63 118.42 705.00 106.05 C701.24 103.79 697.18 102.65 693.00 101.63 C688.50 100.54 682.55 100.41 678.00 101.37Z M1314.36 70.58C1312.81 72.03 1315.13 76.39 1315.67 78.00 C1318.06 85.07 1320.94 92.03 1323.63 99.00 C1332.98 123.22 1342.08 147.62 1351.00 172.00 C1352.84 177.02 1354.95 181.94 1356.67 187.00 C1357.86 190.50 1358.87 193.77 1363.00 193.99 C1368.96 194.30 1375.03 194.00 1381.00 194.00 C1383.02 194.00 1386.03 194.53 1387.84 193.42 C1390.07 192.05 1390.50 188.26 1391.37 186.00 C1393.95 179.30 1396.76 172.69 1399.37 166.00 C1408.97 141.34 1418.25 116.51 1428.22 92.00 C1430.23 87.06 1432.17 82.01 1434.00 77.00 C1434.60 75.36 1436.20 73.02 1435.75 71.21 C1435.25 69.20 1430.49 70.00 1429.00 70.00 C1424.00 70.00 1419.00 70.00 1414.00 70.00 C1411.93 70.00 1408.97 69.49 1407.42 71.21 C1405.67 73.15 1405.15 76.58 1404.33 79.00 C1402.62 84.05 1400.57 89.03 1398.63 94.00 C1391.81 111.51 1384.97 129.18 1379.00 147.00 C1378.09 149.70 1377.16 152.39 1375.96 154.98 C1375.62 155.70 1375.09 156.85 1374.11 156.85 C1372.41 156.85 1371.72 153.16 1371.22 152.00 C1369.01 146.82 1367.46 141.34 1365.67 136.00 C1360.74 121.26 1355.26 106.73 1350.33 92.00 C1348.76 87.29 1346.91 82.71 1345.33 78.00 C1344.61 75.85 1344.13 72.93 1342.58 71.21 C1341.03 69.49 1338.07 70.00 1336.00 70.00 C1331.00 70.00 1326.00 70.00 1321.00 70.00 C1319.35 70.00 1315.68 69.34 1314.36 70.58Z M1460.00 69.37C1456.01 70.17 1452.06 69.98 1448.00 70.00 C1446.54 70.01 1444.52 69.81 1443.58 71.21 C1442.30 73.09 1443.00 76.84 1443.00 79.00 C1443.00 85.67 1443.00 92.33 1443.00 99.00 C1443.00 121.67 1443.00 144.33 1443.00 167.00 C1443.00 173.00 1443.00 179.00 1443.00 185.00 C1443.00 187.16 1442.30 190.91 1443.58 192.79 C1444.67 194.41 1447.34 194.00 1449.00 194.00 C1453.67 194.00 1458.33 194.00 1463.00 194.00 C1464.66 194.00 1467.33 194.41 1468.42 192.79 C1469.70 190.91 1469.00 187.16 1469.00 185.00 C1469.00 178.67 1469.00 172.33 1469.00 166.00 C1469.00 160.67 1469.00 155.33 1469.00 150.00 C1469.00 143.67 1469.00 137.33 1469.00 131.00 C1469.00 129.97 1468.40 125.15 1470.16 125.15 C1471.71 125.15 1472.72 127.95 1473.37 129.00 C1476.15 133.55 1478.62 138.33 1481.21 143.00 C1486.35 152.25 1490.88 161.95 1496.37 171.00 C1497.93 173.57 1499.56 178.83 1502.16 180.42 C1503.53 181.26 1505.48 181.00 1507.00 181.00 C1510.29 181.00 1513.72 181.28 1516.99 180.95 C1521.71 180.47 1523.82 172.54 1525.79 169.00 C1531.92 157.95 1538.08 146.81 1544.63 136.00 C1546.21 133.40 1547.33 130.55 1549.01 128.00 C1549.58 127.14 1550.57 125.15 1551.84 125.15 C1553.30 125.15 1552.99 128.15 1553.00 129.00 C1553.02 133.67 1553.00 138.33 1553.00 143.00 C1553.00 155.00 1553.00 167.00 1553.00 179.00 C1553.00 182.45 1551.60 189.87 1553.58 192.79 C1554.80 194.60 1558.15 194.00 1560.00 194.00 C1565.00 194.00 1570.00 194.02 1575.00 194.00 C1576.96 193.99 1579.27 194.05 1580.20 191.95 C1581.30 189.47 1580.00 185.67 1580.00 183.00 C1580.00 175.33 1580.00 167.67 1580.00 160.00 C1580.00 132.67 1579.99 105.33 1580.00 78.00 C1580.00 76.24 1580.95 71.68 1579.64 70.36 C1578.41 69.13 1574.61 70.00 1573.00 70.00 C1568.00 70.00 1563.00 70.00 1558.00 70.00 C1555.48 70.00 1553.09 69.84 1551.41 72.05 C1548.23 76.24 1546.35 81.51 1543.63 86.00 C1535.92 98.72 1529.01 112.00 1521.79 125.00 C1519.40 129.30 1517.20 133.80 1514.63 138.00 C1513.82 139.33 1512.65 141.75 1511.00 142.22 C1509.07 142.76 1507.77 139.21 1507.00 138.00 C1504.15 133.51 1502.13 128.55 1499.37 124.00 C1491.15 110.46 1484.47 95.85 1476.79 82.00 C1474.70 78.24 1473.06 71.66 1468.99 69.58 C1466.59 68.35 1462.54 68.86 1460.00 69.37Z M1221.00 69.41C1216.64 69.89 1212.41 70.00 1208.00 70.00 C1206.79 70.00 1204.53 69.62 1203.58 70.58 C1202.10 72.05 1203.00 77.07 1203.00 79.00 C1203.00 83.70 1202.09 88.42 1202.09 93.00 C1202.09 95.68 1203.05 98.24 1202.99 101.00 C1202.95 103.07 1202.16 104.96 1202.05 107.00 C1201.92 109.70 1203.04 112.26 1202.99 115.00 C1202.94 117.72 1202.01 120.25 1202.00 123.00 C1201.99 126.44 1202.92 129.62 1202.99 133.00 C1203.19 141.99 1203.00 151.01 1203.00 160.00 C1203.00 169.97 1201.56 181.14 1203.05 190.99 C1203.49 193.86 1205.47 193.99 1208.00 194.00 C1213.00 194.02 1218.00 194.00 1223.00 194.00 C1224.85 194.00 1228.20 194.60 1229.42 192.79 C1231.17 190.20 1230.00 184.03 1230.00 181.00 C1230.00 174.56 1228.59 166.23 1230.21 160.00 C1230.83 157.60 1233.18 155.91 1234.63 154.00 C1236.14 152.01 1238.39 148.41 1241.00 147.78 C1243.34 147.22 1244.96 150.58 1246.15 152.00 C1249.80 156.34 1253.51 160.65 1257.15 165.00 C1263.00 171.98 1268.71 179.43 1275.00 186.00 C1277.24 188.34 1279.04 191.94 1282.00 193.42 C1286.24 195.55 1294.32 194.00 1299.00 194.00 C1302.66 194.00 1306.34 194.09 1310.00 193.99 C1311.19 193.96 1312.84 193.93 1313.58 192.79 C1314.58 191.26 1312.47 189.12 1311.63 188.00 C1308.56 183.88 1305.14 179.94 1301.85 176.00 C1292.44 164.73 1283.26 153.28 1273.85 142.00 C1270.81 138.36 1267.77 134.77 1264.89 131.00 C1263.81 129.59 1262.04 127.83 1261.74 126.00 C1261.47 124.36 1263.00 123.05 1264.01 122.00 C1266.27 119.66 1268.75 117.49 1270.85 115.00 C1280.07 104.01 1289.95 93.26 1299.99 83.00 C1302.77 80.16 1305.28 76.96 1307.94 74.00 C1309.21 72.60 1310.65 70.52 1307.94 70.05 C1301.05 68.88 1293.00 70.00 1286.00 70.00 C1283.30 70.00 1279.48 69.33 1277.00 70.58 C1274.02 72.07 1272.27 75.67 1269.99 78.00 C1263.91 84.22 1257.97 90.61 1252.04 97.00 C1246.92 102.53 1242.26 108.61 1236.99 114.00 C1235.73 115.29 1233.21 118.85 1231.21 118.85 C1229.94 118.85 1230.03 116.78 1230.01 116.00 C1229.91 112.34 1230.00 108.66 1230.00 105.00 C1230.00 96.67 1230.00 88.33 1230.00 80.00 C1230.00 77.70 1231.00 72.32 1229.42 70.47 C1228.35 69.21 1222.60 69.24 1221.00 69.41Z M406.05 69.42C402.73 70.76 404.54 74.53 405.32 77.00 C407.52 83.95 409.65 90.98 411.63 98.00 C418.44 122.05 425.22 146.23 433.00 170.00 C434.83 175.61 436.17 181.39 438.00 187.00 C438.66 189.03 439.19 192.21 441.16 193.42 C442.75 194.40 445.23 194.00 447.00 194.00 C452.00 194.00 457.00 194.00 462.00 194.00 C464.06 194.00 467.04 194.52 468.58 192.79 C469.95 191.26 470.08 188.88 470.68 187.00 C472.26 181.99 473.94 177.05 475.37 172.00 C479.25 158.27 483.83 144.54 488.33 131.00 C489.54 127.36 490.44 123.64 491.67 120.00 C491.99 119.04 492.59 116.15 494.00 116.15 C495.86 116.15 496.90 121.67 497.32 123.00 C499.84 130.95 501.68 139.09 504.33 147.00 C507.92 157.74 511.48 168.98 514.11 180.00 C514.92 183.38 516.21 186.71 517.33 190.00 C517.73 191.17 518.09 192.69 519.20 193.42 C520.93 194.56 524.04 194.00 526.00 194.00 C531.99 194.00 538.01 194.15 544.00 193.99 C547.77 193.89 548.71 191.26 549.63 188.00 C551.15 182.65 552.85 177.35 554.37 172.00 C561.10 148.22 568.57 124.46 576.33 101.00 C578.75 93.70 580.89 86.31 583.33 79.00 C584.02 76.94 586.08 73.24 585.27 71.05 C584.36 68.62 581.04 69.00 579.00 69.00 C574.00 69.00 569.00 69.00 564.00 69.00 C561.93 69.00 559.05 68.66 557.53 70.42 C554.24 74.22 553.82 82.29 552.32 87.00 C547.68 101.64 543.32 116.36 538.68 131.00 C537.21 135.61 535.89 140.32 534.64 145.00 C534.38 145.97 533.92 149.87 532.15 149.22 C530.86 148.73 530.68 146.11 530.32 145.00 C528.76 140.07 527.09 135.03 525.89 130.00 C521.39 111.15 514.79 92.61 509.37 74.00 C508.87 72.31 508.65 70.52 506.95 69.58 C505.26 68.64 502.85 69.00 501.00 69.00 C496.33 69.00 491.67 68.98 487.00 69.00 C484.23 69.01 481.80 69.18 480.68 72.05 C477.50 80.18 475.44 88.73 472.67 97.00 C468.89 108.29 464.61 119.55 461.37 131.00 C460.03 135.71 458.54 140.35 457.00 145.00 C456.74 145.78 456.17 148.85 455.00 148.85 C453.61 148.85 452.67 144.01 452.33 143.00 C450.37 137.14 449.32 130.96 447.63 125.00 C443.87 111.72 440.84 98.16 436.68 85.00 C435.37 80.89 434.98 73.29 431.80 70.21 C430.08 68.54 427.18 69.00 425.00 69.00 C420.67 69.00 416.33 69.00 412.00 69.00 C410.11 69.00 407.85 68.70 406.05 69.42Z M602.00 65.41C594.55 67.69 590.51 75.60 592.38 83.00 C594.37 90.89 603.67 94.56 611.00 92.32 C618.66 89.98 623.18 81.81 620.58 74.00 C618.13 66.63 609.15 63.23 602.00 65.41Z M21.00 8.41C4.83 13.35 8.00 31.95 8.00 45.00 C8.00 67.33 8.00 89.67 8.00 112.00 C8.00 119.00 8.00 126.00 8.00 133.00 C8.00 136.55 7.52 140.50 8.16 144.00 C10.07 154.42 20.48 158.98 30.00 159.00 C55.33 159.06 80.67 159.00 106.00 159.00 C112.00 159.00 118.00 159.03 124.00 159.00 C126.78 158.99 129.68 158.68 131.16 155.95 C133.27 152.07 132.00 145.31 132.00 141.00 C132.00 132.33 132.00 123.67 132.00 115.00 C132.00 107.08 131.73 99.46 137.16 93.00 C144.65 84.08 155.35 84.00 166.00 84.00 C174.33 84.00 182.67 84.00 191.00 84.00 C195.33 84.00 199.67 84.00 204.00 84.00 C206.18 84.00 208.89 84.27 210.58 82.58 C212.47 80.68 212.00 77.42 212.00 75.00 C212.00 69.00 212.00 63.00 212.00 57.00 C212.00 49.67 212.00 42.33 212.00 35.00 C212.00 24.15 211.51 12.38 199.00 8.41 C195.12 7.18 190.04 8.00 186.00 8.00 C148.00 8.00 110.00 8.00 72.00 8.00 C60.67 8.00 49.33 8.00 38.00 8.00 C32.77 8.00 26.03 6.88 21.00 8.41Z"/>
      </symbol>
      <symbol id="logo-icon" viewBox="0 0 348 252" fill="currentColor">
        <path fill="currentColor" fill-rule="evenodd" d="M233.01 102.58C230.02 104.15 230.00 107.05 230.00 110.00 C230.00 114.33 230.00 118.67 230.00 123.00 C230.00 138.18 234.13 159.84 221.00 170.84 C213.47 177.15 205.26 177.00 196.00 177.00 C187.33 177.00 178.67 177.00 170.00 177.00 C166.00 177.00 162.00 177.00 158.00 177.00 C155.73 177.00 153.19 176.79 151.38 178.42 C147.81 181.64 149.00 187.69 149.00 192.00 C149.00 200.67 149.00 209.33 149.00 218.00 C149.00 224.91 149.30 232.44 154.16 237.96 C161.23 245.97 174.51 244.00 184.00 244.00 C209.67 244.00 235.33 244.00 261.00 244.00 C277.00 244.00 293.00 244.00 309.00 244.00 C317.66 244.00 326.85 244.82 333.84 238.62 C340.55 232.69 340.00 223.06 340.00 215.00 C340.00 196.33 340.00 177.67 340.00 159.00 C340.00 149.33 340.00 139.67 340.00 130.00 C340.00 122.55 340.35 115.31 335.62 109.01 C329.83 101.29 320.54 102.00 312.00 102.00 C293.67 102.00 275.33 102.00 257.00 102.00 C251.67 102.00 246.33 102.00 241.00 102.00 C238.59 102.00 235.21 101.41 233.01 102.58Z M164.00 102.38C161.93 102.88 159.85 103.74 158.00 104.80 C148.03 110.51 149.00 122.15 149.00 132.00 C149.00 138.00 148.93 144.00 149.00 150.00 C149.04 153.74 150.08 157.24 154.00 158.58 C158.08 159.96 163.73 159.00 168.00 159.00 C177.53 159.00 187.66 159.98 197.00 158.15 C199.19 157.71 201.02 157.33 203.00 156.20 C212.49 150.77 212.00 140.44 212.00 131.00 C212.00 124.33 212.02 117.67 212.00 111.00 C211.99 107.05 211.20 103.85 207.00 102.42 C202.92 101.04 197.27 102.00 193.00 102.00 C186.67 102.00 180.33 102.00 174.00 102.00 C170.74 102.00 167.19 101.61 164.00 102.38Z M21.00 8.41C4.83 13.35 8.00 31.95 8.00 45.00 C8.00 67.33 8.00 89.67 8.00 112.00 C8.00 119.00 8.00 126.00 8.00 133.00 C8.00 136.55 7.52 140.50 8.16 144.00 C10.07 154.42 20.48 158.98 30.00 159.00 C55.33 159.06 80.67 159.00 106.00 159.00 C112.00 159.00 118.00 159.03 124.00 159.00 C126.78 158.99 129.68 158.68 131.16 155.95 C133.27 152.07 132.00 145.31 132.00 141.00 C132.00 132.33 132.00 123.67 132.00 115.00 C132.00 107.08 131.73 99.46 137.16 93.00 C144.65 84.08 155.35 84.00 166.00 84.00 C174.33 84.00 182.67 84.00 191.00 84.00 C195.33 84.00 199.67 84.00 204.00 84.00 C206.18 84.00 208.89 84.27 210.58 82.58 C212.47 80.68 212.00 77.42 212.00 75.00 C212.00 69.00 212.00 63.00 212.00 57.00 C212.00 49.67 212.00 42.33 212.00 35.00 C212.00 24.15 211.51 12.38 199.00 8.41 C195.12 7.18 190.04 8.00 186.00 8.00 C148.00 8.00 110.00 8.00 72.00 8.00 C60.67 8.00 49.33 8.00 38.00 8.00 C32.77 8.00 26.03 6.88 21.00 8.41Z"/>
      </symbol>
    </defs>
  </svg>
  <section id="auth-view">
    <div class="auth-shell">
      <section class="auth-story" aria-label="Wingman 简介">
        <div class="brand" aria-label="WingmanKVM"><svg class="brand-logo-full" role="img" aria-label="WingmanKVM"><use href="#logo-full"/></svg><svg class="brand-logo-icon" role="img" aria-label="WingmanKVM"><use href="#logo-icon"/></svg></div>
        <div id="auth-story-home" class="auth-story-copy">
          <h2>随时接管。</h2>
        </div>
        <div id="auth-guide" class="auth-guide hidden" role="region" aria-labelledby="auth-guide-title"><button id="auth-guide-back" class="guide-back" type="button">返回</button><p id="auth-guide-kicker" class="eyebrow"></p><h2 id="auth-guide-title" tabindex="-1"></h2><div id="auth-guide-body" class="auth-guide-list"></div></div>
      </section>
      <section class="auth-panel">
        <div class="auth-card">
          <div id="boot-panel" role="status"><div class="boot-orbit" aria-hidden="true"></div><h2 tabindex="-1">连接中</h2></div>
          <form id="login-form" class="auth-form narrow hidden" aria-busy="false">
            <div class="form-heading"><h2 tabindex="-1">登录</h2></div>
            <div class="grid"><label class="field wide"><span>管理员账号</span><input name="username" autocomplete="username webauthn" required></label><label class="field wide"><span>密码</span><input name="password" type="password" autocomplete="current-password" required aria-describedby="login-error"></label></div>
            <span id="login-error" class="error" role="alert"></span><div class="form-actions"><button class="primary wide-button" type="submit">进入控制台</button></div>
            <div id="passkey-login-container" class="passkey-login-box hidden" style="margin-top:16px;padding-top:16px;border-top:1px solid var(--line);"><button id="passkey-login-btn" class="secondary wide-button" type="button" style="display:flex;align-items:center;justify-content:center;gap:8px;font-weight:500;"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20"/><path d="M2 12h20"/><path d="M12 8a4 4 0 0 0-4 4c0 2 2 4 4 4s4-2 4-4a4 4 0 0 0-4-4z"/></svg><span data-i18n="passkey_signin">使用 Passkey (Touch ID / Face ID) 登录</span></button></div>
            <div id="passkey-insecure-hint" class="hint hidden" style="margin-top:10px;text-align:center;color:var(--amber);"></div>
          </form>
          <form id="setup-form" class="auth-form hidden" aria-busy="false">
            <div class="form-heading"><h2 tabindex="-1">初始化</h2></div>
            <div class="setup-progress" aria-label="初始化进度"><div class="progress-step active" data-progress="0">01 / 管理员</div><div class="progress-step" data-progress="1">02 / 连接</div></div>
            <section class="setup-step active" data-setup-step="0">
              <h3>管理员<button class="guide-help" type="button" data-guide="account" aria-label="管理员设置说明" aria-controls="auth-guide" aria-expanded="false">?</button></h3><p class="setup-step-copy">密码至少 12 位，包含大小写字母、数字和符号。</p>
              <div class="grid"><label id="setup-token-field" class="field wide"><span>初始化令牌</span><input name="setup_token" type="password" autocomplete="off" required><span class="hint">安装完成时会直接显示。</span></label><label class="field"><span>管理员账号</span><input name="username" value="admin" autocomplete="username" pattern="[A-Za-z0-9._\-]{3,64}" title="使用 3–64 位字母、数字、点、下划线或短横线" required></label><label class="field"><span>强密码</span><input name="password" type="password" autocomplete="new-password" minlength="12" required></label><label class="field wide"><span>确认密码</span><input name="confirm_password" type="password" autocomplete="new-password" minlength="12" required></label></div>
              <div class="form-actions"><button class="primary" type="button" data-setup-next>继续</button></div>
            </section>
            <section class="setup-step" data-setup-step="1">
              <h3>检查连接</h3>
              <div class="setup-scan-card"><div><strong>自动检测<button class="guide-help" type="button" data-guide="devices" aria-label="自动检测说明" aria-controls="auth-guide" aria-expanded="false">?</button></strong><span class="hint">视频、USB 控制、虚拟介质、电源</span></div><button id="setup-scan" class="secondary" type="button">重新扫描</button></div>
              <div id="setup-device-results" class="setup-detection-grid" role="status"><div class="capability"><span class="status-dot"></span><strong>等待检测</strong></div></div>
              <label class="auth-choice"><span><strong>虚拟介质<button class="guide-help" type="button" data-guide="media" aria-label="虚拟介质说明" aria-controls="auth-guide" aria-expanded="false">?</button></strong></span><span class="switch"><input name="media_enabled" type="checkbox"><i></i></span></label>
              <details class="setup-advanced"><summary>配置电源按钮（可选）</summary><label class="auth-choice"><span><strong>电源控制</strong></span><span class="switch"><input name="power_enabled" type="checkbox"><i></i></span></label><div id="setup-power-fields" class="setup-option-fields grid"><label class="field"><span>GPIO 控制器</span><input name="gpio_chip" list="setup-gpio-options" placeholder="稍后配置"><datalist id="setup-gpio-options"></datalist></label><label class="field"><span>线路</span><input name="gpio_line" type="number" min="0" placeholder="Line"></label><label class="field wide"><span>继电器触发</span><select name="active_high"><option value="true">高电平</option><option value="false">低电平</option></select></label></div></details>
              <details class="setup-advanced"><summary>手动选择设备</summary><div class="grid"><label class="field"><span>视频设备</span><input name="video_device" placeholder="/dev/video0"></label><label class="field"><span>键盘</span><input name="keyboard_device" placeholder="/dev/hidg0"></label><label class="field"><span>相对鼠标</span><input name="mouse_device" placeholder="/dev/hidg1"></label><label class="field"><span>绝对指针</span><input name="absolute_pointer_device" placeholder="/dev/hidg2"></label><label class="field"><span>指针模式</span><select name="pointer_mode"><option value="absolute">绝对</option><option value="relative">相对</option></select></label><label class="field"><span>镜像目录</span><input name="image_directory" placeholder="/var/lib/wingmankvm/images"></label><label class="field wide"><span>虚拟介质 LUN</span><input name="lun_path" placeholder="/sys/kernel/config/usb_gadget/…/lun.0"></label></div></details>
              <span id="setup-error" class="error" role="alert"></span><div class="form-actions"><button class="secondary" type="button" data-setup-prev>返回</button><span id="setup-readiness" class="setup-readiness"></span><button id="setup-submit" class="primary" type="submit">进入控制台</button></div>
            </section>
          </form>
        </div>
      </section>
    </div>
  </section>

  <main id="app" class="hidden inspector-open">
    <header class="topbar">
      <div class="brand" aria-label="WingmanKVM"><svg class="brand-logo-full" role="img" aria-label="WingmanKVM"><use href="#logo-full"/></svg><svg class="brand-logo-icon" role="img" aria-label="WingmanKVM"><use href="#logo-icon"/></svg></div>
      <button id="diagnostics-toggle" class="topbar-center" type="button" aria-expanded="false" aria-controls="diagnostics-popover" title="点击查看连接与设备诊断"><span id="status-dot" class="status-dot"></span><span id="status-label">正在建立视频链路</span><svg width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M4 6l4 4 4-4"/></svg></button>
      <div class="topbar-actions">
        <div class="power-menu-container">
          <button id="power-menu-toggle" class="icon-button" type="button" aria-expanded="false" aria-controls="power-menu" title="电源与系统控制"><span data-power-led-dot class="status-dot"></span><svg class="power-menu-icon" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18.36 6.64a9 9 0 1 1-12.73 0"/><line x1="12" y1="2" x2="12" y2="12"/></svg><span data-i18n="power">电源</span><svg class="power-chevron" width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M4 6l4 4 4-4"/></svg></button>
          <div id="power-menu" class="dropdown-menu" hidden>
            <div class="dropdown-header"><span data-i18n="power_status">电源状态</span>: <span data-power-led-state>读取中…</span></div>
            <button class="dropdown-item" type="button" data-power-action="press"><span data-i18n="short_press">短按电源开机/关机</span></button>
            <button class="dropdown-item" type="button" data-power-action="reset"><span data-i18n="reset_pc">复位重启 (Reset)</span></button>
            <button class="dropdown-item danger" type="button" data-power-action="force-off"><span data-i18n="force_off">强制关机 (长按 5 秒)</span></button>
          </div>
        </div>
        <div class="lang-menu-container">
          <button id="lang-toggle" class="icon-button" type="button" aria-expanded="false" aria-controls="lang-menu" title="切换界面语言 / Language"><svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/></svg><span id="lang-current-label">简</span></button>
          <div id="lang-menu" class="dropdown-menu" hidden>
            <button class="dropdown-item" type="button" data-lang="zh-CN"><span>简体中文 (默认)</span></button>
            <button class="dropdown-item" type="button" data-lang="zh-TW"><span>繁體中文</span></button>
            <button class="dropdown-item" type="button" data-lang="en"><span>English</span></button>
          </div>
        </div>
        <button id="theme-toggle" class="icon-button" type="button" title="切换深色/浅色模式">
          <svg class="theme-icon-dark" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/></svg>
          <svg class="theme-icon-light hidden" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="5"/><line x1="12" y1="1" x2="12" y2="3"/><line x1="12" y1="21" x2="12" y2="23"/><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/><line x1="1" y1="12" x2="3" y2="12"/><line x1="21" y1="12" x2="23" y2="12"/><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/></svg>
        </button>
        <button id="settings-toggle" class="icon-button" type="button" title="全局偏好设置" aria-controls="settings-dialog" style="display:inline-flex;align-items:center;gap:5px;">
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
          <span data-i18n="settings">设置</span>
        </button>
        <button id="keyboard-toggle" class="icon-button" type="button" title="虚拟键盘" aria-controls="keyboard-dialog">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="4" width="20" height="16" rx="2" ry="2"/><line x1="6" y1="8" x2="6.01" y2="8"/><line x1="10" y1="8" x2="10.01" y2="8"/><line x1="14" y1="8" x2="14.01" y2="8"/><line x1="18" y1="8" x2="18.01" y2="8"/><line x1="6" y1="12" x2="6.01" y2="12"/><line x1="10" y1="12" x2="10.01" y2="12"/><line x1="14" y1="12" x2="14.01" y2="12"/><line x1="18" y1="12" x2="18.01" y2="12"/><line x1="7" y1="16" x2="17" y2="16"/></svg>
          <span data-i18n="keyboard">键盘</span>
        </button>
        <button id="inspector-toggle" class="icon-button" type="button" aria-expanded="true" data-i18n="inspector">控制面板</button>
        <span class="user-pill" aria-hidden="true">WK</span>
      </div>
    </header>
    <div id="diagnostics-popover" class="diagnostics-popover" hidden>
      <div class="diagnostics-head">
        <h3 data-i18n="diag_title">连接与设备诊断</h3>
        <button id="diagnostics-close" class="ghost" type="button" aria-label="关闭诊断面板"><svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 4L4 12M4 4l8 8"/></svg></button>
      </div>
      <div class="diagnostics-grid">
        <div class="diagnostics-card"><span data-i18n="diag_transport">视频传输链路</span><strong id="diag-transport">MJPEG</strong></div>
        <div class="diagnostics-card"><span data-i18n="diag_resolution">采集画面分辨率</span><strong id="diag-resolution">--</strong></div>
        <div class="diagnostics-card"><span data-i18n="diag_input">键鼠控制模式</span><strong id="diag-input-mode">转发已暂停</strong></div>
        <div class="diagnostics-card"><span data-i18n="diag_power_led">被控机电源 LED</span><strong id="diag-power-led">读取中…</strong></div>
        <div class="diagnostics-card" style="grid-column:1/-1;"><span data-i18n="diag_client_ip">客户端来源地址</span><strong id="diag-client-ip">--</strong></div>
      </div>
      <div class="diagnostics-actions">
        <button id="diagnostics-reconnect" class="secondary" type="button" data-i18n="diag_reconnect">重连视频</button>
        <button id="diagnostics-scan" class="secondary" type="button" data-i18n="diag_scan">扫描设备</button>
      </div>
    </div>
    <div class="shell">
      <section id="workspace" class="workspace">
        <div id="console" class="console mode-fit render-pixelated">
          <div id="console-head" class="console-head"><div class="workspace-tabs"><button class="workspace-tab active" type="button" data-workspace="video" data-i18n="tab_remote_screen">远程画面</button><button class="workspace-tab" type="button" data-workspace="terminal" data-i18n="tab_terminal">终端</button></div><div class="quick-keys" aria-label="常用快捷键"><button class="icon-button-sm" type="button" data-quick-key="cad" title="发送 Ctrl+Alt+Del">Ctrl+Alt+Del</button><button class="icon-button-sm" type="button" data-quick-key="win" title="发送 Win 徽标键">Win</button><button class="icon-button-sm" type="button" data-quick-key="alttab" title="发送 Alt+Tab">Alt+Tab</button><button class="icon-button-sm" type="button" data-quick-key="esc" title="发送 Esc">Esc</button></div><span id="video-transport-label" class="console-meta">MJPEG</span><div class="console-actions"><div id="console-leds" class="keyboard-led-strip" aria-label="键盘指示灯" title="键盘锁定指示灯（点击切换）"><button id="led-num" class="led-badge" type="button" data-led="NumLock" title="Num Lock"><span class="led-dot"></span><span class="led-text">1 NUM</span></button><button id="led-caps" class="led-badge" type="button" data-led="CapsLock" title="Caps Lock"><span class="led-dot"></span><span class="led-text">A CAPS</span></button><button id="led-scroll" class="led-badge" type="button" data-led="ScrollLock" title="Scroll Lock"><span class="led-dot"></span><span class="led-text">⤓ SCRL</span></button></div><button id="input-release" class="icon-button-sm danger hidden" type="button" data-i18n="release_keys" title="紧急释放所有按键与鼠标锁定">释放按键</button><button id="input-state" class="input-badge clickable" type="button" role="status" title="点击切换键鼠转发状态">输入已暂停</button><button id="terminal-reconnect" class="secondary hidden" type="button" data-i18n="reconnect">重连</button><button id="terminal-clear" class="secondary hidden" type="button" data-i18n="clear">清空</button><button id="mode-button" class="secondary" type="button" data-i18n="mode_fit">适应</button><button id="fullscreen" class="secondary" type="button" data-i18n="fullscreen">全屏</button></div></div>
          <div id="video-viewport" tabindex="0" aria-label="远程视频与鼠标控制区域">
            <img id="video-feed" class="video-surface" alt="远程设备视频" draggable="false"><video id="webrtc-feed" class="video-surface hidden" autoplay muted playsinline aria-label="远程设备视频"></video><span id="video-message" class="video-message" role="status" aria-live="polite">正在连接视频…</span>
          </div>
          <div id="terminal-window" class="terminal-window" hidden><div id="terminal-host" class="terminal-host" aria-label="RK3399 终端"></div></div>
        </div>
        <button id="inspector-float-open" class="inspector-float-handle" type="button" aria-label="展开控制面板" title="展开控制面板"><svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M10 12L6 8l4-4"/></svg><span data-i18n="inspector">控制面板</span></button>
      </section>
      <div id="inspector-backdrop" class="inspector-backdrop" aria-hidden="true"></div>
      <aside id="inspector" class="inspector">
        <header class="inspector-head"><div><h2 data-i18n="inspector">控制面板</h2></div><button id="inspector-close" class="ghost" type="button" aria-label="收起控制面板" title="收起控制面板 (快捷键: Esc)"><svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 4L4 12M4 4l8 8"/></svg><span data-i18n="inspector_close">收起</span></button></header>
        <nav class="inspector-tabs" aria-label="控制面板"><button class="tab-button active" type="button" data-panel-target="control" data-i18n="tab_control">控制</button><button class="tab-button" type="button" data-panel-target="video" data-i18n="tab_video">视频</button><button class="tab-button" type="button" data-panel-target="devices" data-i18n="tab_devices">设备</button><button class="tab-button" type="button" data-panel-target="media" data-i18n="tab_media">介质</button></nav>
        <div class="inspector-body">
          <section class="inspector-panel" data-panel="control">
            <h3 class="panel-title" data-i18n="remote_control">远程控制</h3>
            <div class="panel-section"><div class="control-card"><div><strong data-i18n="forward_input">转发键鼠</strong></div><label class="switch"><input id="remote-input" type="checkbox" aria-label="转发键盘和鼠标"><i></i></label></div></div>
            <div class="panel-section"><div class="key-strip inspector-keys" aria-label="特殊按键"><button type="button" data-key="Escape">Esc</button><button type="button" data-key="Delete">Del</button><button type="button" data-key="F1">F1</button><button type="button" data-key="F2">F2</button><button type="button" data-key="F3">F3</button><button type="button" data-key="F4">F4</button><button type="button" data-key="F5">F5</button><button type="button" data-key="F6">F6</button><button type="button" data-key="F7">F7</button><button type="button" data-key="F8">F8</button><button type="button" data-key="F9">F9</button><button type="button" data-key="F10">F10</button><button type="button" data-key="F11">F11</button><button type="button" data-key="F12">F12</button></div></div>
            <div class="panel-section">
              <div class="control-card">
                <div>
                  <strong data-i18n="keyboard_leds_title">键盘锁定状态</strong>
                  <p data-i18n="keyboard_leds_desc">远程主机键盘锁定指示灯（点击快速切换）</p>
                </div>
              </div>
              <div class="keyboard-led-strip inspector-led-strip" aria-label="键盘指示灯">
                <button id="inspector-led-num" class="led-badge" type="button" data-led="NumLock" title="Num Lock"><span class="led-dot"></span><span class="led-text">1 NUM</span></button>
                <button id="inspector-led-caps" class="led-badge" type="button" data-led="CapsLock" title="Caps Lock"><span class="led-dot"></span><span class="led-text">A CAPS</span></button>
                <button id="inspector-led-scroll" class="led-badge" type="button" data-led="ScrollLock" title="Scroll Lock"><span class="led-dot"></span><span class="led-text">⤓ SCRL</span></button>
              </div>
            </div>
            <div class="panel-section">
              <div class="power-row">
                <button class="primary" type="button" data-power-action="press" data-i18n="short_press_btn">短按电源</button>
                <button class="secondary" type="button" data-power-action="reset" data-i18n="reset_btn">复位</button>
                <button class="danger power-danger" type="button" data-power-action="force-off" data-i18n="force_off_btn">强制关机</button>
              </div>
              <div class="gpio-led-row" role="status" aria-live="polite"><span id="power-led-dot" data-power-led-dot class="status-dot"></span><span class="gpio-led-label">PWR LED</span><span id="power-led-state" data-power-led-state class="gpio-led-state">读取中…</span><button id="power-led-refresh" data-power-led-refresh class="ghost" type="button" data-i18n="refresh">刷新</button></div>
            </div>
            <div class="panel-section">
              <div class="control-card">
                <div>
                  <strong data-i18n="passkey_title">Passkey 通行密钥</strong>
                  <p data-i18n="passkey_desc">绑定 Apple Touch ID / Face ID，无需输入密码快速登录</p>
                </div>
                <button id="passkey-add-btn" class="primary" type="button" style="min-height:32px;padding:0 12px;font-size:12px;" data-i18n="passkey_add">+ 绑定此设备</button>
              </div>
              <div id="passkey-list-container" style="margin-top:10px;">
                <div id="passkey-empty-hint" class="hint" style="text-align:center;padding:8px 0;color:var(--muted);" data-i18n="passkey_none">尚未绑定任何 Passkey</div>
                <div id="passkey-items" class="media-list hidden" style="max-height:160px;margin-top:6px;"></div>
              </div>
              <div id="passkey-inspector-hint" class="hint hidden" style="margin-top:8px;color:var(--amber);"></div>
            </div>
          </section>
          <section class="inspector-panel" data-panel="video" hidden>
            <h3 class="panel-title" data-i18n="tab_video">视频</h3>
            <div class="video-preset-chips" aria-label="视频预设模式">
              <button type="button" class="preset-chip" data-video-preset="desktop" data-i18n="preset_desktop">桌面优化 (1080p 60fps 8M)</button>
              <button type="button" class="preset-chip" data-video-preset="bios" data-i18n="preset_bios">BIOS / UEFI (720p 30fps MJPEG)</button>
              <button type="button" class="preset-chip" data-video-preset="bandwidth" data-i18n="preset_bandwidth">低带宽省流 (720p 25fps 2M)</button>
            </div>
            <form id="video-config" class="side-grid">
              <div class="settings-heading"><strong>Streaming</strong><span>只控制采集和网络传输，不改变被控机桌面布局。</span></div>
              <label class="field wide"><span>播放方式 <button class="field-help" type="button" data-help="自动优先尝试 WebRTC H.264；不可用时回退到 MJPEG。H.264 延迟和带宽通常更低，MJPEG 兼容性更好。" aria-label="播放方式说明">?</button></span><select name="transport"><option value="auto">自动</option><option value="webrtc_h264">低延迟 H.264</option><option value="mjpeg">兼容 MJPEG</option></select></label>
              <label class="field wide"><span>H.264 码率 <button class="field-help" type="button" data-help="H.264 的目标码率。数值越高画质越好、占用带宽越大；数值越低更省带宽，但画面可能出现块状失真。" aria-label="H.264 码率说明">?</button></span><select name="h264_bitrate_preset"><option value="2000">2 Mbps</option><option value="4000">4 Mbps</option><option value="8000">8 Mbps</option><option value="12000">12 Mbps</option><option value="custom">自定义</option></select></label>
              <label class="field wide h264-bitrate-custom hidden"><span>自定义码率 · Kbps <button class="field-help" type="button" data-help="单位是 Kbps，允许范围为 256–50000。" aria-label="自定义码率说明">?</button></span><input name="h264_bitrate_kbps" type="number" min="256" max="50000"></label>
              <label class="field wide"><span>采集输出尺寸 <button class="field-help" type="button" data-help="只调整采集和网络传输的视频尺寸，不改变被控机的桌面分辨率。桌面分辨率请在被控机系统中设置；可选采集尺寸取决于采集卡支持的格式。" aria-label="采集输出尺寸说明">?</button></span><select name="resolution_preset"><option value="auto">设备默认</option><option value="3840x2160">4K · 3840 × 2160</option><option value="2560x1440">1440p · 2560 × 1440</option><option value="1920x1080">1080p · 1920 × 1080</option><option value="1280x720">720p · 1280 × 720</option><option value="720x480">480p · 720 × 480</option><option value="custom">自定义</option></select></label>
              <label class="field resolution-custom"><span>宽度 <button class="field-help" type="button" data-help="与高度共同决定采集分辨率，建议保持源设备支持的宽高比。" aria-label="宽度说明">?</button></span><input name="width" type="number" min="160" max="7680"></label>
              <label class="field resolution-custom"><span>高度 <button class="field-help" type="button" data-help="与宽度共同决定采集分辨率，建议保持源设备支持的宽高比。" aria-label="高度说明">?</button></span><input name="height" type="number" min="120" max="4320"></label>
              <label class="field wide"><span>帧率 <button class="field-help" type="button" data-help="采集帧率。越高动作越流畅，也会增加采集、编码和带宽负载；设备不支持时可能无法应用。" aria-label="帧率说明">?</button></span><select name="fps_preset"><option value="auto">设备默认</option><option value="60">60 FPS</option><option value="30">30 FPS</option><option value="25">25 FPS</option><option value="24">24 FPS</option><option value="15">15 FPS</option><option value="custom">自定义</option></select></label>
              <label class="field wide fps-custom"><span>自定义帧率 <button class="field-help" type="button" data-help="自定义采集帧率，范围为 1–120 FPS；实际值仍受采集卡能力限制。" aria-label="自定义帧率说明">?</button></span><input name="frames_per_second" type="number" min="1" max="120"></label>
              <label class="field wide"><span>MJPEG 处理 <button class="field-help" type="button" data-help="直通直接转发采集卡 JPEG，CPU 和延迟最低；JPEG 压缩会重新编码，可降低带宽但增加 CPU，且可能损失画质。" aria-label="MJPEG 处理说明">?</button></span><select name="encoding"><option value="mjpeg_passthrough">直通</option><option value="transcode_jpeg">JPEG 压缩</option></select></label>
              <label class="field wide"><span>JPEG 质量 · <output id="quality-value">80</output> <button class="field-help" type="button" data-help="仅在选择“JPEG 压缩”时生效。数值越高画质和带宽越高；数值越低文件更小，但压缩痕迹更明显。" aria-label="JPEG 质量说明">?</button></span><input name="jpeg_quality" type="range" min="20" max="100" value="80"></label>
              <details class="side-advanced wide"><summary>H.264 编码 <button class="field-help" type="button" data-help="这里控制 H.264 的硬件编码器和并发上限。RK3399 建议优先使用硬件编码，软件编码只作为后备。" aria-label="H.264 编码说明">?</button></summary><div class="side-grid"><label class="field wide"><span>编码器 <button class="field-help" type="button" data-help="自动按 Rockchip MPP → V4L2 M2M 选择硬件编码器；硬件不可用时不会默认占用 CPU。手动选择需对应 FFmpeg 编码器存在。" aria-label="编码器说明">?</button></span><select name="h264_encoder"><option value="auto">自动</option><option value="rockchip_mpp">Rockchip MPP</option><option value="v4l2_m2m">V4L2 M2M</option><option value="software">软件编码</option></select></label><label class="field wide"><span>同时连接 <button class="field-help" type="button" data-help="允许同时建立的 H.264/WebRTC 会话数，不是按键连接数；每个会话都会增加 CPU 和内存占用。MJPEG 不受此上限影响。" aria-label="同时连接说明">?</button></span><select name="h264_max_sessions"><option value="1">1</option><option value="2">2</option><option value="3">3</option><option value="4">4</option></select></label><label class="check wide"><input name="h264_allow_software" type="checkbox"><span>允许软件编码 <button class="field-help" type="button" data-help="允许使用 libx264 作为后备或指定编码器。兼容性更好，但 CPU 占用明显更高；RK3399 建议优先硬件编码。" aria-label="允许软件编码说明">?</button></span></label></div></details>
              <details class="side-advanced wide"><summary>采集设备 <button class="field-help" type="button" data-help="留空使用自动检测；手动填写采集卡的 /dev/videoN，需确认它支持 Video Capture 和 MJPG。" aria-label="采集设备说明">?</button></summary><label class="field wide"><span>设备路径</span><input name="device" class="mono" placeholder="/dev/video0"></label></details>
              <div class="settings-heading"><strong>Viewer</strong><span>仅改变浏览器里的显示方式。</span></div>
              <label class="field wide"><span>显示缩放 <button class="field-help" type="button" data-help="适应窗口会等比显示整张画面；1:1 按原像素显示；填满窗口可能变形。这些选项不会改变 HDMI 或采集分辨率。" aria-label="显示缩放说明">?</button></span><select name="display_scale"><option value="fit">适应窗口</option><option value="native">1:1 像素</option><option value="fill">填满窗口</option></select></label>
              <label class="field wide"><span>显示插值 <button class="field-help" type="button" data-help="放大或缩小时的像素处理：像素锐利保留边缘，适合 BIOS 和文字；平滑画面更柔和，但细字可能变糊。" aria-label="显示插值说明">?</button></span><select name="rendering"><option value="pixelated">像素锐利</option><option value="smooth">平滑</option></select></label>
              <div class="side-actions wide"><button class="secondary" data-viewer-fullscreen type="button" data-i18n="fullscreen">进入全屏</button></div>
              <div class="side-actions wide"><button class="primary" type="submit" data-i18n="apply">应用</button></div>
            </form>
          </section>
          <section class="inspector-panel" data-panel="devices" hidden>
            <h3 class="panel-title" data-i18n="tab_devices">设备</h3>
            <div id="device-results" class="setup-detection-grid device-overview" role="status"><div class="capability"><span class="status-dot"></span><strong>等待检测</strong></div></div>
            <div class="side-actions"><button id="scan-devices" class="secondary" type="button" data-i18n="rescan">重新检测</button></div>
            <details class="side-advanced">
              <summary>手动配置</summary>
              <form id="device-config" class="side-grid">
                <label class="field wide"><span>键盘 <button class="field-help" type="button" data-help="填写对应的键盘设备路径，例如 /dev/hidg0。自动检测优先，手动值只用于多个 Gadget 或特殊板卡。" aria-label="键盘设备说明">?</button></span><input name="keyboard_device" class="mono"></label>
                <label class="field wide"><span>相对鼠标 <button class="field-help" type="button" data-help="填写 Boot Mouse 的设备路径，例如 /dev/hidg1。相对模式发送位移，适合只支持传统鼠标的 BIOS/UEFI。" aria-label="相对鼠标说明">?</button></span><input name="mouse_device" class="mono"></label>
                <label class="field wide"><span>绝对指针 <button class="field-help" type="button" data-help="填写绝对指针设备路径。绝对模式会把网页坐标映射到远端，鼠标位置反馈更一致。" aria-label="绝对指针说明">?</button></span><input name="absolute_pointer_device" class="mono"></label>
                <label class="field wide"><span>指针模式 <button class="field-help" type="button" data-help="绝对模式同步网页与远端位置；相对模式用于 BIOS/UEFI，点击画面捕获鼠标，按 Esc 释放。" aria-label="指针模式说明">?</button></span><select name="pointer_mode"><option value="absolute">绝对</option><option value="relative">相对</option></select></label>
                <label class="check wide"><input name="power_enabled" type="checkbox"><span>电源控制 <button class="field-help" type="button" data-help="启用后，POWER SW GPIO 才会出现在控制面板并可执行短按、长按和测试脉冲。" aria-label="电源控制说明">?</button></span></label>
                <div class="gpio-config-title wide"><span>POWER SW <button class="field-help" type="button" data-help="这是输出线路，连接电源继电器或按键模拟电路。保存配置后可用测试按钮发送短脉冲。" aria-label="POWER SW 说明">?</button></span><button class="ghost gpio-inline-test" data-gpio-test="power" type="button">测试</button></div>
                <label class="field"><span>GPIO 芯片 <button class="field-help" type="button" data-help="填写实际连接的 gpiochip 名称，例如 gpiochip1；自动扫描只能发现芯片，不能判断物理接线。" aria-label="GPIO 芯片说明">?</button></span><input name="gpio_chip" class="mono"></label>
                <label class="field"><span>GPIO 线路 <button class="field-help" type="button" data-help="这是 GPIO chip 内的 line offset，不是排针上的物理脚号；请按原理图确认。" aria-label="GPIO 线路说明">?</button></span><input name="gpio_line" type="number" min="0"></label>
                <label class="field wide"><span>继电器触发 <button class="field-help" type="button" data-help="选择继电器实际的触发电平。高电平表示输出 1 时导通，低电平表示输出 0 时导通。" aria-label="继电器触发说明">?</button></span><select name="active_high"><option value="true">高电平</option><option value="false">低电平</option></select></label>
                <div class="gpio-config-block">
                  <div class="gpio-config-title"><span>RESET SW <button class="field-help" type="button" data-help="这是输出线路，连接复位继电器或按键模拟电路。保存配置后可用测试按钮发送短脉冲。" aria-label="RESET SW 说明">?</button></span><button class="ghost gpio-inline-test" data-gpio-test="reset" type="button">测试</button></div>
                  <div class="side-grid gpio-config-fields">
                    <label class="field"><span>GPIO 芯片</span><input name="reset_gpio_chip" class="mono"></label>
                    <label class="field"><span>线路</span><input name="reset_gpio_line" type="number" min="0"></label>
                    <label class="field"><span>极性 <button class="field-help" type="button" data-help="选择复位继电器实际的触发电平；高/低电平必须按电路确认。" aria-label="复位极性说明">?</button></span><select name="reset_active_high"><option value="true">高电平</option><option value="false">低电平</option></select></label>
                    <label class="field"><span>脉冲 · ms <button class="field-help" type="button" data-help="复位输出保持有效的时间，范围 50–2000 ms。" aria-label="复位脉冲说明">?</button></span><input name="reset_pulse_ms" type="number" min="50" max="2000" placeholder="500"></label>
                  </div>
                </div>
                <div class="gpio-config-block">
                  <div class="gpio-config-title"><span>PWR LED <button class="field-help" type="button" data-help="这是输入线路，只读取 LED 状态，不能发送测试脉冲。低电平有效通常表示 LED 亮。" aria-label="PWR LED 说明">?</button></span><span class="gpio-config-status"><span data-power-led-dot class="status-dot"></span><span data-power-led-state>读取中…</span><button class="ghost gpio-inline-test" data-power-led-refresh type="button" data-i18n="refresh">刷新</button></span></div>
                  <div class="side-grid gpio-config-fields">
                    <label class="field"><span>GPIO 芯片</span><input name="power_led_gpio_chip" class="mono"></label>
                    <label class="field"><span>线路</span><input name="power_led_gpio_line" type="number" min="0"></label>
                    <label class="field"><span>极性 <button class="field-help" type="button" data-help="低电平有效表示 GPIO 读到 0 时判定 LED 亮；高电平有效则相反。" aria-label="PWR LED 极性说明">?</button></span><select name="power_led_active_low"><option value="true">低电平有效</option><option value="false">高电平有效</option></select></label>
                    <label class="field"><span>偏置 <button class="field-help" type="button" data-help="输入线路的上拉/下拉设置。没有外部偏置时通常使用上拉；具体以电路为准。" aria-label="PWR LED 偏置说明">?</button></span><select name="power_led_bias"><option value="pull_up">上拉</option><option value="pull_down">下拉</option><option value="disabled">关闭</option><option value="as_is">默认</option></select></label>
                    <label class="field"><span>轮询 · ms <button class="field-help" type="button" data-help="读取 LED 输入的间隔；数值越小反馈越快，也会增加读取次数。" aria-label="PWR LED 轮询说明">?</button></span><input name="power_led_poll_interval_ms" type="number" min="100" max="5000" placeholder="1000"></label>
                    <label class="field"><span>去抖 · ms <button class="field-help" type="button" data-help="要求状态稳定一段时间后才更新，避免 LED 或线路抖动造成误报。" aria-label="PWR LED 去抖说明">?</button></span><input name="power_led_debounce_ms" type="number" min="0" max="5000" placeholder="50"></label>
                  </div>
                </div>
                <div class="side-actions wide"><button class="primary" type="submit" data-i18n="save">保存</button></div>
              </form>
            </details>
            <details class="side-advanced"><summary>诊断信息</summary><pre id="device-diagnostics" class="device-results">尚未扫描</pre></details>
          </section>
          <section class="inspector-panel" data-panel="media" hidden><h3 class="panel-title" data-i18n="tab_media">虚拟介质</h3><div class="panel-section"><div id="media-status" class="media-status" role="status"><span class="status-dot"></span><span class="media-status-copy"><strong>正在读取…</strong></span></div><form id="media-upload" class="upload-zone"><label class="field"><span>上传 ISO / IMG <button class="field-help" type="button" data-help="ISO 通常以只读光驱挂载；IMG 可选择 U 盘模式，并按需读写。" aria-label="上传镜像说明">?</button></span><input name="file" type="file" accept=".iso,.img" required></label><div class="side-actions"><button class="primary" type="submit" data-i18n="upload">上传</button><button id="media-refresh" class="secondary" type="button" data-i18n="refresh">刷新</button></div><div class="upload-progress" role="progressbar" aria-label="上传进度" aria-valuemin="0" aria-valuemax="100" aria-valuenow="0"><i id="upload-bar"></i></div></form><div id="media-list" class="media-list">正在读取…</div></div><details class="side-advanced"><summary>存储设置 <button class="field-help" type="button" data-help="启用虚拟介质需要一个已连接到 Gadget 的 LUN，以及一个用于保存镜像的目录。" aria-label="存储设置说明">?</button></summary><form id="media-config" class="side-grid"><label class="check wide"><input name="enabled" type="checkbox"><span>启用虚拟介质 <button class="field-help" type="button" data-help="启用后，被控机会看到一只 USB 光驱或 U 盘；启用前请确认 Gadget 已提供 Mass Storage LUN。" aria-label="启用虚拟介质说明">?</button></span></label><label class="field wide"><span>LUN 目录 <button class="field-help" type="button" data-help="指向 USB Gadget 的 mass_storage lun.0；可以使用自动检测，也可以手动填写 configfs 路径。" aria-label="LUN 目录说明">?</button></span><input name="lun_path" class="mono" placeholder="/sys/kernel/config/usb_gadget/…/lun.0"></label><label class="field wide"><span>镜像目录 <button class="field-help" type="button" data-help="上传的 ISO/IMG 文件会保存到这里。目录必须允许 WingmanKVM 服务读写。" aria-label="镜像目录说明">?</button></span><input name="image_directory" class="mono" placeholder="/var/lib/wingmankvm/images"></label><div class="side-actions wide"><button id="media-scan" class="secondary" type="button">自动检测</button><button class="primary" type="submit" data-i18n="save">保存</button></div></form></details></section>
        </div>
        <div class="session-row"><span class="session-identity"><span class="session-avatar">WK</span><span style="display:flex;flex-direction:column;line-height:1.2;"><span id="session-user">管理员</span><small id="session-ip" style="font-size:10px;color:var(--muted);font-family:ui-monospace,monospace;"></small></span></span><div style="display:flex;align-items:center;gap:6px;"><button id="settings-inspector-btn" class="ghost" type="button" data-i18n="settings">设置</button><button id="logout" class="ghost" type="button" data-i18n="logout">退出登录</button></div></div>
      </aside>
    </div>
    <dialog id="settings-dialog">
      <div class="settings-modal" role="dialog" aria-modal="true" aria-labelledby="settings-dialog-title">
        <div class="dialog-head">
          <div style="display:flex;align-items:center;gap:8px;">
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="12" cy="12" r="3"/>
              <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/>
            </svg>
            <h3 id="settings-dialog-title" data-i18n="settings_title">全局偏好设置</h3>
          </div>
          <button class="secondary" type="button" data-settings-close data-i18n="close">收起</button>
        </div>
        <div class="settings-dialog-body">
          <div class="settings-group">
            <span class="settings-group-title" data-i18n="settings_group_input">交互与通知</span>
            <div class="settings-item">
              <div class="settings-item-copy">
                <strong data-i18n="settings_suppress_hid">屏蔽 HID 写入超时警告</strong>
                <span data-i18n="settings_suppress_hid_desc">在纯 Linux 环境或被控端未开机时，忽略鼠标与键盘写入超时 (writable timeout) 弹窗警告</span>
              </div>
              <label class="switch"><input type="checkbox" id="setting-suppress-hid-timeout" checked><i></i></label>
            </div>
            <div class="settings-item">
              <div class="settings-item-copy">
                <strong data-i18n="settings_capture_pulse">接管边缘微光脉冲</strong>
                <span data-i18n="settings_capture_pulse_desc">激活键鼠转发时，在远端画面边缘显示光晕视觉反馈</span>
              </div>
              <label class="switch"><input type="checkbox" id="setting-capture-pulse" checked><i></i></label>
            </div>
            <div class="settings-item">
              <div class="settings-item-copy">
                <strong data-i18n="settings_release_toast">按键释放提示</strong>
                <span data-i18n="settings_release_toast_desc">执行「释放按键」或切换控制权时弹出状态提示</span>
              </div>
              <label class="switch"><input type="checkbox" id="setting-release-toast" checked><i></i></label>
            </div>
          </div>
          <div class="settings-group">
            <span class="settings-group-title" data-i18n="settings_group_display">画面与渲染</span>
            <div class="settings-item">
              <div class="settings-item-copy">
                <strong data-i18n="settings_render_pixelated">像素锐利渲染 (Pixelated)</strong>
                <span data-i18n="settings_render_pixelated_desc">近邻采样字符边缘无模糊滤镜，适合 BIOS 与纯文本终端</span>
              </div>
              <label class="switch"><input type="checkbox" id="setting-render-pixelated"><i></i></label>
            </div>
            <div class="settings-item">
              <div class="settings-item-copy">
                <strong data-i18n="settings_auto_inspector">宽屏默认展开面板</strong>
                <span data-i18n="settings_auto_inspector_desc">屏幕宽度充足时，进入控制台默认展开右侧控制面板</span>
              </div>
              <label class="switch"><input type="checkbox" id="setting-auto-inspector" checked><i></i></label>
            </div>
          </div>
          <div class="settings-group">
            <span class="settings-group-title" data-i18n="settings_group_network">网络与代理</span>
            <div class="settings-item">
              <div class="settings-item-copy">
                <strong data-i18n="settings_ppv2">PROXY Protocol v2 (PPv2)</strong>
                <span data-i18n="settings_ppv2_desc">解析并透传反向代理 (HAProxy / Nginx) 转发的真实客户端 IP</span>
              </div>
              <select id="setting-proxy-protocol" style="min-width:110px;padding:6px 10px;font-size:12px;border-radius:6px;background:var(--soft);color:var(--ink);border:1px solid var(--line);">
                <option value="disabled" data-i18n="proxy_disabled">关闭</option>
                <option value="auto" data-i18n="proxy_auto">自动检测</option>
                <option value="required" data-i18n="proxy_required">强制要求</option>
              </select>
            </div>
          </div>
          <div class="settings-group">
            <span class="settings-group-title" data-i18n="settings_group_system">系统与重置</span>
            <div class="settings-item">
              <div class="settings-item-copy">
                <strong data-i18n="settings_reset_btn">恢复默认设置</strong>
                <span data-i18n="settings_reset_desc">清除所有保存在本机的偏好设置并还原为初始默认值</span>
              </div>
              <button id="setting-reset-btn" class="secondary" type="button" style="min-height:30px;padding:0 10px;font-size:12px;" data-i18n="settings_reset_action">还原</button>
            </div>
          </div>
        </div>
      </div>
    </dialog>
    <dialog id="keyboard-dialog"><div class="keyboard-sheet"><div class="dialog-head"><h3 data-i18n="virtual_keyboard">虚拟键盘</h3><button class="secondary" type="button" data-close data-i18n="close">收起</button></div><div id="virtual-keyboard" class="keyboard"></div></div></dialog>
  </main>

  <div id="toast" role="status" aria-live="polite"></div>
  <div id="field-tooltip" role="tooltip" hidden></div>
  <script src="/assets/xterm.js"></script>
  <script src="/assets/xterm-fit.js"></script>
  <script>
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
        diag_title:'连接与设备诊断',diag_transport:'视频传输链路',diag_resolution:'采集画面分辨率',diag_input:'键鼠控制模式',diag_power_led:'被控机电源 LED',diag_client_ip:'客户端来源地址',diag_reconnect:'重连视频',diag_scan:'扫描设备',
        tab_remote_screen:'远程画面',tab_terminal:'终端',release_keys:'释放按键',reconnect:'重连',clear:'清空',mode_fit:'适应',fullscreen:'全屏',exit_fullscreen:'退出全屏',
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
        led_num:'数字锁定 (Num Lock)',led_caps:'大写锁定 (Caps Lock)',led_scroll:'滚动锁定 (Scroll Lock)'
      },
      'zh-TW': {
        power:'電源',power_status:'電源狀態',short_press:'短按電源開機/關機',reset_pc:'重置重啟 (Reset)',force_off:'強制關機 (長按 5 秒)',
        keyboard:'鍵盤',inspector:'控制面板',inspector_close:'收起',
        diag_title:'連線與設備診斷',diag_transport:'視訊傳輸鏈路',diag_resolution:'採集畫面解析度',diag_input:'鍵鼠控制模式',diag_power_led:'被控機電源 LED',diag_client_ip:'客戶端來源位址',diag_reconnect:'重新連線視訊',diag_scan:'掃描設備',
        tab_remote_screen:'遠端畫面',tab_terminal:'終端機',release_keys:'釋放按鍵',reconnect:'重新連線',clear:'清除',mode_fit:'適應',fullscreen:'全螢幕',exit_fullscreen:'退出全螢幕',
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
        led_num:'數字鎖定 (Num Lock)',led_caps:'大寫鎖定 (Caps Lock)',led_scroll:'捲動鎖定 (Scroll Lock)'
      },
      'en': {
        power:'Power',power_status:'Power Status',short_press:'Power On / Off (Short Press)',reset_pc:'System Reset (Reset)',force_off:'Force Power Off (Hold 5s)',
        keyboard:'Keyboard',inspector:'Inspector',inspector_close:'Collapse',
        diag_title:'Diagnostics',diag_transport:'Video Transport',diag_resolution:'Capture Resolution',diag_input:'Input Mode',diag_power_led:'Target Power LED',diag_client_ip:'Client Address',diag_reconnect:'Reconnect Video',diag_scan:'Scan Devices',
        tab_remote_screen:'Remote Screen',tab_terminal:'Terminal',release_keys:'Release Keys',reconnect:'Reconnect',clear:'Clear',mode_fit:'Fit',fullscreen:'Fullscreen',exit_fullscreen:'Exit Fullscreen',
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
        led_num:'Num Lock',led_caps:'Caps Lock',led_scroll:'Scroll Lock'
      }
    };
    let currentLang = 'zh-CN';
    function t(key, fallback = '') { return I18N[currentLang]?.[key] ?? I18N['zh-CN']?.[key] ?? fallback ?? key; }
    function updateI18nElements() {
      $$('[data-i18n]').forEach(el => {
        const key = el.dataset.i18n;
        if (key && I18N[currentLang]?.[key]) el.textContent = I18N[currentLang][key];
      });
      const numTitle = t('led_num', 'Num Lock'), capsTitle = t('led_caps', 'Caps Lock'), scrlTitle = t('led_scroll', 'Scroll Lock');
      $('#led-num')?.setAttribute('title', numTitle);
      $('#led-caps')?.setAttribute('title', capsTitle);
      $('#led-scroll')?.setAttribute('title', scrlTitle);
      $('#inspector-led-num')?.setAttribute('title', numTitle);
      $('#inspector-led-caps')?.setAttribute('title', capsTitle);
      $('#inspector-led-scroll')?.setAttribute('title', scrlTitle);
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
    window.addEventListener('resize', invalidateSurfaceBox, { passive: true });
    window.addEventListener('scroll', invalidateSurfaceBox, { passive: true });
    document.addEventListener('fullscreenchange', invalidateSurfaceBox);
    let bootstrap = {}, remoteWanted = false, pageActive = !document.hidden, reconnectTimer = 0, reconnectDelay = 500;
    let videoGeneration = 0, videoWorkspace = 'video', videoPreference = 'auto', activeVideoTransport = 'none', videoUiState = 'paused', videoFallback = false;
    let webrtcPeer = null, webrtcSessionId = null, webrtcAbort = null, videoFirstFrameTimer = 0, statusTimer = 0, latestVideoStatus = null, latestPowerStatus = null, videoConfigRequestId = 0;
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
      if (response.status === 401 && url !== '/api/login' && !app.classList.contains('hidden')) { bootstrap.authenticated=false; stopInput(); showState('login'); $('#login-error').textContent='会话已过期，请重新登录。'; }
      if (!response.ok) throw new Error((data && (data.error || data.message)) || data || `请求失败 (${response.status})`); return data;
    }
    const rawValue = (form, name) => new FormData(form).get(name)?.toString() || '';
    const value = (form, name) => rawValue(form, name).trim();
    const optionalNumber = v => v === '' ? null : Number(v);
    function hardwareFrom(form) { const mouse=value(form,'mouse_device')||null,absolute=value(form,'absolute_pointer_device')||null,selected=value(form,'pointer_mode')==='relative'?'relative':'absolute';return {video_device:value(form,'video_device')||null,keyboard_device:value(form,'keyboard_device')||null,mouse_device:mouse,absolute_pointer_device:absolute,pointer_mode:selected,power_enabled:form.elements.power_enabled.checked,gpio_chip:value(form,'gpio_chip')||null,gpio_line:optionalNumber(value(form,'gpio_line')),active_high:value(form,'active_high')!=='false',media_enabled:form.elements.media_enabled.checked,lun_path:value(form,'lun_path')||null,image_directory:value(form,'image_directory')||null}; }
    function showState(state) {
      $('#boot-panel').classList.add('hidden'); setupForm.classList.toggle('hidden', state !== 'setup'); loginForm.classList.toggle('hidden', state !== 'login'); authView.classList.toggle('hidden', state === 'main'); app.classList.toggle('hidden', state !== 'main');if(state!=='setup')closeSetupGuide(false);
      if (state === 'main') {
        pageActive=!document.hidden; applyBootstrap(); startVideo(); refreshStatus(); scanDevices();
        let defaultOpen=!matchMedia('(max-width:900px)').matches;
        try{const saved=localStorage.getItem('wingman_inspector_open');if(saved!==null)defaultOpen=saved==='true';}catch(_){}
        setInspectorOpen(defaultOpen);
        if(bootstrap.capabilities?.mass_storage)refreshMedia();else $('#media-list').textContent='虚拟介质尚未配置';
        refreshPasskeys();
        requestAnimationFrame(()=>viewport.focus({preventScroll:true}));
      }
      else { stopInput(); stopVideo(); if (state === 'login') updateLoginPasskeyUI(); requestAnimationFrame(()=>$(state==='setup'?'#setup-form h2':'#login-form h2')?.focus()); }
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
      try { const payload = {username:value(setupForm,'username'), password:rawValue(setupForm,'password'), setup_token:rawValue(setupForm,'setup_token'), ...hardwareFrom(setupForm)}; await request('/api/setup',{method:'POST',body:JSON.stringify(payload)}); toast('初始化完成，正在建立控制台'); bootstrap = await request('/api/bootstrap'); showState(bootstrap.authenticated?'main':'login'); }
      catch (e) { error.textContent = e.message;if(e.message.includes('令牌'))$('#setup-token-field').classList.remove('hidden'); } finally { button.disabled = false; button.textContent=label; setupForm.setAttribute('aria-busy','false'); }
    });
    loginForm.addEventListener('submit', async event => {
      event.preventDefault(); const button = $('button[type=submit]', loginForm), error = $('#login-error'), label=button.textContent; button.disabled = true; button.textContent='正在连接…'; loginForm.setAttribute('aria-busy','true'); error.textContent = '';
      try { await request('/api/login',{method:'POST',body:JSON.stringify({username:value(loginForm,'username'),password:rawValue(loginForm,'password')})}); bootstrap = await request('/api/bootstrap'); showState(bootstrap.authenticated?'main':'login'); }
      catch (e) { error.textContent = e.message; } finally { button.disabled = false; button.textContent=label; loginForm.setAttribute('aria-busy','false'); }
    });
    $('#logout').addEventListener('click', async () => { await stopInput(); try { await request('/api/logout',{method:'POST'}); } finally { bootstrap.authenticated=false; try { bootstrap = await request('/api/bootstrap'); } catch(_) {} showState('login'); } });

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
      $('#mode-button').textContent=next[2];$('#mode-button').title=`切换显示方式：适应窗口 / 1:1 像素 / 填满窗口`;$('#video-config').elements.display_scale.value=next[0];
      if(remember)saveVideoUiPreference('scale',next[0]);
      invalidateSurfaceBox();
    }
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
      const w=latestVideoStatus?.width,h=latestVideoStatus?.height,fps=latestVideoStatus?.frames_per_second;
      $('#diag-resolution').textContent=w&&h?`${w} × ${h}${fps?` @ ${Math.round(fps)} FPS`:''}`:'--';
      $('#diag-input-mode').textContent=!inputEnabled()?'已暂停':absoluteMode()?'绝对指针 (USB Tablet)':relativeCaptured()?'相对鼠标 (已捕获)':'相对鼠标 (未捕获)';
      const led=latestPowerStatus?.power_led;
      const ledOn=led?.state==='on'||led?.active===true;
      const ledOff=led?.state==='off'||led?.active===false;
      $('#diag-power-led').textContent=ledOn?'已开机 (LED 亮)':ledOff?'已关机 (LED 灭)':led?.configured===false?'未配置':'未知';
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
      event.preventDefault(); const f=event.currentTarget,button=$('button[type=submit]',f);if(button.disabled)return;syncVideoPresets();const requestId=++videoConfigRequestId,label=button.textContent,payload={video:{device:value(f,'device')||null,width:optionalNumber(value(f,'width')),height:optionalNumber(value(f,'height')),frames_per_second:optionalNumber(value(f,'frames_per_second')),encoding:value(f,'encoding'),jpeg_quality:Number(value(f,'jpeg_quality')),h264:{bitrate_kbps:Number(value(f,'h264_bitrate_kbps')),encoder:value(f,'h264_encoder'),allow_software:f.elements.h264_allow_software.checked,max_sessions:Number(value(f,'h264_max_sessions'))}}};
      button.disabled=true;button.textContent='应用中…';stopVideo();
      try { await request('/api/config',{method:'POST',body:JSON.stringify(payload)});if(requestId!==videoConfigRequestId)return;bootstrap=await request('/api/bootstrap');applyBootstrap();toast('视频设置已应用'); }
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
    function renderVideoUi(){
      const dot=$('#status-dot'),label=$('#status-label'),meta=$('#video-transport-label'),mode=activeVideoTransport==='webrtc_h264'?'H.264':activeVideoTransport==='mjpeg'?'MJPEG':'视频';
      const serverState=latestVideoStatus?.state,serverUnavailable=serverState&&!["ready","starting"].includes(serverState);
      dot.classList.toggle('online',videoUiState==='playing'&&!serverUnavailable);dot.classList.toggle('warning',videoUiState==='connecting'||videoUiState==='fallback'||serverUnavailable);
      if(!shouldVideoRun()){label.textContent='视频已暂停';meta.textContent='暂停';return;}
      if(serverUnavailable){label.textContent=latestVideoStatus?.message||'设备离线';meta.textContent=mode;return;}
      if(videoUiState==='playing'){const fps=latestVideoStatus?.frames_per_second;label.textContent=videoFallback?'MJPEG · H.264 回退':`${mode}${fps?` · ${Math.round(fps)} FPS`:''}`;meta.textContent=`LIVE / ${mode}`;return;}
      if(videoUiState==='fallback'){label.textContent='MJPEG · H.264 回退';meta.textContent='MJPEG';return;}
      label.textContent=videoUiState==='connecting'?`正在连接 ${mode}…`:'视频连接中';meta.textContent=mode;updateDiagnosticsCard();
    }
    function setVideoState(state,transport=activeVideoTransport){activeVideoTransport=transport;videoUiState=state;renderVideoUi();}
    function clearVideoTimers(){clearTimeout(reconnectTimer);clearTimeout(videoFirstFrameTimer);reconnectTimer=0;videoFirstFrameTimer=0;}
    function closeWebrtcSession(sessionId){if(!sessionId)return;fetch('/api/webrtc/close',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({session_id:sessionId}),credentials:'same-origin',keepalive:true}).catch(()=>{});}
    function cleanupWebrtc(){const peer=webrtcPeer,sessionId=webrtcSessionId,abort=webrtcAbort;webrtcPeer=null;webrtcSessionId=null;webrtcAbort=null;abort?.abort();if(peer){peer.ontrack=null;peer.onconnectionstatechange=null;peer.oniceconnectionstatechange=null;peer.getReceivers().forEach(receiver=>receiver.track?.stop());peer.close();}if(sessionId)closeWebrtcSession(sessionId);webrtcFeed.onplaying=null;webrtcFeed.onloadedmetadata=null;webrtcFeed.pause();webrtcFeed.srcObject=null;}
    function stopVideo(showPaused=true){videoGeneration++;clearVideoTimers();reconnectDelay=500;cleanupWebrtc();feed.onload=null;feed.onerror=null;feed.removeAttribute('src');showVideoSurface('none');activeVideoTransport='none';videoFallback=false;videoUiState='paused';if(showPaused)renderVideoUi();}
    function restartVideo(){stopVideo(false);if(shouldVideoRun())startVideo();}
    function markVideoPlaying(generation,transport){if(generation!==videoGeneration||!shouldVideoRun()||activeVideoTransport!==transport)return;clearTimeout(videoFirstFrameTimer);setVideoState('playing',transport);setVideoMessage('',false);}
    function startMjpeg(generation,fallback=false){
      if(generation!==videoGeneration||!shouldVideoRun())return;
      activeVideoTransport='mjpeg';videoFallback=fallback||videoFallback;setVideoState(videoFallback?'fallback':'connecting','mjpeg');setVideoMessage(videoFallback?'H.264 不可用，切换 MJPEG…':'正在连接 MJPEG…');showVideoSurface('mjpeg');feed.onload=null;feed.onerror=null;feed.removeAttribute('src');
      const url=`/video_feed?t=${Date.now()}`;
      const onLoad=()=>{if(generation!==videoGeneration||!shouldVideoRun()||feed.getAttribute('src')!==url)return;reconnectDelay=500;markVideoPlaying(generation,'mjpeg');};
      const onError=()=>{if(generation!==videoGeneration||!shouldVideoRun()||feed.getAttribute('src')!==url)return;feed.onload=null;feed.onerror=null;feed.removeAttribute('src');setVideoState(videoFallback?'fallback':'connecting','mjpeg');setVideoMessage('视频断开，正在重连…');clearTimeout(reconnectTimer);const delay=reconnectDelay;reconnectTimer=setTimeout(()=>{if(generation===videoGeneration&&shouldVideoRun())startMjpeg(generation,videoFallback);},delay);reconnectDelay=Math.min(reconnectDelay*2,10000);};
      feed.onload=onLoad;feed.onerror=onError;feed.setAttribute('src',url);clearTimeout(videoFirstFrameTimer);videoFirstFrameTimer=setTimeout(onError,6000);
    }
    function waitForIceGathering(peer,signal,timeout=2500){if(peer.iceGatheringState==='complete')return Promise.resolve();return new Promise((resolve,reject)=>{let timer=0;const finish=(error)=>{clearTimeout(timer);peer.removeEventListener('icegatheringstatechange',changed);signal?.removeEventListener('abort',aborted);error?reject(error):resolve();};const changed=()=>{if(peer.iceGatheringState==='complete')finish();};const aborted=()=>finish(new Error('视频连接已取消'));peer.addEventListener('icegatheringstatechange',changed);signal?.addEventListener('abort',aborted,{once:true});timer=setTimeout(()=>finish(new Error('视频连接超时')),timeout);});}
    function fallbackToMjpeg(generation){if(generation!==videoGeneration||!shouldVideoRun()||activeVideoTransport!=='webrtc_h264')return;cleanupWebrtc();startMjpeg(generation,true);}
    async function startWebrtc(generation){
      if(generation!==videoGeneration||!shouldVideoRun())return;
      activeVideoTransport='webrtc_h264';setVideoState('connecting','webrtc_h264');setVideoMessage('正在连接 H.264…');showVideoSurface('webrtc_h264');
      const controller=new AbortController();webrtcAbort=controller;let peer=null,signalTimer=0;
      try {
        peer=new RTCPeerConnection({iceServers:bootstrap.ice_servers||[]});if(generation!==videoGeneration||!shouldVideoRun()){peer.close();return;}webrtcPeer=peer;
        const transceiver=peer.addTransceiver('video',{direction:'recvonly'});if(transceiver?.setCodecPreferences&&globalThis.RTCRtpReceiver?.getCapabilities){const codecs=RTCRtpReceiver.getCapabilities('video')?.codecs||[],h264=codecs.filter(codec=>String(codec.mimeType||'').toLowerCase()==='video/h264');if(h264.length)transceiver.setCodecPreferences([...h264,...codecs.filter(codec=>!h264.includes(codec))]);}
        peer.ontrack=event=>{if(generation!==videoGeneration||!shouldVideoRun()||activeVideoTransport!=='webrtc_h264'||event.track.kind!=='video')return;const stream=event.streams?.[0]||new MediaStream([event.track]);webrtcFeed.srcObject=stream;webrtcFeed.onplaying=()=>markVideoPlaying(generation,'webrtc_h264');webrtcFeed.onloadedmetadata=()=>setDisplayScale(displayModes[modeIndex][0]);event.track.onended=()=>fallbackToMjpeg(generation);showVideoSurface('webrtc_h264');webrtcFeed.play().catch(()=>{});};
        peer.onconnectionstatechange=()=>{if(generation!==videoGeneration||peer!==webrtcPeer)return;if(peer.connectionState==='failed'||peer.connectionState==='closed')fallbackToMjpeg(generation);else if(peer.connectionState==='disconnected'){clearTimeout(reconnectTimer);reconnectTimer=setTimeout(()=>{if(generation===videoGeneration&&peer===webrtcPeer&&peer.connectionState==='disconnected')fallbackToMjpeg(generation);},2000);}else clearTimeout(reconnectTimer);};
        peer.oniceconnectionstatechange=()=>{if(generation!==videoGeneration||peer!==webrtcPeer)return;if(peer.iceConnectionState==='failed')fallbackToMjpeg(generation);};
        const offer=await peer.createOffer();await peer.setLocalDescription(offer);await waitForIceGathering(peer,controller.signal);if(generation!==videoGeneration||!shouldVideoRun())return;
        signalTimer=setTimeout(()=>controller.abort(),8000);const answer=await request('/api/webrtc/offer',{method:'POST',body:JSON.stringify({type:'offer',sdp:peer.localDescription?.sdp||''}),signal:controller.signal});clearTimeout(signalTimer);signalTimer=0;if(generation!==videoGeneration||!shouldVideoRun()){closeWebrtcSession(answer?.session_id);return;}webrtcSessionId=answer?.session_id||null;if(!answer?.sdp||!/H264\/90000/i.test(answer.sdp))throw new Error('未协商 H.264');clearTimeout(videoFirstFrameTimer);videoFirstFrameTimer=setTimeout(()=>fallbackToMjpeg(generation),6000);await peer.setRemoteDescription({type:answer.type||'answer',sdp:answer.sdp});
      } catch(error) {clearTimeout(signalTimer);if(generation!==videoGeneration||!shouldVideoRun()){if(peer&&peer!==webrtcPeer)peer.close();return;}fallbackToMjpeg(generation);}
    }
    function startVideo(force=false){if(!shouldVideoRun())return;if(force)stopVideo(false);else if(videoUiState!=='paused')return;const generation=++videoGeneration,canH264=h264Available();clearVideoTimers();videoFallback=false;reconnectDelay=500;showVideoSurface('none');setVideoMessage('正在连接视频…');if(videoPreference==='mjpeg'||!canH264)startMjpeg(generation,videoPreference==='webrtc_h264'&&!canH264);else startWebrtc(generation);}
    $('#mode-button').addEventListener('click',()=>setDisplayScale(displayModes[(modeIndex+1)%displayModes.length][0],true));
    async function toggleFullscreen(){try{if(document.fullscreenElement)await document.exitFullscreen();else await consoleBox.requestFullscreen();}catch(e){toast(e.message,true);}}
    $$('#fullscreen,[data-viewer-fullscreen]').forEach(button=>button.addEventListener('click',toggleFullscreen));
    document.addEventListener('fullscreenchange',()=>{$$('[data-viewer-fullscreen]').forEach(button=>button.textContent=document.fullscreenElement?t('exit_fullscreen','退出全屏'):t('fullscreen','进入全屏'));if(!document.fullscreenElement)$('#console-head')?.classList.remove('revealed');});
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
      drag = { x: event.clientX - r.left, y: event.clientY - r.top };
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
    function relativeCaptured(){return document.pointerLockElement===viewport;}
    function pulseViewport(){if(!getSetting('capture_pulse',true))return;viewport.classList.remove('just-captured');requestAnimationFrame(()=>{requestAnimationFrame(()=>{viewport.classList.add('just-captured');setTimeout(()=>viewport.classList.remove('just-captured'),450);});});}
    function syncInputUi(){const enabled=inputEnabled(),relative=enabled&&!absoluteMode(),captured=relative&&relativeCaptured();$$('#remote-input,.remote-input-mirror').forEach(input=>input.checked=remoteWanted);viewport.classList.toggle('remote',enabled);$('#input-state').classList.toggle('active',enabled);$('#input-release')?.classList.toggle('hidden',!enabled);$('#input-state').textContent=!enabled?t('input_paused','输入已暂停'):relative?(captured?t('input_captured','相对鼠标已捕获'):t('input_click_to_capture','点击画面捕获鼠标')):t('input_forwarding','键鼠正在转发');updateDiagnosticsCard();}
    function setRemote(enabled){remoteWanted=enabled;if(!enabled&&relativeCaptured())document.exitPointerLock();syncInputUi();if(!enabled)releaseAll();else pulseViewport();}
    $$('#remote-input,.remote-input-mirror').forEach(input=>input.addEventListener('change',event=>setRemote(event.target.checked)));
    $('#input-state')?.addEventListener('click',()=>{setRemote(!remoteWanted);if(getSetting('release_key_toast',true))toast(remoteWanted?t('toast_input_started','已开启键鼠转发'):t('toast_input_paused','已暂停键鼠转发'));});
    $('#input-release')?.addEventListener('click',async()=>{await releaseAll();if(relativeCaptured())document.exitPointerLock();if(getSetting('release_key_toast',true))toast(t('input_released','已释放所有按键与鼠标锁定'));});
    async function sendKey(key, event={}) {
      if (!inputEnabled()) { toast(t('input_paused', '请先开启“转发键鼠”'), true); return; }
      const res = await request('/api/key', { method: 'POST', body: JSON.stringify({ key, ctrl: !!event.ctrlKey, shift: !!event.shiftKey, alt: !!event.altKey, meta: !!event.metaKey, hold_ms: key.startsWith('F') ? 100 : 25 }) });
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
    document.addEventListener('keydown',event=>{if(!inputEnabled()||interactiveTarget(event.target)||event.repeat)return;const key=normalizeKey(event);if(!key)return;event.preventDefault();sendKey(key,event).catch(handleHidError);});
    function normalizeKey(event){const code=event.code;if(/^Key[A-Z]$/.test(code)||/^Digit[0-9]$/.test(code)||/^F(?:[1-9]|1[0-2])$/.test(code)||['Escape','Delete','Enter','Tab','Backspace','Space','ArrowUp','ArrowDown','ArrowLeft','ArrowRight','Home','End','PageUp','PageDown','Insert','Minus','Equal','BracketLeft','BracketRight','Backslash','Semicolon','Quote','Backquote','Comma','Period','Slash','CapsLock'].includes(code))return code;return null;}
    $$('.key-strip [data-key]').forEach(button=>button.addEventListener('click',()=>sendKey(button.dataset.key).catch(handleHidError)));
    function releaseAll(){ mouseX=mouseY=0;absolutePending=null;if(app.classList.contains('hidden')||bootstrap.authenticated===false)return Promise.resolve(); return request('/api/input/release-all',{method:'POST',keepalive:true}).catch(()=>{}); }
    function stopInput(){pageActive=false;remoteWanted=false;if(relativeCaptured())document.exitPointerLock();syncInputUi();return releaseAll();}
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
    viewport.addEventListener('pointermove',event=>{if(!inputEnabled())return;if(absoluteMode()){const point=mapAbsolute(event.clientX,event.clientY);absolutePending=point;if(!point)return;}else{if(!relativeCaptured())return;mouseX+=event.movementX;mouseY+=event.movementY;}scheduleMouse();});
    viewport.addEventListener('pointerdown',event=>{
      if(event.button>2)return;
      if(!inputEnabled()){
        setRemote(true);
        if(getSetting('release_key_toast',true))toast('已激活键鼠转发 (点击状态栏或关闭开关可暂停)');
        viewport.focus({preventScroll:true});
        return;
      }
      event.preventDefault();viewport.focus({preventScroll:true});
      if(!absoluteMode()&&!relativeCaptured()){viewport.requestPointerLock?.();pulseViewport();return;}
      const button=event.button===0?1:event.button===2?2:4;
      if(absoluteMode()){const point=mapAbsolute(event.clientX,event.clientY);absolutePending=point;if(!point)return;request('/api/mouse/absolute',{method:'POST',body:JSON.stringify({action:'click',...point,button})}).catch(handleHidError);}
      else request('/api/mouse/click',{method:'POST',body:JSON.stringify({button})}).catch(handleHidError);
    });
    viewport.addEventListener('contextmenu',event=>{if(inputEnabled())event.preventDefault();});
    viewport.addEventListener('wheel',event=>{if(!inputEnabled()||(!absoluteMode()&&!relativeCaptured()))return;event.preventDefault();const wheel=Math.max(-127,Math.min(127,Math.round(-event.deltaY/40)||Math.sign(-event.deltaY)));if(absoluteMode()){const point=mapAbsolute(event.clientX,event.clientY);absolutePending=point;if(!point)return;request('/api/mouse/absolute',{method:'POST',body:JSON.stringify({action:'scroll',...point,delta:wheel})}).catch(handleHidError);}else request('/api/mouse/scroll',{method:'POST',body:JSON.stringify({wheel})}).catch(handleHidError);},{passive:false});
    function scheduleMouse(){const pending=absoluteMode()?absolutePending:mouseX||mouseY;if(mouseBusy||mouseTimer||!pending)return;const wait=Math.max(0,16-(performance.now()-lastMouseSend));mouseTimer=setTimeout(flushMouse,wait);}
    async function flushMouse(){mouseTimer=0;if(mouseBusy||!inputEnabled())return;let url,body;if(absoluteMode()){if(!absolutePending)return;const point=absolutePending;absolutePending=null;url='/api/mouse/absolute';body={action:'move',...point};}else{const dx=Math.max(-127,Math.min(127,mouseX)),dy=Math.max(-127,Math.min(127,mouseY));if(!dx&&!dy)return;mouseX-=dx;mouseY-=dy;url='/api/mouse/move';body={dx,dy};}mouseBusy=true;lastMouseSend=performance.now();try{await request(url,{method:'POST',body:JSON.stringify(body)});}catch(e){handleHidError(e);}finally{mouseBusy=false;scheduleMouse();}}

    const rows=[['Escape','F1','F2','F3','F4','F5','F6','F7','F8','F9','F10','F11','F12','Delete'],['`','1','2','3','4','5','6','7','8','9','0','-','=','Backspace'],['Tab','Q','W','E','R','T','Y','U','I','O','P','[',']','\\'],['CapsLock','A','S','D','F','G','H','J','K','L',';','\'','Enter'],['Shift','Z','X','C','V','B','N','M',',','.','/','Shift'],['Control','Meta','Alt','Spacebar','Alt','Meta','Control']];
    const vk=$('#virtual-keyboard');rows.forEach(row=>{const line=document.createElement('div');line.className='keyboard-row';row.forEach(key=>{const b=document.createElement('button');b.type='button';b.textContent=key==='Spacebar'?'空格':key;b.dataset.key=key;if(key==='Spacebar')b.className='grow';b.addEventListener('click',()=>sendKey(key).catch(handleHidError));line.append(b);});vk.append(line);});
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
      if(terminalSocket&&terminalSocket.readyState<=WebSocket.OPEN)return;
      const scheme=location.protocol==='https:'?'wss':'ws';const socket=new WebSocket(`${scheme}://${location.host}/api/terminal/ws`);terminalSocket=socket;terminalLastSize='';socket.binaryType='arraybuffer';
      socket.onopen=()=>{if(terminalSocket!==socket)return;$('#terminal-reconnect')?.classList.add('hidden');scheduleTerminalFit();requestAnimationFrame(()=>{terminalResize(terminal.cols,terminal.rows);terminal.focus();});};
      socket.onmessage=event=>terminal.write(typeof event.data==='string'?event.data:new Uint8Array(event.data));
      socket.onerror=()=>terminal.writeln('\r\n\x1b[31m终端连接失败\x1b[0m');
      socket.onclose=()=>{if(terminalSocket!==socket)return;terminalSocket=null;if(videoWorkspace==='terminal')$('#terminal-reconnect')?.classList.remove('hidden');};
    }
    function terminalDisconnect(){if(terminalSocket){const socket=terminalSocket;terminalSocket=null;socket.close();}$('#terminal-reconnect')?.classList.remove('hidden');}
    $('#terminal-reconnect')?.addEventListener('click',terminalConnect);
    $('#terminal-clear')?.addEventListener('click',()=>terminal?.clear());
    const keyboardDialog=$('#keyboard-dialog');$$('#keyboard-toggle,#keyboard-toggle-mobile').forEach(button=>button.addEventListener('click',()=>{if(keyboardDialog.open)keyboardDialog.close();else keyboardDialog.show();}));$('[data-close]',keyboardDialog).addEventListener('click',()=>keyboardDialog.close());

    function setWorkspaceMode(mode){const next=mode==='terminal'?'terminal':'video',terminalMode=next==='terminal',changed=videoWorkspace!==next;videoWorkspace=next;invalidateSurfaceBox();$('#terminal-window').hidden=!terminalMode;$('#video-viewport').hidden=terminalMode;$$('[data-workspace]').forEach(tab=>tab.classList.toggle('active',tab.dataset.workspace===next));$('#terminal-clear')?.classList.toggle('hidden',!terminalMode);if(!terminalMode)$('#terminal-reconnect')?.classList.add('hidden');else if(!terminalSocket||terminalSocket.readyState>WebSocket.OPEN)$('#terminal-reconnect')?.classList.remove('hidden');$('.quick-keys')?.classList.toggle('hidden',terminalMode);$('#input-state')?.classList.toggle('hidden',terminalMode);$('#mode-button')?.classList.toggle('hidden',terminalMode);$('#video-transport-label')?.classList.toggle('hidden',terminalMode);syncInputUi();if(terminalMode){if(changed)stopVideo();terminalConnect();scheduleTerminalFit();requestAnimationFrame(()=>terminal?.focus());}else if(changed)startVideo();}
    $$('[data-workspace]').forEach(tab=>tab.addEventListener('click',()=>setWorkspaceMode(tab.dataset.workspace)));
    $$('.tab-button').forEach(button=>button.addEventListener('click',()=>{const mode=button.dataset.panelTarget;$$('.tab-button').forEach(item=>item.classList.toggle('active',item===button));$$('[data-panel]').forEach(panel=>panel.hidden=panel.dataset.panel!==mode);}));
    const inspectorToggle=$('#inspector-toggle');
    function setInspectorOpen(open){invalidateSurfaceBox();app.classList.toggle('inspector-open',open);inspectorToggle.setAttribute('aria-expanded',String(open));inspectorToggle.classList.toggle('active',open);inspectorToggle.textContent=open?t('collapse_inspector','收起面板'):t('open_inspector','控制面板');try{localStorage.setItem('wingman_inspector_open',open?'true':'false');}catch(_){}requestAnimationFrame(()=>{invalidateSurfaceBox();window.dispatchEvent(new Event('resize'));if(videoWorkspace==='terminal')scheduleTerminalFit();});}
    inspectorToggle.addEventListener('click',()=>setInspectorOpen(!app.classList.contains('inspector-open')));
    $('#inspector-close')?.addEventListener('click',()=>setInspectorOpen(false));
    $('#inspector-backdrop')?.addEventListener('click',()=>setInspectorOpen(false));
    $('#inspector-float-open')?.addEventListener('click',()=>setInspectorOpen(true));
    window.addEventListener('keydown',event=>{if(event.key==='Escape'&&!inputEnabled()&&app.classList.contains('inspector-open')&&!keyboardDialog?.open&&!settingsDialog?.open){setInspectorOpen(false);}});

    async function refreshStatus(){if(statusTimer===-1)return;if(statusTimer>0)clearTimeout(statusTimer);if(app.classList.contains('hidden')||document.hidden){statusTimer=0;renderVideoUi();return;}statusTimer=-1;try{const status=await request('/api/status');latestVideoStatus=status.video||null;latestPowerStatus=status.power||null;renderPowerLed(latestPowerStatus);if(status.hid?.keyboard_leds)renderKeyboardLeds(status.hid.keyboard_leds);if(status.client)renderClientInfo(status.client);renderVideoUi();}catch{latestVideoStatus={state:'offline',message:'连接失败'};latestPowerStatus={power_led:{configured:true,state:'unknown',sense_error:'连接失败'}};renderPowerLed(latestPowerStatus);renderVideoUi();}finally{statusTimer=app.classList.contains('hidden')||document.hidden?0:setTimeout(()=>{statusTimer=0;refreshStatus();},3000);}}
    const mediaChoices=new Map();let mediaBusy=false,mediaServerBusy=false,mediaStatus={};
    function mediaItems(data){return Array.isArray(data)?data:(data?.items||data?.images||[]);}
    function mediaTypeLabel(type){return type==='cdrom'?'光驱':type==='disk'?'U 盘':'自动';}
    function mediaStateLabel(state){return ({attached:'已挂载',mounted:'已挂载',detached:'未挂载',attaching:'正在挂载',detaching:'正在弹出',ejecting:'正在弹出',error:'状态异常'})[state]||'';}
    function formatBytes(value){const bytes=Number(value);if(!Number.isFinite(bytes)||bytes<0)return '';if(bytes<1024)return `${bytes} B`;const units=['KB','MB','GB','TB'];let size=bytes,index=-1;do{size/=1024;index++;}while(size>=1024&&index<units.length-1);return `${size>=10?size.toFixed(0):size.toFixed(1)} ${units[index]}`;}
    function mountedMediaName(status,items){return status?.mounted_name||items.find(item=>typeof item!=='string'&&(item.mounted||item.attached))?.name||'';}
    function writableMountedDisk(status,name){return !!name&&status?.read_only===false&&status?.media_type==='disk';}
    function syncMediaBusy(){const locked=mediaBusy||mediaServerBusy;$$('.media-operation').forEach(control=>control.disabled=locked||control.dataset.mediaLocked==='true');$$('#media-upload input,#media-upload button').forEach(control=>control.disabled=mediaBusy);$('#media-refresh').disabled=mediaBusy;$('#media-list').setAttribute('aria-busy',String(locked));}
    async function runMediaAction(action,success){if(mediaBusy||mediaServerBusy)return;mediaBusy=true;syncMediaBusy();try{await action();toast(success);await refreshMedia();}catch(error){toast(error.message,true);await refreshMedia();}finally{mediaBusy=false;syncMediaBusy();}}
    function renderMediaStatus(status,items){const box=$('#media-status'),name=mountedMediaName(status,items),state=status?.state||(name?'attached':'detached'),mounted=!!name;mediaServerBusy=['attaching','detaching','ejecting','busy'].includes(state);box.className=`media-status${mounted?' mounted':''}${mediaServerBusy?' busy':''}${state==='error'?' error':''}`;box.replaceChildren();const dot=document.createElement('span');dot.className='status-dot';const copy=document.createElement('span');copy.className='media-status-copy';const title=document.createElement('strong');title.textContent=name||mediaStateLabel(state)||'未挂载';const detail=document.createElement('span');if(mounted){const parts=[mediaTypeLabel(status?.media_type),status?.read_only===false?'读写':'只读'];const stateText=mediaStateLabel(state);if(stateText&&stateText!=='已挂载')parts.unshift(stateText);detail.textContent=parts.join(' · ');}else detail.textContent=state==='error'?'请检查 LUN 配置':'选择镜像即可挂载';copy.append(title,detail);box.append(dot,copy);if(mounted){const actions=document.createElement('span');actions.className='media-status-actions';const detach=document.createElement('button');detach.type='button';detach.className='secondary media-operation';detach.textContent='弹出';detach.addEventListener('click',()=>{if(writableMountedDisk(status,name)&&!confirm('请先在目标机中弹出此 U 盘。确认已经弹出并继续吗？'))return;runMediaAction(()=>request('/api/media/detach',{method:'POST',body:JSON.stringify({force:false})}),'介质已弹出');});actions.append(detach);if(status?.forced_eject_supported){const force=document.createElement('button');force.type='button';force.className='danger media-operation';force.textContent='强制弹出';force.title='强制弹出可能损坏镜像文件系统';force.addEventListener('click',()=>{if(!confirm('强制弹出可能损坏镜像文件系统，确定继续吗？'))return;runMediaAction(()=>request('/api/media/detach',{method:'POST',body:JSON.stringify({force:true})}),'介质已强制弹出');});actions.append(force);}box.append(actions);}syncMediaBusy();}
    function renderMediaItem(item,mountedName){const data=typeof item==='string'?{name:item,size:null,mounted:false}:item,name=data.name||data.path||'',isIso=name.toLowerCase().endsWith('.iso'),mounted=!!(data.mounted||data.attached||name===mountedName),anotherMounted=!!mountedName&&!mounted,saved=mediaChoices.get(name)||{},defaultType=isIso?'cdrom':'disk',type=mounted?(mediaStatus.media_type||saved.media_type||defaultType):(saved.media_type||defaultType),readOnly=isIso||type==='cdrom'?true:mounted?mediaStatus.read_only!==false:(saved.read_only??false);const row=document.createElement('article');row.className='media-item';const head=document.createElement('div');head.className='media-item-head';const nameBox=document.createElement('span');nameBox.className='media-item-name';const title=document.createElement('strong');title.textContent=name;title.title=name;const meta=document.createElement('span');meta.className='media-item-meta';const size=formatBytes(data.size);if(size)meta.append(document.createTextNode(size));if(mounted){const badge=document.createElement('span');badge.className='media-badge';badge.textContent='已挂载';meta.append(badge);}nameBox.append(title,meta);head.append(nameBox);const controls=document.createElement('div');controls.className='media-item-controls';const select=document.createElement('select');select.className='media-type-select media-operation';select.setAttribute('aria-label',`${name} 的介质模式`);[['auto','自动'],['cdrom','光驱'],['disk','U 盘']].forEach(([value,label])=>{const option=document.createElement('option');option.value=value;option.textContent=label;select.append(option);});select.value=type;select.dataset.mediaLocked=String(mounted);const readonlyLabel=document.createElement('label');readonlyLabel.className='media-readonly';const readonly=document.createElement('input');readonly.type='checkbox';readonly.checked=readOnly;readonly.dataset.mediaLocked=String(isIso||mounted||type==='cdrom');readonly.className='media-operation';readonly.setAttribute('aria-label',`${name} 只读`);readonlyLabel.append(readonly,document.createTextNode('只读'));if(isIso)readonlyLabel.title='ISO 始终以只读方式挂载';select.addEventListener('change',()=>{if(select.value==='cdrom'){readonly.checked=true;readonly.dataset.mediaLocked='true';}else readonly.dataset.mediaLocked=String(isIso||mounted);mediaChoices.set(name,{media_type:select.value,read_only:readonly.checked});syncMediaBusy();});readonly.addEventListener('change',()=>mediaChoices.set(name,{media_type:select.value,read_only:readonly.checked}));controls.append(select,readonlyLabel);if(!mounted){const attach=document.createElement('button');attach.type='button';attach.className='primary media-attach media-operation';attach.textContent='挂载';attach.dataset.mediaLocked=String(anotherMounted);if(anotherMounted)attach.title='请先弹出当前介质';attach.addEventListener('click',()=>{const payload={name,media_type:select.value,read_only:isIso||select.value==='cdrom'?true:readonly.checked};mediaChoices.set(name,{media_type:payload.media_type,read_only:payload.read_only});runMediaAction(()=>request('/api/media/attach',{method:'POST',body:JSON.stringify(payload)}),'镜像已挂载');});controls.append(attach);}row.append(head,controls);return row;}
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

    let passkeyLoginBusy = false;
    let conditionalUiStarted = false;

    async function loginWithPasskey(isConditional = false) {
      if (passkeyLoginBusy) return;
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
      if (errorBox) errorBox.textContent = '';
      passkeyLoginBusy = true;
      if (!isConditional && btn) btn.disabled = true;

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
          if (errorBox) errorBox.textContent = err.message || 'Passkey 登录失败';
          else toast(err.message || 'Passkey 登录失败', true);
        }
      } finally {
        passkeyLoginBusy = false;
        if (btn) btn.disabled = false;
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
  </script>
</body>
</html>"##;

#[cfg(test)]
mod tests {
    use super::{ICON_SVG, INDEX_HTML};

    #[test]
    fn pointer_mode_selectors_only_offer_absolute_and_relative() {
        let selector = r#"<select name="pointer_mode"><option value="absolute">绝对</option><option value="relative">相对</option></select>"#;
        assert_eq!(INDEX_HTML.matches(selector).count(), 2);
    }

    #[test]
    fn video_settings_offer_capture_and_viewer_without_edid_controls() {
        assert!(!INDEX_HTML.contains("virtual_monitor"));
        assert!(!INDEX_HTML.contains("control_device"));
        assert!(!INDEX_HTML.contains("follow_display"));
        assert!(!INDEX_HTML.contains("EDID"));
        assert!(INDEX_HTML.contains("只调整采集和网络传输的视频尺寸"));
        assert!(INDEX_HTML.contains("<strong>Streaming</strong>"));
        assert!(INDEX_HTML.contains("<strong>Viewer</strong>"));
        assert!(INDEX_HTML.contains("object-fit:contain"));
    }

    #[test]
    fn inspector_has_close_button_and_only_four_tabs() {
        assert!(INDEX_HTML.contains("id=\"inspector-close\""));
        assert!(INDEX_HTML.contains("id=\"inspector-float-open\""));
        assert!(!INDEX_HTML.contains("data-panel-target=\"terminal\""));
        assert!(!INDEX_HTML.contains("data-panel=\"terminal\""));
        assert!(INDEX_HTML.contains("data-panel-target=\"control\""));
        assert!(INDEX_HTML.contains("data-panel-target=\"video\""));
        assert!(INDEX_HTML.contains("data-panel-target=\"devices\""));
        assert!(INDEX_HTML.contains("data-panel-target=\"media\""));
        assert!(INDEX_HTML.contains("id=\"terminal-clear\""));
        assert!(INDEX_HTML.contains("id=\"terminal-reconnect\""));
    }

    #[test]
    fn topbar_power_menu_diagnostics_and_quick_keys() {
        assert!(INDEX_HTML.contains("id=\"power-menu-toggle\""));
        assert!(INDEX_HTML.contains("id=\"power-menu\""));
        assert!(INDEX_HTML.contains("id=\"diagnostics-toggle\""));
        assert!(INDEX_HTML.contains("id=\"diagnostics-popover\""));
        assert!(INDEX_HTML.contains("data-quick-key=\"cad\""));
        assert!(INDEX_HTML.contains("data-quick-key=\"win\""));
        assert!(INDEX_HTML.contains("data-quick-key=\"alttab\""));
        assert!(INDEX_HTML.contains("data-quick-key=\"esc\""));
        assert!(!INDEX_HTML.contains("class=\"window-dots\""));
        assert!(INDEX_HTML.contains("id=\"input-state\" class=\"input-badge clickable\""));
    }

    #[test]
    fn theme_toggle_i18n_and_ergonomic_elements_present() {
        assert!(INDEX_HTML.contains("id=\"theme-toggle\""));
        assert!(INDEX_HTML.contains("id=\"lang-toggle\""));
        assert!(INDEX_HTML.contains("id=\"lang-menu\""));
        assert!(INDEX_HTML.contains("data-lang=\"zh-CN\""));
        assert!(INDEX_HTML.contains("data-lang=\"zh-TW\""));
        assert!(INDEX_HTML.contains("data-lang=\"en\""));
        assert!(INDEX_HTML.contains("id=\"input-release\""));
        assert!(INDEX_HTML.contains("data-video-preset=\"desktop\""));
        assert!(INDEX_HTML.contains("data-video-preset=\"bios\""));
        assert!(INDEX_HTML.contains("data-video-preset=\"bandwidth\""));
        assert!(INDEX_HTML.contains("capture-pulse"));
    }

    #[test]
    fn passkey_ui_and_scripts_present() {
        assert!(INDEX_HTML.contains("id=\"passkey-login-btn\""));
        assert!(INDEX_HTML.contains("id=\"passkey-add-btn\""));
        assert!(INDEX_HTML.contains("id=\"passkey-items\""));
        assert!(INDEX_HTML.contains("registerPasskey"));
        assert!(INDEX_HTML.contains("loginWithPasskey"));
        assert!(INDEX_HTML.contains("refreshPasskeys"));
        assert!(INDEX_HTML.contains("updateLoginPasskeyUI"));
        assert!(INDEX_HTML.contains("autocomplete=\"username webauthn\""));
        assert!(INDEX_HTML.contains("passkey_signin"));
        assert!(INDEX_HTML.contains("passkey_title"));
    }

    #[test]
    fn global_settings_ui_and_scripts_present() {
        assert!(INDEX_HTML.contains("id=\"settings-toggle\""));
        assert!(INDEX_HTML.contains("id=\"settings-inspector-btn\""));
        assert!(INDEX_HTML.contains("id=\"settings-dialog\""));
        assert!(INDEX_HTML.contains("id=\"setting-suppress-hid-timeout\""));
        assert!(INDEX_HTML.contains("id=\"setting-capture-pulse\""));
        assert!(INDEX_HTML.contains("id=\"setting-release-toast\""));
        assert!(INDEX_HTML.contains("id=\"setting-render-pixelated\""));
        assert!(INDEX_HTML.contains("id=\"setting-auto-inspector\""));
        assert!(INDEX_HTML.contains("id=\"setting-reset-btn\""));
        assert!(INDEX_HTML.contains("handleHidError"));
        assert!(INDEX_HTML.contains("isHidWritableTimeoutError"));
        assert!(INDEX_HTML.contains("getSetting"));
        assert!(INDEX_HTML.contains("setSetting"));
        assert!(INDEX_HTML.contains("settings-modal-pop"));
        assert!(INDEX_HTML.contains("place-items:center"));
    }

    #[test]
    fn no_emojis_in_ui_and_clean_dialog_animation() {
        let has_emoji = INDEX_HTML.chars().any(|c| {
            matches!(c,
                '\u{1F300}'..='\u{1F9FF}' |
                '\u{2600}'..='\u{27BF}' |
                '\u{2300}'..='\u{23FF}'
            )
        });
        assert!(!has_emoji, "INDEX_HTML should not contain informal emojis");
    }

    #[test]
    fn dark_mode_contrast_and_adaptations() {
        assert!(INDEX_HTML.contains("--ink-contrast:#fff"));
        assert!(INDEX_HTML.contains("--ink-contrast:#0d0d0d"));
        assert!(INDEX_HTML.contains(":root[data-theme=\"dark\"] #inspector-toggle.active{color:#0d0d0d;background:#f0f0f0;border-color:#f0f0f0}"));
        assert!(INDEX_HTML.contains(":root[data-theme=\"dark\"] .mark,:root[data-theme=\"dark\"] .user-pill,:root[data-theme=\"dark\"] .session-avatar{color:#0d0d0d;background:#f0f0f0}"));
        assert!(INDEX_HTML.contains(".mark{position:relative;display:grid;place-items:center;width:34px;height:34px;flex:0 0 auto;border-radius:50%;color:var(--ink-contrast);background:var(--ink)"));
        assert!(INDEX_HTML.contains(":root[data-theme=\"dark\"] .topbar,:root[data-theme=\"dark\"] .command-bar{background:rgba(20,20,20,0.88)}"));
    }

    #[test]
    fn keyboard_led_indicators_present() {
        assert!(INDEX_HTML.contains("id=\"console-leds\""));
        assert!(INDEX_HTML.contains("id=\"led-num\""));
        assert!(INDEX_HTML.contains("id=\"led-caps\""));
        assert!(INDEX_HTML.contains("id=\"led-scroll\""));
        assert!(INDEX_HTML.contains("id=\"inspector-led-num\""));
        assert!(INDEX_HTML.contains("id=\"inspector-led-caps\""));
        assert!(INDEX_HTML.contains("id=\"inspector-led-scroll\""));
        assert!(INDEX_HTML.contains("renderKeyboardLeds"));
        assert!(INDEX_HTML.contains("keyboard_leds_title"));
        assert!(INDEX_HTML.contains("led-badge"));
        assert!(INDEX_HTML.contains("led-dot"));
    }

    #[test]
    fn frontend_rendering_performance_optimizations() {
        assert!(INDEX_HTML.contains("contain:strict"));
        assert!(INDEX_HTML.contains("isolation:isolate"));
        assert!(INDEX_HTML.contains("translateZ(0)"));
        assert!(INDEX_HTML.contains("translate3d"));
        assert!(INDEX_HTML.contains("cachedSurfaceBox"));
        assert!(INDEX_HTML.contains("dragRaf"));
    }

    #[test]
    fn proxy_protocol_ui_and_client_address_display_present() {
        assert!(INDEX_HTML.contains("id=\"diag-client-ip\""));
        assert!(INDEX_HTML.contains("id=\"session-ip\""));
        assert!(INDEX_HTML.contains("id=\"setting-proxy-protocol\""));
        assert!(INDEX_HTML.contains("PROXY Protocol v2 (PPv2)"));
        assert!(INDEX_HTML.contains("renderClientInfo"));
        assert!(INDEX_HTML.contains("settings_ppv2"));
    }

    #[test]
    fn narrow_screen_and_responsive_optimizations_present() {
        assert!(INDEX_HTML.contains("@media(max-width:768px)"));
        assert!(INDEX_HTML.contains("@media(max-width:440px)"));
        assert!(INDEX_HTML.contains("id=\"inspector-backdrop\""));
        assert!(INDEX_HTML.contains(".inspector-backdrop"));
        assert!(INDEX_HTML.contains("power-menu-icon"));
        assert!(INDEX_HTML.contains("border-radius:20px 20px 0 0"));
    }

    #[test]
    fn brand_svg_logos_and_favicon_present() {
        assert!(INDEX_HTML.contains("id=\"logo-full\""));
        assert!(INDEX_HTML.contains("id=\"logo-icon\""));
        assert!(INDEX_HTML.contains("brand-logo-full"));
        assert!(INDEX_HTML.contains("brand-logo-icon"));
        assert!(INDEX_HTML.contains("rel=\"icon\" type=\"image/svg+xml\""));
        assert!(ICON_SVG.contains("xmlns=\"http://www.w3.org/2000/svg\""));
    }
}

pub static ICON_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 348 252"><style>:root{fill:#191D1F}@media(prefers-color-scheme:dark){:root{fill:#F5F7FA}}</style><path fill-rule="evenodd" d="M233.01 102.58C230.02 104.15 230.00 107.05 230.00 110.00 C230.00 114.33 230.00 118.67 230.00 123.00 C230.00 138.18 234.13 159.84 221.00 170.84 C213.47 177.15 205.26 177.00 196.00 177.00 C187.33 177.00 178.67 177.00 170.00 177.00 C166.00 177.00 162.00 177.00 158.00 177.00 C155.73 177.00 153.19 176.79 151.38 178.42 C147.81 181.64 149.00 187.69 149.00 192.00 C149.00 200.67 149.00 209.33 149.00 218.00 C149.00 224.91 149.30 232.44 154.16 237.96 C161.23 245.97 174.51 244.00 184.00 244.00 C209.67 244.00 235.33 244.00 261.00 244.00 C277.00 244.00 293.00 244.00 309.00 244.00 C317.66 244.00 326.85 244.82 333.84 238.62 C340.55 232.69 340.00 223.06 340.00 215.00 C340.00 196.33 340.00 177.67 340.00 159.00 C340.00 149.33 340.00 139.67 340.00 130.00 C340.00 122.55 340.35 115.31 335.62 109.01 C329.83 101.29 320.54 102.00 312.00 102.00 C293.67 102.00 275.33 102.00 257.00 102.00 C251.67 102.00 246.33 102.00 241.00 102.00 C238.59 102.00 235.21 101.41 233.01 102.58Z M164.00 102.38C161.93 102.88 159.85 103.74 158.00 104.80 C148.03 110.51 149.00 122.15 149.00 132.00 C149.00 138.00 148.93 144.00 149.00 150.00 C149.04 153.74 150.08 157.24 154.00 158.58 C158.08 159.96 163.73 159.00 168.00 159.00 C177.53 159.00 187.66 159.98 197.00 158.15 C199.19 157.71 201.02 157.33 203.00 156.20 C212.49 150.77 212.00 140.44 212.00 131.00 C212.00 124.33 212.02 117.67 212.00 111.00 C211.99 107.05 211.20 103.85 207.00 102.42 C202.92 101.04 197.27 102.00 193.00 102.00 C186.67 102.00 180.33 102.00 174.00 102.00 C170.74 102.00 167.19 101.61 164.00 102.38Z M21.00 8.41C4.83 13.35 8.00 31.95 8.00 45.00 C8.00 67.33 8.00 89.67 8.00 112.00 C8.00 119.00 8.00 126.00 8.00 133.00 C8.00 136.55 7.52 140.50 8.16 144.00 C10.07 154.42 20.48 158.98 30.00 159.00 C55.33 159.06 80.67 159.00 106.00 159.00 C112.00 159.00 118.00 159.03 124.00 159.00 C126.78 158.99 129.68 158.68 131.16 155.95 C133.27 152.07 132.00 145.31 132.00 141.00 C132.00 132.33 132.00 123.67 132.00 115.00 C132.00 107.08 131.73 99.46 137.16 93.00 C144.65 84.08 155.35 84.00 166.00 84.00 C174.33 84.00 182.67 84.00 191.00 84.00 C195.33 84.00 199.67 84.00 204.00 84.00 C206.18 84.00 208.89 84.27 210.58 82.58 C212.47 80.68 212.00 77.42 212.00 75.00 C212.00 69.00 212.00 63.00 212.00 57.00 C212.00 49.67 212.00 42.33 212.00 35.00 C212.00 24.15 211.51 12.38 199.00 8.41 C195.12 7.18 190.04 8.00 186.00 8.00 C148.00 8.00 110.00 8.00 72.00 8.00 C60.67 8.00 49.33 8.00 38.00 8.00 C32.77 8.00 26.03 6.88 21.00 8.41Z"/></svg>"##;
