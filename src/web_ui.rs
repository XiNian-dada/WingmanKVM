pub static INDEX_HTML: &str = r##"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover">
  <meta name="color-scheme" content="light dark">
  <meta http-equiv="Cache-Control" content="no-store, max-age=0">
  <meta http-equiv="Pragma" content="no-cache">
  <link rel="icon" href="data:,">
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
    :root{font-family:Inter,"PingFang SC","Microsoft YaHei",system-ui,-apple-system,sans-serif;color:#171717;background:#fafafa;line-height:1.45;font-feature-settings:"ss01","ss02";--ink:#171717;--ink-contrast:#fff;--body:#4d4d4d;--muted:#888;--line:#ebebeb;--line-strong:#d4d4d4;--canvas:#fff;--soft:#fafafa;--soft-2:#f5f5f5;--blue:#0070f3;--red:#ee0000;--red-soft:#fff1f1;--amber:#ab570a;--switch-bg:#d4d4d4;--shadow-2:0 1px 1px #00000005,0 2px 2px #0000000a,0 0 0 1px #0000000d;--shadow-4:0 2px 2px #00000008,0 8px 16px -4px #0000000d,0 0 0 1px #00000012;--shadow-5:0 1px 1px #00000005,0 8px 16px -4px #0000000a,0 24px 32px -8px #00000016,0 0 0 1px #00000012}
    :root[data-theme="dark"]{--ink:#f0f0f0;--ink-contrast:#0d0d0d;--body:#a8a8a8;--muted:#757575;--line:#2d2d2d;--line-strong:#444;--canvas:#141414;--soft:#0d0d0d;--soft-2:#1e1e1e;--blue:#3291ff;--red:#ff5555;--red-soft:#2d1012;--amber:#f5a623;--switch-bg:#383838;--shadow-2:0 1px 1px #00000040,0 2px 2px #00000060,0 0 0 1px #ffffff12;--shadow-4:0 2px 2px #00000050,0 8px 16px -4px #00000080,0 0 0 1px #ffffff14;--shadow-5:0 1px 1px #00000060,0 8px 16px -4px #000000a0,0 24px 32px -8px #000000d0,0 0 0 1px #ffffff18}
    :root[data-theme="dark"] .topbar,:root[data-theme="dark"] .command-bar{background:rgba(20,20,20,0.88)}
    :root[data-theme="dark"] .workspace{background:radial-gradient(circle at 50% 8%,#1c1c1c 0,#131313 42%,#0a0a0a 100%)}
    :root[data-theme="dark"] .console-head,:root[data-theme="dark"] .dropdown-menu,:root[data-theme="dark"] .diagnostics-popover,:root[data-theme="dark"] #toast,:root[data-theme="dark"] .keyboard-sheet,:root[data-theme="dark"] #keyboard-dialog,:root[data-theme="dark"] .settings-modal,:root[data-theme="dark"] #settings-dialog{background:var(--canvas);color:var(--ink)}
    :root[data-theme="dark"] .auth-shell{background:rgba(20,20,20,0.85);border-color:#ffffff18}
    :root[data-theme="dark"] .auth-story{background:linear-gradient(145deg,#181818ea,#121212f0)}
    :root[data-theme="dark"] #auth-view::before{opacity:.22}
    :root[data-theme="dark"] #auth-view::after{background:linear-gradient(to bottom,#00000015 0,#0d0d0dcc 50%,#0d0d0d 76%)}
    :root[data-theme="dark"] .auth-guide-item,:root[data-theme="dark"] .feature-chip{background:rgba(30,30,30,0.8);border-color:var(--line)}
    :root[data-theme="dark"] .guide-back,:root[data-theme="dark"] .guide-help{background:var(--soft-2);color:var(--muted);border-color:var(--line-strong)}
    :root[data-theme="dark"] .diagnostics-card{background:var(--soft-2)}
    :root[data-theme="dark"] .tab-button.active,:root[data-theme="dark"] .workspace-tab.active{background:var(--canvas);color:var(--ink)}
    :root[data-theme="dark"] .video-message{background:rgba(20,20,20,0.92);border-color:#ffffff1a}
    :root[data-theme="dark"] .media-list,:root[data-theme="dark"] .media-type-select,:root[data-theme="dark"] .setup-detection-grid select{background:var(--canvas);color:var(--ink)}
    :root[data-theme="dark"] .input-badge.active,:root[data-theme="dark"] .media-badge{color:#60a5fa;background:#172554}
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
      :root:not([data-theme="light"]){--ink:#f0f0f0;--ink-contrast:#0d0d0d;--body:#a8a8a8;--muted:#757575;--line:#2d2d2d;--line-strong:#444;--canvas:#141414;--soft:#0d0d0d;--soft-2:#1e1e1e;--blue:#3291ff;--red:#ff5555;--red-soft:#2d1012;--amber:#f5a623;--switch-bg:#383838;--shadow-2:0 1px 1px #00000040,0 2px 2px #00000060,0 0 0 1px #ffffff12;--shadow-4:0 2px 2px #00000050,0 8px 16px -4px #00000080,0 0 0 1px #ffffff14;--shadow-5:0 1px 1px #00000060,0 8px 16px -4px #000000a0,0 24px 32px -8px #000000d0,0 0 0 1px #ffffff18}
      :root:not([data-theme="light"]) .topbar,:root:not([data-theme="light"]) .command-bar{background:rgba(20,20,20,0.88)}
      :root:not([data-theme="light"]) .workspace{background:radial-gradient(circle at 50% 8%,#1c1c1c 0,#131313 42%,#0a0a0a 100%)}
      :root:not([data-theme="light"]) .console-head,:root:not([data-theme="light"]) .dropdown-menu,:root:not([data-theme="light"]) .diagnostics-popover,:root:not([data-theme="light"]) #toast,:root:not([data-theme="light"]) .keyboard-sheet,:root:not([data-theme="light"]) #keyboard-dialog,:root:not([data-theme="light"]) .settings-modal,:root:not([data-theme="light"]) #settings-dialog{background:var(--canvas);color:var(--ink)}
      :root:not([data-theme="light"]) .auth-shell{background:rgba(20,20,20,0.85);border-color:#ffffff18}
      :root:not([data-theme="light"]) .auth-story{background:linear-gradient(145deg,#181818ea,#121212f0)}
      :root:not([data-theme="light"]) #auth-view::before{opacity:.22}
      :root:not([data-theme="light"]) #auth-view::after{background:linear-gradient(to bottom,#00000015 0,#0d0d0dcc 50%,#0d0d0d 76%)}
      :root:not([data-theme="light"]) .auth-guide-item,:root:not([data-theme="light"]) .feature-chip{background:rgba(30,30,30,0.8);border-color:var(--line)}
      :root:not([data-theme="light"]) .guide-back,:root:not([data-theme="light"]) .guide-help{background:var(--soft-2);color:var(--muted);border-color:var(--line-strong)}
      :root:not([data-theme="light"]) .diagnostics-card{background:var(--soft-2)}
      :root:not([data-theme="light"]) .tab-button.active,:root:not([data-theme="light"]) .workspace-tab.active{background:var(--canvas);color:var(--ink)}
      :root:not([data-theme="light"]) .video-message{background:rgba(20,20,20,0.92);border-color:#ffffff1a}
      :root:not([data-theme="light"]) .media-list,:root:not([data-theme="light"]) .media-type-select,:root:not([data-theme="light"]) .setup-detection-grid select{background:var(--canvas);color:var(--ink)}
      :root:not([data-theme="light"]) .input-badge.active,:root:not([data-theme="light"]) .media-badge{color:#60a5fa;background:#172554}
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
    .theme-switching,.theme-switching *,.theme-switching *::before,.theme-switching *::after{transition:background-color 180ms ease,border-color 180ms ease,color 180ms ease,box-shadow 180ms ease!important}
    *{box-sizing:border-box}html,body{margin:0;min-width:320px;min-height:100%;background:var(--soft)}body{min-height:100vh;min-height:100dvh}::selection{color:var(--ink-contrast);background:var(--ink)}button,input,select{font:inherit}button{cursor:pointer}button,input,select{color:var(--ink)}button:focus-visible,input:focus-visible,select:focus-visible,[tabindex]:focus-visible{outline:0;box-shadow:0 0 0 2px var(--canvas),0 0 0 4px var(--blue)}button:disabled{color:#a1a1a1;background:var(--soft-2);cursor:not-allowed}.hidden{display:none!important}.muted{color:var(--muted)}.ok{color:var(--blue)}.error{display:block;min-height:20px;color:var(--red);font-size:13px}.mono{font-family:"Geist Mono",ui-monospace,SFMono-Regular,Menlo,Monaco,monospace}.eyebrow{margin:0 0 12px;color:var(--muted);font:12px/16px "Geist Mono",ui-monospace,SFMono-Regular,Menlo,monospace;letter-spacing:.02em;text-transform:uppercase}
    button:active:not(:disabled),.tab-button:active,.preset-chip:active,.switch:active{transform:scale(0.96)}
    .brand{display:flex;align-items:center;gap:11px;min-width:0}.mark{position:relative;display:grid;place-items:center;width:34px;height:34px;flex:0 0 auto;border-radius:50%;color:var(--ink-contrast);background:var(--ink);font-size:13px;font-weight:600;letter-spacing:-.4px;transition:background-color 160ms cubic-bezier(.2,.8,.2,1),color 160ms cubic-bezier(.2,.8,.2,1)}.brand-copy{min-width:0}.brand h1{margin:0;font-size:15px;font-weight:600;line-height:20px;letter-spacing:-.35px}.brand p{margin:1px 0 0;color:var(--muted);font:11px/15px "Geist Mono",ui-monospace,monospace;white-space:nowrap}    .primary,.secondary,.ghost,.danger,.icon-button,.key-strip button,.tab-button{min-height:36px;padding:0 14px;border:1px solid var(--line);border-radius:999px;background:var(--canvas);font-size:14px;font-weight:500;transition:background-color 160ms cubic-bezier(.2,.8,.2,1),border-color 160ms cubic-bezier(.2,.8,.2,1),color 160ms cubic-bezier(.2,.8,.2,1),transform 140ms cubic-bezier(.2,.8,.2,1)}.primary{color:var(--ink-contrast);background:var(--ink);border-color:var(--ink)}.primary:hover{background:#000}.secondary:hover,.ghost:hover,.icon-button:hover,.key-strip button:hover{background:var(--soft-2);border-color:var(--line-strong)}.ghost{border-color:transparent;background:transparent}.danger{color:var(--red);background:var(--canvas);border-color:#f2c7c7}.danger:hover{background:var(--red-soft);border-color:#efaaaa}.wide-button{width:100%;min-height:44px}.field{display:grid;gap:7px;color:var(--body);font-size:13px}.field>span:first-child,.field-label{font-weight:500;color:var(--ink)}.field input,.field select{width:100%;height:40px;padding:0 12px;border:1px solid var(--line);border-radius:6px;background:var(--canvas);font-size:14px;box-shadow:0 1px 1px #00000004;transition:border-color 160ms,box-shadow 160ms}.field input:hover,.field select:hover{border-color:var(--line-strong)}.field input::placeholder{color:#a1a1a1}.field input[type=range]{height:24px;padding:0;border:0;box-shadow:none;accent-color:var(--ink)}.hint{color:var(--muted);font-size:12px;line-height:17px}.grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:16px}.wide{grid-column:1/-1}.check{display:flex;align-items:flex-start;gap:10px;color:var(--body);font-size:13px;line-height:20px}.check input{width:16px;height:16px;margin:2px 0 0;accent-color:var(--ink)}.form-actions{display:flex;align-items:center;gap:10px;margin-top:24px;flex-wrap:wrap}.form-actions .error{flex:1 1 100%}.setup-readiness{flex:1;color:var(--muted);font-size:12px}.section-rule{height:1px;margin:24px 0;background:var(--line)}
    #auth-view{position:relative;isolation:isolate;display:grid;place-items:center;min-height:100vh;min-height:100dvh;padding:32px;overflow:hidden;background:var(--soft)}#auth-view::before{content:"";position:absolute;z-index:-2;left:50%;top:-38vw;width:min(1300px,110vw);height:min(900px,82vw);transform:translateX(-50%);background:radial-gradient(circle at 18% 58%,#00dfd8 0,transparent 28%),radial-gradient(circle at 43% 32%,#007cf0 0,transparent 31%),radial-gradient(circle at 67% 46%,#ff0080 0,transparent 31%),radial-gradient(circle at 83% 67%,#f9cb28 0,transparent 28%);filter:blur(42px);opacity:.34;animation:mesh-drift 24s ease-in-out infinite alternate}#auth-view::after{content:"";position:absolute;z-index:-1;inset:0;background:linear-gradient(to bottom,#ffffff20 0,#fafafacc 50%,#fafafa 76%)}.auth-shell{display:grid;grid-template-columns:minmax(320px,.78fr) minmax(520px,1.22fr);width:min(1180px,100%);min-height:680px;overflow:hidden;border:1px solid #ffffffb8;border-radius:20px;background:#ffffffd9;box-shadow:var(--shadow-5);backdrop-filter:blur(18px)}.auth-story{position:relative;display:flex;flex-direction:column;justify-content:space-between;min-height:100%;padding:48px;border-right:1px solid var(--line);background:linear-gradient(145deg,#ffffffdc,#fafafae8)}.auth-story-copy{position:relative;z-index:1;margin:auto 0}.auth-story h2{max-width:430px;margin:0;font-size:44px;font-weight:600;line-height:1.03;letter-spacing:-2.1px}.auth-story h2 span{color:var(--muted)}.auth-guide{margin:auto 0;scroll-margin-top:16px;animation:step-in 180ms ease}.auth-guide .eyebrow{margin-top:26px}.auth-guide h2{font-size:34px;line-height:1.08;letter-spacing:-1.4px}.auth-guide-list{display:grid;gap:10px;margin-top:24px}.auth-guide-item{padding:13px 14px;border:1px solid var(--line);border-radius:9px;background:#ffffffa8}.auth-guide-item strong{display:block;font-size:13px;font-weight:500}.auth-guide-item span{display:block;margin-top:4px;color:var(--body);font-size:12px;line-height:18px}.guide-back{min-height:32px;padding:0 11px;border:1px solid var(--line);border-radius:999px;background:var(--canvas);font-size:12px}.guide-help{display:inline-grid;place-items:center;width:28px;height:28px;margin:-4px 0 -4px 3px;padding:0;border:1px solid var(--line-strong);border-radius:50%;color:var(--muted);background:var(--canvas);font:600 11px/1 ui-monospace,monospace;vertical-align:2px}.guide-help:hover{color:var(--ink);border-color:var(--ink)}.capability-heading{display:flex;align-items:center;gap:2px}.capability-heading .guide-help{margin-left:1px}.auth-story-text{max-width:430px;margin:22px 0 0;color:var(--body);font-size:16px;line-height:25px}.feature-list{display:grid;grid-template-columns:repeat(3,1fr);gap:10px;margin-top:42px}.feature-chip{padding:12px;border:1px solid var(--line);border-radius:8px;background:#ffffffb8}.feature-chip strong{display:block;font:12px/16px "Geist Mono",ui-monospace,monospace;font-weight:400}.feature-chip span{display:block;margin-top:5px;color:var(--muted);font-size:11px}.auth-foot{display:flex;justify-content:space-between;color:var(--muted);font:11px/16px "Geist Mono",ui-monospace,monospace}.auth-panel{display:flex;align-items:center;padding:48px;background:var(--canvas)}.auth-card{width:100%;max-width:660px;margin:auto}.auth-card h2{margin:0;font-size:30px;font-weight:600;line-height:38px;letter-spacing:-1.1px}.auth-lead{margin:8px 0 28px;color:var(--body);font-size:14px;line-height:21px}.auth-form.narrow{max-width:420px;margin:auto}.auth-form .field input{height:44px}.auth-form .primary{min-height:44px}.form-heading{margin-bottom:28px}.boot-orbit{width:34px;height:34px;margin-bottom:22px;border:1px solid var(--line);border-top-color:var(--ink);border-radius:50%;animation:spin .8s linear infinite}.setup-progress{display:grid;grid-template-columns:repeat(2,1fr);gap:12px;margin:0 0 30px}.progress-step{position:relative;padding-top:12px;border-top:2px solid var(--line);color:var(--muted);font:11px/16px "Geist Mono",ui-monospace,monospace}.progress-step.active{border-color:var(--ink);color:var(--ink)}.progress-step.done{border-color:var(--blue);color:var(--body)}.setup-step{display:none;animation:step-in 180ms ease}.setup-step.active{display:block}.setup-step h3{margin:0 0 6px;font-size:20px;font-weight:600;letter-spacing:-.6px}.setup-step-copy{margin:0 0 22px;color:var(--muted);font-size:13px}.setup-scan-card{display:flex;align-items:center;justify-content:space-between;gap:16px;margin-bottom:18px;padding:16px;border:1px solid var(--line);border-radius:10px;background:var(--soft)}.setup-scan-card strong{display:block;font-size:14px;font-weight:500}.setup-detection-grid{display:grid;grid-template-columns:repeat(2,1fr);gap:10px}.setup-detection-grid .capability{min-width:0}.setup-detection-grid .capability p{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.setup-detection-grid select{width:100%;height:34px;margin-top:10px;padding:0 9px;border:1px solid var(--line);border-radius:6px;background:var(--canvas);font-size:12px}.setup-advanced{margin-top:16px;border-top:1px solid var(--line)}.setup-advanced summary{padding:16px 0;color:var(--body);cursor:pointer;font-size:13px}.setup-advanced[open] summary{margin-bottom:4px}.capability{padding:16px;border:1px solid var(--line);border-radius:8px;background:var(--canvas)}.capability .status-dot{margin-bottom:12px}.capability strong{display:block;font-size:14px;font-weight:500}.capability p{margin:5px 0 0;color:var(--muted);font-size:12px}.status-dot.warning{background:var(--amber)}.setup-option-fields{margin:0 0 12px;padding:14px;border:1px solid var(--line);border-radius:8px;background:var(--soft)}.auth-choice{display:flex;justify-content:space-between;gap:18px;padding:16px 0;border-top:1px solid var(--line)}.auth-choice strong{display:block;font-size:14px;font-weight:500}.auth-choice p{margin:4px 0 0;color:var(--muted);font-size:12px}.device-results,.media-list{margin-top:12px;padding:12px;max-height:170px;overflow:auto;white-space:pre-wrap;border:1px solid var(--line);border-radius:8px;background:var(--soft);color:var(--body);font:11px/1.55 "Geist Mono",ui-monospace,monospace}
    .auth-shell{max-height:calc(100dvh - 64px)}.auth-panel{min-height:0;max-height:calc(100dvh - 64px);overflow:auto}
    #app{height:100vh;height:100dvh;display:grid;grid-template-rows:64px minmax(0,1fr);overflow:hidden;background:var(--soft)}.topbar{position:relative;z-index:40;display:grid;grid-template-columns:minmax(220px,1fr) auto minmax(220px,1fr);align-items:center;gap:16px;padding:0 20px;border-bottom:1px solid var(--line);background:#ffffffed;backdrop-filter:blur(14px)}.topbar-center{display:flex;align-items:center;gap:9px;height:32px;padding:0 12px;border:1px solid var(--line);border-radius:999px;background:var(--soft);font-size:12px}.status-dot{display:inline-block;width:7px;height:7px;flex:0 0 auto;border-radius:50%;background:#a1a1a1}.status-dot.online{background:var(--blue)}.topbar-actions{display:flex;justify-content:flex-end;align-items:center;gap:8px}.icon-button{min-height:34px;padding:0 12px;border-radius:6px;white-space:nowrap}.user-pill{display:flex;align-items:center;justify-content:center;width:32px;height:32px;border-radius:50%;color:var(--ink-contrast);background:var(--ink);font-size:12px;transition:background-color 160ms cubic-bezier(.2,.8,.2,1),color 160ms cubic-bezier(.2,.8,.2,1)}.shell{display:grid;grid-template-columns:minmax(0,1fr) 360px;min-height:0;transition:grid-template-columns 200ms cubic-bezier(.2,.8,.2,1)}#app:not(.inspector-open) .shell{grid-template-columns:minmax(0,1fr) 0}.workspace{position:relative;min-width:0;min-height:0;overflow:hidden;background:radial-gradient(circle at 50% 8%,#fff 0,#f7f7f7 42%,#f3f3f3 100%)}.workspace::after{content:"";position:absolute;inset:auto 0 0;height:1px;background:var(--line)}.command-bar{position:absolute;z-index:15;top:18px;left:24px;right:24px;display:flex;align-items:center;gap:10px;height:44px;padding:5px 6px 5px 10px;border:1px solid var(--line);border-radius:10px;background:#ffffffeb;box-shadow:var(--shadow-2);backdrop-filter:blur(12px)}.remote-control{display:flex;align-items:center;gap:9px;padding-right:10px;border-right:1px solid var(--line);white-space:nowrap;font-size:12px;font-weight:500}.switch{position:relative;width:32px;height:18px;flex:0 0 auto}.switch input{position:absolute;inset:0;z-index:1;margin:0;opacity:0;cursor:pointer}.switch i{position:absolute;inset:0;border-radius:999px;background:var(--switch-bg);transition:background 160ms}.switch i::after{content:"";position:absolute;top:2px;left:2px;width:14px;height:14px;border-radius:50%;background:#fff;box-shadow:0 1px 3px #0003;transition:transform 160ms}.switch input:checked+i{background:var(--blue)}.switch input:checked+i::after{transform:translateX(14px)}.switch input:focus-visible+i{box-shadow:0 0 0 2px #fff,0 0 0 4px var(--blue)}.key-strip{display:flex;min-width:0;gap:5px;overflow-x:auto;scrollbar-width:none}.key-strip::-webkit-scrollbar{display:none}.key-strip button{min-width:38px;min-height:32px;padding:0 9px;border-radius:6px;font:12px/16px "Geist Mono",ui-monospace,monospace;white-space:nowrap}.mobile-only{display:none}.command-spacer{flex:1}.command-meta{color:var(--muted);font:11px/16px "Geist Mono",ui-monospace,monospace;white-space:nowrap}.console{position:absolute;z-index:5;left:24px;top:24px;width:calc(100% - 48px);height:calc(100% - 48px);min-width:360px;min-height:240px;resize:both;overflow:hidden;border:1px solid var(--line-strong);border-radius:12px;background:#050505;box-shadow:0 2px 2px #0000000a,0 12px 24px -8px #0000001a,0 32px 52px -20px #00000024;transition:border-radius 200ms cubic-bezier(.2,.8,.2,1),border-color 160ms,box-shadow 160ms,background-color 160ms}.console:hover{border-color:#bdbdbd}.console.dragging{transition:none!important}:fullscreen .console,#console:fullscreen{position:fixed!important;inset:0!important;left:0!important;top:0!important;width:100vw!important;height:100vh!important;max-width:none!important;max-height:none!important;border:0!important;border-radius:0!important;box-shadow:none!important;z-index:9999!important}:fullscreen #video-viewport,#console:fullscreen #video-viewport{inset:0!important}:fullscreen #console-head,#console:fullscreen #console-head{position:absolute;top:0;left:50%;transform:translateX(-50%) translateY(-100%);width:min(880px,94vw);border-radius:0 0 10px 10px;border:1px solid var(--line);border-top:0;box-shadow:var(--shadow-5);background:var(--canvas);z-index:100;opacity:0;transition:transform 200ms cubic-bezier(.2,.8,.2,1),opacity 200ms ease;pointer-events:none}:fullscreen #console-head:hover,:fullscreen #console-head:focus-within,:fullscreen #console-head.revealed,#console:fullscreen #console-head:hover,#console:fullscreen #console-head:focus-within,#console:fullscreen #console-head.revealed{transform:translateX(-50%) translateY(0);opacity:1;pointer-events:auto}.console-head{height:46px;display:flex;align-items:center;gap:12px;padding:0 14px;border-bottom:1px solid var(--line);background:var(--canvas);cursor:move;user-select:none}.console-title{font-size:13px;font-weight:500}.console-meta{color:var(--muted);font:11px/16px "Geist Mono",ui-monospace,monospace}.console-actions{display:flex;align-items:center;gap:6px;margin-left:auto}.console-actions button{min-height:28px;padding:0 9px;font-size:11px}.input-badge{display:inline-flex;align-items:center;gap:6px;min-height:24px;padding:0 9px;border-radius:999px;background:var(--soft);color:var(--muted);font-size:11px}.input-badge::before{content:"";width:6px;height:6px;border-radius:50%;background:#a1a1a1}.input-badge.active{color:#0761d1;background:#edf6ff}.input-badge.active::before{background:var(--blue)}#video-viewport{position:absolute;inset:46px 0 0;display:grid;place-items:center;overflow:hidden;outline:none;touch-action:auto;cursor:default;background:#050505}#video-viewport.remote{cursor:crosshair;touch-action:none;box-shadow:inset 0 0 0 2px var(--blue)}#video-viewport.remote.just-captured{animation:capture-pulse 400ms ease-out}@keyframes capture-pulse{0%{box-shadow:inset 0 0 0 4px var(--blue)}50%{box-shadow:inset 0 0 0 7px rgba(0,112,243,0.6)}100%{box-shadow:inset 0 0 0 2px var(--blue)}}#video-feed{display:block;width:100%;height:100%;max-width:100%;max-height:100%;object-fit:contain;user-select:none;-webkit-user-drag:none}.mode-native #video-feed{width:auto;height:auto;max-width:none;max-height:none}.mode-fill #video-feed{object-fit:fill}.render-pixelated #video-feed{image-rendering:pixelated}.video-message{position:absolute;max-width:calc(100% - 32px);padding:10px 13px;border:1px solid #ffffff24;border-radius:8px;background:#ffffffef;color:var(--body);box-shadow:var(--shadow-4);font-size:12px;pointer-events:none}
    .inspector{position:relative;z-index:30;display:grid;grid-template-rows:auto auto minmax(0,1fr) auto;min-width:0;overflow:hidden;border-left:1px solid var(--line);background:var(--canvas);transition:opacity 180ms,transform 200ms}#app:not(.inspector-open) .inspector{opacity:0;pointer-events:none;transform:translateX(24px)}.inspector-head{display:flex;align-items:flex-start;justify-content:space-between;padding:20px 20px 14px}.inspector-head h2{margin:0;font-size:17px;font-weight:600;letter-spacing:-.45px}.inspector-head p{margin:3px 0 0;color:var(--muted);font:11px/16px "Geist Mono",ui-monospace,monospace}.inspector-tabs{display:grid;grid-template-columns:repeat(4,1fr);gap:3px;margin:0 16px 12px;padding:3px;border-radius:8px;background:var(--soft-2)}.tab-button{min-width:0;min-height:30px;padding:0 6px;border:0;border-radius:6px;background:transparent;color:var(--muted);font-size:12px}.tab-button.active{color:var(--ink);background:var(--canvas);box-shadow:0 1px 2px #0000000c,0 0 0 1px #00000008}.inspector-body{overflow:auto;border-top:1px solid var(--line)}.inspector-panel{padding:20px}.panel-kicker{margin:0 0 5px;color:var(--muted);font:11px/16px "Geist Mono",ui-monospace,monospace;text-transform:uppercase}.panel-title{margin:0;font-size:18px;font-weight:600;letter-spacing:-.5px}.panel-copy{margin:6px 0 20px;color:var(--body);font-size:13px;line-height:19px}.panel-section{padding:18px 0;border-top:1px solid var(--line)}.panel-section:first-of-type{padding-top:0;border-top:0}.control-card{display:flex;align-items:center;justify-content:space-between;gap:14px;padding:15px;border:1px solid var(--line);border-radius:10px;background:var(--soft)}.control-card strong{display:block;font-size:13px;font-weight:500}.control-card p{margin:3px 0 0;color:var(--muted);font-size:11px}.power-row{display:grid;grid-template-columns:1fr;gap:8px}.power-row form,.power-row button{width:100%}.power-row .primary{min-height:42px}.danger.power-danger{min-height:40px;background:var(--canvas)}.side-grid{display:grid;grid-template-columns:1fr 1fr;gap:13px}.side-grid .field{font-size:12px}.settings-heading{grid-column:1/-1;margin-top:8px;padding-top:16px;border-top:1px solid var(--line)}.settings-heading:first-child{margin-top:12px;padding-top:0;border-top:0}.settings-heading strong{display:block;font:600 11px/16px "Geist Mono",ui-monospace,monospace;letter-spacing:.08em;text-transform:uppercase}.settings-heading span{display:block;margin-top:3px;color:var(--muted);font-size:11px;line-height:16px}.display-status{display:flex;align-items:flex-start;gap:8px;grid-column:1/-1;padding:10px;border:1px solid var(--line);border-radius:8px;background:var(--soft);color:var(--body);font-size:11px;line-height:16px}.display-status .status-dot{flex:0 0 auto;margin-top:4px}.display-status.error{color:var(--red);border-color:#efcaca;background:#fff7f7}.side-actions{display:flex;align-items:center;gap:8px;margin-top:15px;flex-wrap:wrap}.side-actions.wide{grid-column:1/-1}.side-actions .primary{min-height:38px}.side-advanced{margin-top:16px;border-top:1px solid var(--line)}.side-advanced.wide{grid-column:1/-1;margin-top:0}.side-advanced summary{padding:14px 0;color:var(--body);cursor:pointer;font-size:12px}.side-advanced[open] summary{margin-bottom:10px}.device-overview{margin-top:16px}.device-overview .capability{padding:13px}.device-overview .capability p{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.upload-zone{margin-top:14px;padding:18px;border:1px dashed var(--line-strong);border-radius:10px;background:var(--soft);text-align:center}.upload-zone .field{display:block}.upload-zone input[type=file]{height:auto;margin-top:10px;padding:8px;background:var(--canvas)}.upload-progress{height:3px;margin-top:12px;overflow:hidden;border-radius:3px;background:#e5e5e5}.upload-progress i{display:block;width:0;height:100%;background:var(--ink);transition:width 160ms}.media-list{max-height:240px;background:var(--canvas)}.media-list>div{display:flex;align-items:center;justify-content:space-between;gap:8px;padding:8px 0;border-bottom:1px solid var(--line)}.media-list>div:last-child{border-bottom:0}.session-row{display:flex;align-items:center;justify-content:space-between;padding:13px 16px calc(13px + env(safe-area-inset-bottom));border-top:1px solid var(--line);color:var(--muted);font-size:12px}.session-identity{display:flex;align-items:center;gap:9px}.session-avatar{display:grid;place-items:center;width:26px;height:26px;border-radius:50%;color:var(--ink-contrast);background:var(--ink);font-size:10px;transition:background-color 160ms cubic-bezier(.2,.8,.2,1),color 160ms cubic-bezier(.2,.8,.2,1)}
    .media-status{display:flex;align-items:center;gap:10px;margin-top:14px;padding:12px;border:1px solid var(--line);border-radius:10px;background:var(--soft)}.media-status>.status-dot{margin:0 2px}.media-status.mounted>.status-dot{background:var(--blue)}.media-status.busy>.status-dot{background:var(--amber)}.media-status.error>.status-dot{background:var(--red)}.media-status-copy{min-width:0;flex:1}.media-status-copy strong,.media-status-copy span{display:block;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.media-status-copy strong{font-size:13px;font-weight:500}.media-status-copy span{margin-top:2px;color:var(--muted);font-size:11px}.media-status-actions{display:flex;gap:6px;flex:0 0 auto}.media-status-actions button{min-height:30px;padding:0 9px;font-size:11px}.media-list{max-height:300px;padding:0;white-space:normal;background:var(--canvas);font:inherit}.media-item{padding:12px;border-bottom:1px solid var(--line)}.media-item:last-child{border-bottom:0}.media-item-head{display:flex;align-items:flex-start;justify-content:space-between;gap:8px}.media-item-name{min-width:0}.media-item-name strong{display:block;overflow:hidden;text-overflow:ellipsis;color:var(--ink);font-size:12px;font-weight:500;white-space:nowrap}.media-item-meta{display:flex;align-items:center;gap:6px;margin-top:3px;color:var(--muted);font:10px/15px "Geist Mono",ui-monospace,monospace}.media-badge{padding:1px 6px;border-radius:999px;color:#0761d1;background:#edf6ff}.media-item-controls{display:grid;grid-template-columns:minmax(88px,1fr) auto auto;align-items:center;gap:6px;margin-top:10px}.media-type-select{min-width:0;height:32px;padding:0 7px;border:1px solid var(--line);border-radius:6px;background:var(--canvas);font-size:11px}.media-readonly{display:flex;align-items:center;gap:5px;height:32px;padding:0 7px;border:1px solid var(--line);border-radius:6px;color:var(--body);font-size:11px;white-space:nowrap}.media-readonly input{width:14px;height:14px;margin:0;accent-color:var(--ink)}.media-attach{min-height:32px;padding:0 10px;border-radius:6px;font-size:11px}
    dialog{max-width:none;padding:0;border:0;color:var(--ink);background:transparent}.keyboard-sheet{position:fixed;left:50%;bottom:24px;width:min(940px,calc(100vw - 48px));transform:translateX(-50%);overflow:hidden;border:1px solid var(--line);border-radius:14px;background:var(--canvas);box-shadow:var(--shadow-5)}dialog::backdrop{background:#00000032;backdrop-filter:blur(4px)}.dialog-head{display:flex;align-items:center;padding:13px 16px;border-bottom:1px solid var(--line)}.dialog-head h3{margin:0;font-size:14px;font-weight:600}.dialog-head p{margin:0 0 0 10px;color:var(--muted);font:11px/16px "Geist Mono",ui-monospace,monospace}.dialog-head button{margin-left:auto}.keyboard{display:grid;gap:7px;padding:16px;overflow-x:auto}.keyboard-row{display:flex;justify-content:center;gap:6px;min-width:max-content}.keyboard button{min-width:44px;height:44px;padding:5px;border:1px solid var(--line);border-radius:6px;background:var(--soft);font:12px/16px "Geist Mono",ui-monospace,monospace}.keyboard button:hover{background:var(--soft-2);border-color:var(--line-strong)}.keyboard .grow{min-width:180px}.keyboard .active{color:var(--ink-contrast);background:var(--ink)}#toast{position:fixed;z-index:100;left:50%;bottom:24px;max-width:calc(100vw - 32px);padding:10px 14px;border:1px solid var(--line);border-radius:8px;background:var(--canvas);color:var(--ink);box-shadow:var(--shadow-4);opacity:0;pointer-events:none;translate:-50% 12px;transition:opacity 180ms,translate 180ms}#toast.show{opacity:1;translate:-50% 0}
    @keyframes spin{to{transform:rotate(360deg)}}@keyframes step-in{from{opacity:0;transform:translateX(8px)}to{opacity:1;transform:none}}@keyframes mesh-drift{to{transform:translateX(-48%) translateY(5%) scale(1.04)}}@keyframes dropdown-drop{from{opacity:0;transform:translateY(-6px) scale(0.98)}to{opacity:1;transform:translateY(0) scale(1)}}@keyframes popover-drop{from{opacity:0;transform:translateX(-50%) translateY(-6px) scale(0.98)}to{opacity:1;transform:translateX(-50%) translateY(0) scale(1)}}
    @media(max-width:1040px){.auth-shell{grid-template-columns:360px minmax(0,1fr)}.auth-story{padding:36px}.auth-story h2{font-size:38px}.feature-list{grid-template-columns:1fr}.feature-chip:nth-child(3){display:none}.topbar{grid-template-columns:1fr auto}.topbar-center{display:none}.console{left:24px;width:calc(100% - 48px)}}
    @media(max-width:900px){#app{grid-template-rows:56px minmax(0,1fr)}.topbar{padding:0 14px}.topbar .brand p,.topbar-actions #keyboard-toggle{display:none}.shell,#app:not(.inspector-open) .shell{display:block}.workspace{height:100%}.command-bar{top:12px;left:12px;right:12px}.command-meta{display:none}.console{position:absolute;left:12px!important;top:12px!important;width:calc(100% - 24px)!important;height:calc(100% - 24px)!important;min-width:0;min-height:220px;max-width:none;max-height:none;resize:none}.console-head{cursor:default}.inspector{position:fixed;z-index:80;left:12px;right:12px;bottom:12px;height:min(76dvh,680px);border:1px solid var(--line);border-radius:14px;box-shadow:var(--shadow-5);transform:translateY(calc(100% + 28px));opacity:0;pointer-events:none}#app.inspector-open .inspector{transform:none;opacity:1;pointer-events:auto}.inspector-head::before{content:"";position:absolute;top:7px;left:50%;width:36px;height:4px;transform:translateX(-50%);border-radius:4px;background:var(--line-strong)}.inspector-head{padding-top:22px}.session-row{border-radius:0 0 14px 14px}.keyboard-sheet{bottom:8px;width:calc(100vw - 16px)}}
    @media(max-width:720px){#auth-view{padding:0;overflow:auto}.auth-shell{display:block;width:100%;min-height:100dvh;border:0;border-radius:0;box-shadow:none}.auth-story{min-height:auto;padding:24px;border:0;border-bottom:1px solid var(--line)}.auth-story-copy{margin:54px 0 0}.auth-story h2{font-size:34px;letter-spacing:-1.5px}.auth-story-text{margin-top:14px}.feature-list,.auth-foot{display:none}.auth-panel{padding:32px 20px 48px}.auth-form.narrow{max-width:none}.grid{grid-template-columns:1fr}.wide{grid-column:auto}.capability-grid,.setup-detection-grid{grid-template-columns:1fr}.topbar-actions .icon-button:not(#inspector-toggle){display:none}.command-bar{height:48px}.remote-control>span:last-child{display:none}.mobile-only{display:inline-flex;align-items:center;min-height:36px;padding:0 10px;border-radius:6px}.key-strip button{min-width:44px;min-height:36px}.console{top:12px!important;height:calc(100% - 24px)!important}.console-head{height:42px;padding:0 10px}.window-dots,.console-meta{display:none}.console-title{font-size:12px}.input-badge{padding:0 7px}#video-viewport{inset:42px 0 0}.side-grid{grid-template-columns:1fr}.side-actions.wide{grid-column:auto}.setup-scan-card{align-items:flex-start;flex-direction:column}.form-actions{align-items:stretch}.form-actions button{min-height:44px}.form-actions .primary{order:-1;width:100%}}
    @media(max-width:420px){.topbar{grid-template-columns:1fr auto}.topbar-actions{gap:4px}.mark{width:30px;height:30px}.brand h1{font-size:14px}.command-bar{gap:6px;padding-left:7px}.remote-control{padding-right:6px}.key-strip{gap:4px}.console-actions .input-badge{display:none}.inspector{left:6px;right:6px;bottom:6px;height:84dvh}.inspector-panel{padding:18px 16px}.keyboard-sheet{bottom:0;width:100vw;border-radius:14px 14px 0 0}.keyboard{padding:12px}.dialog-head p{display:none}}
    #app{grid-template-rows:64px minmax(0,1fr) auto}#keyboard-dialog{position:relative;inset:auto;align-self:end;width:100%;max-width:none;max-height:min(34dvh,310px);margin:0;overflow:hidden;border:0;border-top:1px solid var(--line);border-radius:0;background:var(--canvas)}#keyboard-dialog::backdrop{display:none}.keyboard-sheet{position:static;left:auto;bottom:auto;width:100%;transform:none;border:0;border-radius:0;box-shadow:0 -8px 24px -16px #0003}.keyboard-sheet .dialog-head{min-height:46px;padding:9px 16px}.keyboard{max-height:calc(min(34dvh,310px) - 46px);gap:4px;padding:10px}.keyboard-row{gap:4px}.keyboard button{min-width:40px;height:36px;font-size:11px}.keyboard .grow{min-width:150px}
    @media(max-width:900px){#app{grid-template-rows:56px minmax(0,1fr) auto}#keyboard-dialog{max-height:min(38dvh,300px)}.keyboard{max-height:calc(min(38dvh,300px) - 46px)}.keyboard button{min-width:42px;height:38px}}
    .console{left:24px;top:24px;width:calc(100% - 48px);height:calc(100% - 48px);max-width:none;max-height:none}.inspector-keys{display:grid;grid-template-columns:repeat(7,minmax(0,1fr));gap:6px;overflow:visible;margin-bottom:10px}.inspector-keys button{width:100%;min-width:0;min-height:34px;padding:0 4px}
    @media(max-width:900px){#keyboard-toggle{display:inline-flex!important;align-items:center}.user-pill{display:none}.console{left:12px!important;top:12px!important;width:calc(100% - 24px)!important;height:calc(100% - 24px)!important}}
    h2[tabindex="-1"]:focus{outline:0;box-shadow:none}
    @media(max-width:720px){.auth-shell{max-height:none}.auth-panel{max-height:none;overflow:visible}}
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
    .dropdown-menu{position:absolute;right:0;top:calc(100% + 8px);z-index:90;min-width:210px;padding:6px;border:1px solid var(--line);border-radius:10px;background:var(--canvas);box-shadow:var(--shadow-5);display:grid;gap:3px;animation:dropdown-drop 140ms cubic-bezier(.16,1,.3,1)}
    .dropdown-menu[hidden]{display:none!important}
    .dropdown-header{padding:6px 10px 8px;border-bottom:1px solid var(--line);color:var(--muted);font:11px/16px "Geist Mono",ui-monospace,monospace}
    .dropdown-item{display:flex;align-items:center;gap:8px;width:100%;min-height:34px;padding:6px 10px;border:0;border-radius:6px;background:transparent;color:var(--ink);font-size:12px;text-align:left;cursor:pointer;transition:background-color 140ms}
    .dropdown-item:hover{background:var(--soft-2)}
    .dropdown-item.danger{color:var(--red)}
    .dropdown-item.danger:hover{background:var(--red-soft)}
    .diagnostics-popover{position:fixed;left:50%;top:64px;transform:translateX(-50%);z-index:90;width:min(440px,calc(100vw - 32px));padding:18px;border:1px solid var(--line);border-radius:14px;background:var(--canvas);box-shadow:var(--shadow-5);animation:popover-drop 150ms cubic-bezier(.16,1,.3,1)}
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
    #settings-dialog{max-width:none;padding:0;border:0;color:var(--ink);background:transparent}
    #settings-dialog::backdrop{background:#00000040;backdrop-filter:blur(4px)}
    .settings-modal{position:fixed;left:50%;top:50%;transform:translate(-50%,-50%);width:min(540px,calc(100vw - 32px));max-height:min(86dvh,700px);display:flex;flex-direction:column;overflow:hidden;border:1px solid var(--line);border-radius:14px;background:var(--canvas);box-shadow:var(--shadow-5);animation:popover-drop 160ms cubic-bezier(.16,1,.3,1)}
    .settings-dialog-body{padding:16px 20px 22px;overflow-y:auto;display:flex;flex-direction:column;gap:18px}
    .settings-group{display:flex;flex-direction:column;gap:9px}
    .settings-group-title{font:600 11px/16px "Geist Mono",ui-monospace,monospace;color:var(--muted);text-transform:uppercase;letter-spacing:.08em;padding-bottom:4px;border-bottom:1px solid var(--line)}
    .settings-item{display:flex;align-items:center;justify-content:space-between;gap:14px;padding:11px 13px;border:1px solid var(--line);border-radius:9px;background:var(--soft)}
    .settings-item-copy{display:flex;flex-direction:column;gap:2px}
    .settings-item-copy strong{font-size:13px;font-weight:500;color:var(--ink)}
    .settings-item-copy span{font-size:11px;color:var(--muted);line-height:15px}
    @media(max-width:720px){.quick-keys{display:none}}
  </style>
  <style>
    #webrtc-feed{display:block;width:100%;height:100%;max-width:100%;max-height:100%;object-fit:contain;user-select:none;-webkit-user-drag:none;background:#050505}
    #video-feed.video-surface,#webrtc-feed.video-surface{display:block}
    #video-feed.video-surface.hidden,#webrtc-feed.video-surface.hidden{display:none!important}
    .mode-native #webrtc-feed{width:auto;height:auto;max-width:none;max-height:none}
    .mode-fill #webrtc-feed{object-fit:fill}
    .render-pixelated #webrtc-feed{image-rendering:pixelated}
    #video-transport-label{min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
    @media(max-width:720px){.window-dots{display:none}.console-meta{display:inline;max-width:64px;font-size:10px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}}
    @media(max-width:420px){.console-meta{max-width:50px}}
    .gpio-config-block{grid-column:1/-1;margin-top:5px;padding-top:14px;border-top:1px solid var(--line)}.gpio-config-title{display:flex;align-items:center;justify-content:space-between;gap:8px;margin-bottom:10px;color:var(--ink);font:600 11px/16px "Geist Mono",ui-monospace,monospace;letter-spacing:.04em}.gpio-inline-test{min-height:26px;padding:0 8px;font-size:10px;letter-spacing:0}.gpio-config-status{display:flex;align-items:center;gap:6px;color:var(--muted);font-size:10px;font-weight:400;letter-spacing:0}.gpio-config-status .status-dot{width:6px;height:6px}.gpio-config-fields{gap:10px}.gpio-config-fields .field{font-size:11px}.gpio-config-fields .field input,.gpio-config-fields .field select{height:36px;font-size:12px}.field-help{display:inline-grid;place-items:center;width:16px;height:16px;margin-left:4px;padding:0;border:1px solid var(--line-strong);border-radius:50%;color:var(--muted);background:var(--canvas);font:600 10px/1 "Geist Mono",ui-monospace,monospace;vertical-align:1px;cursor:help;transition:color 160ms,border-color 160ms,background-color 160ms}.field-help:hover,.field-help:focus-visible{color:var(--ink);border-color:var(--ink);background:var(--soft-2)}.field-help:focus-visible{outline:0;box-shadow:0 0 0 2px var(--canvas),0 0 0 4px var(--blue)}#field-tooltip{position:fixed;z-index:120;max-width:min(280px,calc(100vw - 24px));padding:8px 10px;border:1px solid #383838;border-radius:8px;color:#fff;background:#1d1d1d;box-shadow:0 8px 20px #0003;font-size:11px;line-height:17px;overflow-wrap:anywhere;pointer-events:none;opacity:0;translate:0 4px;transition:opacity 140ms,translate 140ms}#field-tooltip.visible{opacity:1;translate:0 0}#field-tooltip[hidden]{display:none}
  </style>
</head>
<body>
  <section id="auth-view">
    <div class="auth-shell">
      <section class="auth-story" aria-label="WingmanKVM 简介">
        <div class="brand"><div class="mark">W</div><div class="brand-copy"><h1>WingmanKVM</h1></div></div>
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
      <div class="brand"><div class="mark">W</div><div class="brand-copy"><h1>WingmanKVM</h1></div></div>
      <button id="diagnostics-toggle" class="topbar-center" type="button" aria-expanded="false" aria-controls="diagnostics-popover" title="点击查看连接与设备诊断"><span id="status-dot" class="status-dot"></span><span id="status-label">正在建立视频链路</span><svg width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M4 6l4 4 4-4"/></svg></button>
      <div class="topbar-actions">
        <div class="power-menu-container">
          <button id="power-menu-toggle" class="icon-button" type="button" aria-expanded="false" aria-controls="power-menu" title="电源与系统控制"><span data-power-led-dot class="status-dot"></span><span data-i18n="power">电源</span><svg width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M4 6l4 4 4-4"/></svg></button>
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
        <button id="keyboard-toggle" class="icon-button" type="button" data-i18n="keyboard">键盘</button>
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
      </div>
      <div class="diagnostics-actions">
        <button id="diagnostics-reconnect" class="secondary" type="button" data-i18n="diag_reconnect">重连视频</button>
        <button id="diagnostics-scan" class="secondary" type="button" data-i18n="diag_scan">扫描设备</button>
      </div>
    </div>
    <div class="shell">
      <section id="workspace" class="workspace">
        <div id="console" class="console mode-fit render-pixelated">
          <div id="console-head" class="console-head"><div class="workspace-tabs"><button class="workspace-tab active" type="button" data-workspace="video" data-i18n="tab_remote_screen">远程画面</button><button class="workspace-tab" type="button" data-workspace="terminal" data-i18n="tab_terminal">终端</button></div><div class="quick-keys" aria-label="常用快捷键"><button class="icon-button-sm" type="button" data-quick-key="cad" title="发送 Ctrl+Alt+Del">Ctrl+Alt+Del</button><button class="icon-button-sm" type="button" data-quick-key="win" title="发送 Win 徽标键">Win</button><button class="icon-button-sm" type="button" data-quick-key="alttab" title="发送 Alt+Tab">Alt+Tab</button><button class="icon-button-sm" type="button" data-quick-key="esc" title="发送 Esc">Esc</button></div><span id="video-transport-label" class="console-meta">MJPEG</span><div class="console-actions"><button id="input-release" class="icon-button-sm danger hidden" type="button" data-i18n="release_keys" title="紧急释放所有按键与鼠标锁定">释放按键</button><button id="input-state" class="input-badge clickable" type="button" role="status" title="点击切换键鼠转发状态">输入已暂停</button><button id="terminal-reconnect" class="secondary hidden" type="button" data-i18n="reconnect">重连</button><button id="terminal-clear" class="secondary hidden" type="button" data-i18n="clear">清空</button><button id="mode-button" class="secondary" type="button" data-i18n="mode_fit">适应</button><button id="fullscreen" class="secondary" type="button" data-i18n="fullscreen">全屏</button></div></div>
          <div id="video-viewport" tabindex="0" aria-label="远程视频与鼠标控制区域">
            <img id="video-feed" class="video-surface" alt="远程设备视频" draggable="false"><video id="webrtc-feed" class="video-surface hidden" autoplay muted playsinline aria-label="远程设备视频"></video><span id="video-message" class="video-message" role="status" aria-live="polite">正在连接视频…</span>
          </div>
          <div id="terminal-window" class="terminal-window" hidden><div id="terminal-host" class="terminal-host" aria-label="RK3399 终端"></div></div>
        </div>
        <button id="inspector-float-open" class="inspector-float-handle" type="button" aria-label="展开控制面板" title="展开控制面板"><svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M10 12L6 8l4-4"/></svg><span data-i18n="inspector">控制面板</span></button>
      </section>
      <aside id="inspector" class="inspector">
        <header class="inspector-head"><div><h2 data-i18n="inspector">控制面板</h2></div><button id="inspector-close" class="ghost" type="button" aria-label="收起控制面板" title="收起控制面板 (快捷键: Esc)"><svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 4L4 12M4 4l8 8"/></svg><span data-i18n="inspector_close">收起</span></button></header>
        <nav class="inspector-tabs" aria-label="控制面板"><button class="tab-button active" type="button" data-panel-target="control" data-i18n="tab_control">控制</button><button class="tab-button" type="button" data-panel-target="video" data-i18n="tab_video">视频</button><button class="tab-button" type="button" data-panel-target="devices" data-i18n="tab_devices">设备</button><button class="tab-button" type="button" data-panel-target="media" data-i18n="tab_media">介质</button></nav>
        <div class="inspector-body">
          <section class="inspector-panel" data-panel="control">
            <h3 class="panel-title" data-i18n="remote_control">远程控制</h3>
            <div class="panel-section"><div class="control-card"><div><strong data-i18n="forward_input">转发键鼠</strong></div><label class="switch"><input id="remote-input" type="checkbox" aria-label="转发键盘和鼠标"><i></i></label></div></div>
            <div class="panel-section"><div class="key-strip inspector-keys" aria-label="特殊按键"><button type="button" data-key="Escape">Esc</button><button type="button" data-key="Delete">Del</button><button type="button" data-key="F1">F1</button><button type="button" data-key="F2">F2</button><button type="button" data-key="F3">F3</button><button type="button" data-key="F4">F4</button><button type="button" data-key="F5">F5</button><button type="button" data-key="F6">F6</button><button type="button" data-key="F7">F7</button><button type="button" data-key="F8">F8</button><button type="button" data-key="F9">F9</button><button type="button" data-key="F10">F10</button><button type="button" data-key="F11">F11</button><button type="button" data-key="F12">F12</button></div></div>
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
              <button type="button" class="preset-chip" data-video-preset="desktop" data-i18n="preset_desktop">🖥️ 桌面优化 (1080p 60fps 8M)</button>
              <button type="button" class="preset-chip" data-video-preset="bios" data-i18n="preset_bios">⚙️ BIOS/UEFI (720p 30fps MJPEG)</button>
              <button type="button" class="preset-chip" data-video-preset="bandwidth" data-i18n="preset_bandwidth">⚡ 低带宽省流 (720p 25fps 2M)</button>
            </div>
            <form id="video-config" class="side-grid">
              <div class="settings-heading"><strong>Display</strong><span>决定被控机通过 HDMI EDID 看到的虚拟显示器。</span></div>
              <label class="field wide"><span>虚拟显示器 <button class="field-help" type="button" data-help="写入仅包含所选分辨率的易失 EDID，并短暂中断采集卡 HDMI RX。此型号尚未证实能从软件触发被控机侧物理 HPD；若被控机未重新输出目标分辨率，会自动回滚。停止接管只是不再写入，不会猜测或恢复未知的出厂 EDID。" aria-label="虚拟显示器说明">?</button></span><select name="virtual_monitor"><option value="unmanaged">未接管 / 停止接管</option><option value="hd1080p60">1920 × 1080 @ 60Hz</option><option value="hd720p60">1280 × 720 @ 60Hz</option></select></label>
              <div id="display-status" class="display-status" role="status" aria-live="polite"><span class="status-dot"></span><span>未接管采集卡 EDID</span></div>
              <details class="side-advanced wide"><summary>EDID 控制接口 <button class="field-help" type="button" data-help="通常留空，WingmanKVM 会寻找与视频节点属于同一块 345f:2130 采集卡的厂商 HID 接口；只有排障时才手动填写。" aria-label="EDID 控制接口说明">?</button></summary><label class="field wide"><span>HIDRAW 路径</span><input name="control_device" class="mono" placeholder="自动匹配 /dev/hidrawN"></label></details>
              <div class="settings-heading"><strong>Streaming</strong><span>只控制采集和网络传输，不改变被控机桌面布局。</span></div>
              <label class="field wide"><span>播放方式 <button class="field-help" type="button" data-help="自动优先尝试 WebRTC H.264；不可用时回退到 MJPEG。H.264 延迟和带宽通常更低，MJPEG 兼容性更好。" aria-label="播放方式说明">?</button></span><select name="transport"><option value="auto">自动</option><option value="webrtc_h264">低延迟 H.264</option><option value="mjpeg">兼容 MJPEG</option></select></label>
              <label class="field wide"><span>H.264 码率 <button class="field-help" type="button" data-help="H.264 的目标码率。数值越高画质越好、占用带宽越大；数值越低更省带宽，但画面可能出现块状失真。" aria-label="H.264 码率说明">?</button></span><select name="h264_bitrate_preset"><option value="2000">2 Mbps</option><option value="4000">4 Mbps</option><option value="8000">8 Mbps</option><option value="12000">12 Mbps</option><option value="custom">自定义</option></select></label>
              <label class="field wide h264-bitrate-custom hidden"><span>自定义码率 · Kbps <button class="field-help" type="button" data-help="单位是 Kbps，允许范围为 256–50000。" aria-label="自定义码率说明">?</button></span><input name="h264_bitrate_kbps" type="number" min="256" max="50000"></label>
              <label class="field wide"><span>采集输出尺寸 <button class="field-help" type="button" data-help="Native 会在 EDID 已确认应用后，让 UVC 分辨率与虚拟显示器一致；固定尺寸只改变采集和传输大小。" aria-label="采集输出尺寸说明">?</button></span><select name="resolution_preset"><option value="native">Native · 跟随虚拟显示器</option><option value="auto">设备默认</option><option value="3840x2160">4K · 3840 × 2160</option><option value="2560x1440">1440p · 2560 × 1440</option><option value="1920x1080">1080p · 1920 × 1080</option><option value="1280x720">720p · 1280 × 720</option><option value="720x480">480p · 720 × 480</option><option value="custom">自定义</option></select></label>
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
        <div class="session-row"><span class="session-identity"><span class="session-avatar">WK</span><span id="session-user">管理员</span></span><div style="display:flex;align-items:center;gap:6px;"><button id="settings-inspector-btn" class="ghost" type="button" data-i18n="settings">设置</button><button id="logout" class="ghost" type="button" data-i18n="logout">退出登录</button></div></div>
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
        diag_title:'连接与设备诊断',diag_transport:'视频传输链路',diag_resolution:'采集画面分辨率',diag_input:'键鼠控制模式',diag_power_led:'被控机电源 LED',diag_reconnect:'重连视频',diag_scan:'扫描设备',
        tab_remote_screen:'远程画面',tab_terminal:'终端',release_keys:'释放按键',reconnect:'重连',clear:'清空',mode_fit:'适应',fullscreen:'全屏',exit_fullscreen:'退出全屏',
        tab_control:'控制',tab_video:'视频',tab_devices:'设备',tab_media:'介质',remote_control:'远程控制',forward_input:'转发键鼠',
        short_press_btn:'短按电源',reset_btn:'复位',force_off_btn:'强制关机',refresh:'刷新',
        preset_desktop:'🖥️ 桌面优化 (1080p 60fps 8M)',preset_bios:'⚙️ BIOS/UEFI (720p 30fps MJPEG)',preset_bandwidth:'⚡ 低带宽省流 (720p 25fps 2M)',
        apply:'应用',rescan:'重新检测',save:'保存',upload:'上传',logout:'退出登录',virtual_keyboard:'虚拟键盘',close:'收起',
        input_paused:'输入已暂停',input_captured:'相对鼠标已捕获',input_click_to_capture:'点击画面捕获鼠标',input_forwarding:'键鼠正在转发',input_released:'已释放所有按键与鼠标锁定',
        theme_dark:'深色模式',theme_light:'浅色模式',theme_system:'跟随系统',switch_theme:'切换外观主题',
        connecting:'正在连接…',video_connecting:'正在连接视频…',video_paused:'视频已暂停',device_offline:'设备离线',
        collapse_inspector:'收起面板',open_inspector:'控制面板',lang_name:'简',
        passkey_title:'Passkey 通行密钥',passkey_desc:'绑定 Apple Touch ID / Face ID，无需密码快速登录',
        passkey_add:'+ 绑定此设备',passkey_signin:'使用 Passkey (Touch ID / Face ID) 登录',passkey_none:'尚未绑定任何 Passkey',
        passkey_delete:'删除',passkey_prompt_name:'为新 Passkey 命名（例如：MacBook Pro、iPhone）：',
        passkey_insecure:'⚠️ Passkey 需要在安全连接（HTTPS 或 localhost）下使用',
        passkey_ip_warn:'⚠️ WebAuthn 规范要求使用域名（如 localhost 或 wingman.local），不支持 IP 地址',
        passkey_added_success:'Passkey 绑定成功！',passkey_delete_confirm:'确定要删除此 Passkey 吗？',
        settings:'设置',settings_title:'全局偏好设置',settings_desc:'配置交互反馈、通知拦截与全局渲染偏好',
        settings_group_input:'交互与通知',
        settings_suppress_hid:'屏蔽 HID 写入超时警告',settings_suppress_hid_desc:'在纯 Linux 环境或被控端未开机时，忽略鼠标与键盘写入超时 (writable timeout) 弹窗警告',
        settings_capture_pulse:'接管边缘微光脉冲',settings_capture_pulse_desc:'激活键鼠转发时，在远端画面边缘显示光晕视觉反馈',
        settings_release_toast:'按键释放提示',settings_release_toast_desc:'执行「释放按键」或切换控制权时弹出状态提示',
        settings_group_display:'画面与渲染',
        settings_render_pixelated:'像素锐利渲染 (Pixelated)',settings_render_pixelated_desc:'近邻采样字符边缘无模糊滤镜，适合 BIOS 与纯文本终端',
        settings_auto_inspector:'宽屏默认展开面板',settings_auto_inspector_desc:'屏幕宽度充足时，进入控制台默认展开右侧控制面板',
        settings_group_system:'系统与重置',
        settings_reset_btn:'恢复默认设置',settings_reset_desc:'清除所有保存在本机的偏好设置并还原为初始默认值',
        settings_reset_action:'还原',settings_reset_confirm:'确定要恢复所有全局偏好设置为默认值吗？',settings_reset_done:'已恢复默认设置',settings_saved:'设置已保存'
      },
      'zh-TW': {
        power:'電源',power_status:'電源狀態',short_press:'短按電源開機/關機',reset_pc:'重置重啟 (Reset)',force_off:'強制關機 (長按 5 秒)',
        keyboard:'鍵盤',inspector:'控制面板',inspector_close:'收起',
        diag_title:'連線與設備診斷',diag_transport:'視訊傳輸鏈路',diag_resolution:'採集畫面解析度',diag_input:'鍵鼠控制模式',diag_power_led:'被控機電源 LED',diag_reconnect:'重新連線視訊',diag_scan:'掃描設備',
        tab_remote_screen:'遠端畫面',tab_terminal:'終端機',release_keys:'釋放按鍵',reconnect:'重新連線',clear:'清除',mode_fit:'適應',fullscreen:'全螢幕',exit_fullscreen:'退出全螢幕',
        tab_control:'控制',tab_video:'視訊',tab_devices:'設備',tab_media:'媒體',remote_control:'遠端控制',forward_input:'轉發鍵鼠',
        short_press_btn:'短按電源',reset_btn:'重置',force_off_btn:'強制關機',refresh:'重新整理',
        preset_desktop:'🖥️ 桌面最佳化 (1080p 60fps 8M)',preset_bios:'⚙️ BIOS/UEFI (720p 30fps MJPEG)',preset_bandwidth:'⚡ 低頻寬省流 (720p 25fps 2M)',
        apply:'套用',rescan:'重新檢測',save:'儲存',upload:'上傳',logout:'登出',virtual_keyboard:'虛擬鍵盤',close:'收起',
        input_paused:'輸入已暫停',input_captured:'相對滑鼠已捕獲',input_click_to_capture:'點擊畫面捕獲滑鼠',input_forwarding:'鍵鼠正在轉發',input_released:'已釋放所有按鍵與滑鼠鎖定',
        theme_dark:'深色模式',theme_light:'淺色模式',theme_system:'跟隨系統',switch_theme:'切換外觀主題',
        connecting:'正在連線…',video_connecting:'正在連線視訊…',video_paused:'視訊已暫停',device_offline:'設備離線',
        collapse_inspector:'收起面板',open_inspector:'控制面板',lang_name:'繁',
        passkey_title:'Passkey 通行密鑰',passkey_desc:'綁定 Apple Touch ID / Face ID，無需密碼快速登入',
        passkey_add:'+ 綁定此設備',passkey_signin:'使用 Passkey (Touch ID / Face ID) 登入',passkey_none:'尚未綁定任何 Passkey',
        passkey_delete:'刪除',passkey_prompt_name:'為新 Passkey 命名（例如：MacBook Pro、iPhone）：',
        passkey_insecure:'⚠️ Passkey 需要在安全連線（HTTPS 或 localhost）下使用',
        passkey_ip_warn:'⚠️ WebAuthn 規範要求使用網域名稱（如 localhost 或 wingman.local），不支援 IP 位址',
        passkey_added_success:'Passkey 綁定成功！',passkey_delete_confirm:'確定要刪除此 Passkey 嗎？',
        settings:'設定',settings_title:'全局偏好設定',settings_desc:'配置交互反饋、通知攔截與全局渲染偏好',
        settings_group_input:'交互與通知',
        settings_suppress_hid:'屏蔽 HID 寫入逾時警告',settings_suppress_hid_desc:'在純 Linux 環境或被控端未開機時，忽略滑鼠與鍵盤寫入逾時 (writable timeout) 彈窗警告',
        settings_capture_pulse:'接管邊緣微光脈衝',settings_capture_pulse_desc:'激活鍵鼠轉發時，在遠端畫面邊緣顯示光暈視覺反饋',
        settings_release_toast:'按鍵釋放提示',settings_release_toast_desc:'執行「釋放按鍵」或切換控制權時彈出狀態提示',
        settings_group_display:'畫面與渲染',
        settings_render_pixelated:'像素銳利渲染 (Pixelated)',settings_render_pixelated_desc:'近鄰採樣字符邊緣無模糊濾鏡，適合 BIOS 與純文本終端',
        settings_auto_inspector:'寬螢幕預設展開面板',settings_auto_inspector_desc:'螢幕寬度充足時，進入控制台預設展開右側控制面板',
        settings_group_system:'系統與重設',
        settings_reset_btn:'恢復預設設定',settings_reset_desc:'清除所有保存在本機的偏好設定並還原為初始預設值',
        settings_reset_action:'還原',settings_reset_confirm:'確定要恢復所有全局偏好設定為預設值嗎？',settings_reset_done:'已恢復預設設定',settings_saved:'設定已儲存'
      },
      'en': {
        power:'Power',power_status:'Power Status',short_press:'Power On / Off (Short Press)',reset_pc:'System Reset (Reset)',force_off:'Force Power Off (Hold 5s)',
        keyboard:'Keyboard',inspector:'Inspector',inspector_close:'Collapse',
        diag_title:'Diagnostics',diag_transport:'Video Transport',diag_resolution:'Capture Resolution',diag_input:'Input Mode',diag_power_led:'Target Power LED',diag_reconnect:'Reconnect Video',diag_scan:'Scan Devices',
        tab_remote_screen:'Remote Screen',tab_terminal:'Terminal',release_keys:'Release Keys',reconnect:'Reconnect',clear:'Clear',mode_fit:'Fit',fullscreen:'Fullscreen',exit_fullscreen:'Exit Fullscreen',
        tab_control:'Control',tab_video:'Video',tab_devices:'Devices',tab_media:'Media',remote_control:'Remote Control',forward_input:'Forward Input',
        short_press_btn:'Power',reset_btn:'Reset',force_off_btn:'Force Off',refresh:'Refresh',
        preset_desktop:'🖥️ Desktop (1080p 60fps 8M)',preset_bios:'⚙️ BIOS/UEFI (720p 30fps MJPEG)',preset_bandwidth:'⚡ Low Bandwidth (720p 25fps 2M)',
        apply:'Apply',rescan:'Rescan',save:'Save',upload:'Upload',logout:'Log Out',virtual_keyboard:'Virtual Keyboard',close:'Close',
        input_paused:'Input Paused',input_captured:'Relative Mouse Captured',input_click_to_capture:'Click to Capture Mouse',input_forwarding:'Input Active',input_released:'All keys and pointer locks released',
        theme_dark:'Dark Mode',theme_light:'Light Mode',theme_system:'System Theme',switch_theme:'Toggle Theme',
        connecting:'Connecting…',video_connecting:'Connecting video…',video_paused:'Video paused',device_offline:'Device offline',
        collapse_inspector:'Collapse',open_inspector:'Inspector',lang_name:'EN',
        passkey_title:'Passkey Credentials',passkey_desc:'Sign in with Apple Touch ID / Face ID or Windows Hello',
        passkey_add:'+ Add This Device',passkey_signin:'Sign in with Passkey',passkey_none:'No Passkeys registered yet',
        passkey_delete:'Delete',passkey_prompt_name:'Enter a name for this Passkey (e.g. MacBook Pro, iPhone):',
        passkey_insecure:'⚠️ Passkey requires a secure context (HTTPS or localhost)',
        passkey_ip_warn:'⚠️ WebAuthn requires a domain name (e.g. localhost or wingman.local), IP addresses not supported',
        passkey_added_success:'Passkey registered successfully!',passkey_delete_confirm:'Are you sure you want to delete this Passkey?',
        settings:'Settings',settings_title:'Global Preferences',settings_desc:'Configure interaction feedback, alert silencing, and rendering preferences',
        settings_group_input:'Interaction & Alerts',
        settings_suppress_hid:'Silence HID Timeout Warnings',settings_suppress_hid_desc:'Suppress popup warnings when mouse/keyboard /dev/hidg device times out waiting to become writable',
        settings_capture_pulse:'Input Capture Edge Glow',settings_capture_pulse_desc:'Display a subtle blue edge glow feedback when activating remote keyboard/mouse capture',
        settings_release_toast:'Key Release Notification',settings_release_toast_desc:'Show toast notification when releasing held keys or toggling remote input',
        settings_group_display:'Display & Rendering',
        settings_render_pixelated:'Pixelated Sharp Scaling',settings_render_pixelated_desc:'Disable bilinear blur filtering for crisp characters in BIOS and text terminals',
        settings_auto_inspector:'Auto-expand Inspector on Wide Screens',settings_auto_inspector_desc:'Automatically keep the right-hand Inspector open on large screens',
        settings_group_system:'System & Reset',
        settings_reset_btn:'Reset to Defaults',settings_reset_desc:'Clear all locally stored preferences and restore initial default values',
        settings_reset_action:'Reset',settings_reset_confirm:'Are you sure you want to reset all preferences to defaults?',settings_reset_done:'Preferences reset to defaults',settings_saved:'Settings saved'
      }
    };
    let currentLang = 'zh-CN';
    function t(key, fallback = '') { return I18N[currentLang]?.[key] ?? I18N['zh-CN']?.[key] ?? fallback ?? key; }
    function updateI18nElements() {
      $$('[data-i18n]').forEach(el => {
        const key = el.dataset.i18n;
        if (key && I18N[currentLang]?.[key]) el.textContent = I18N[currentLang][key];
      });
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
    addEventListener('resize', positionFieldTooltip);
    addEventListener('scroll', positionFieldTooltip, true);
    let bootstrap = {}, remoteWanted = false, pageActive = !document.hidden, reconnectTimer = 0, reconnectDelay = 500;
    let videoGeneration = 0, videoWorkspace = 'video', videoPreference = 'auto', activeVideoTransport = 'none', videoUiState = 'paused', videoFallback = false;
    let webrtcPeer = null, webrtcSessionId = null, webrtcAbort = null, videoFirstFrameTimer = 0, statusTimer = 0, latestDisplayStatus = null, latestVideoStatus = null, latestPowerStatus = null, videoConfigRequestId = 0;
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
    }
    function setVideoRendering(rendering, remember = false) {
      const next=rendering==='smooth'?'smooth':'pixelated';consoleBox.classList.toggle('render-pixelated',next==='pixelated');
      $('#video-config').elements.rendering.value=next;if(remember)saveVideoUiPreference('rendering',next);
    }
    function syncVideoPresets() {
      const form=$('#video-config'), resolution=value(form,'resolution_preset'), fps=value(form,'fps_preset');
      const native=resolution==='native';
      $$('.resolution-custom',form).forEach(field=>field.classList.toggle('hidden',resolution!=='custom'));
      form.elements.fps_preset.disabled=native;form.elements.frames_per_second.disabled=native;
      $('.fps-custom',form).classList.toggle('hidden',native||fps!=='custom');
      if(resolutionPresets.has(resolution)){const [width,height]=resolution.split('x');form.elements.width.value=width;form.elements.height.value=height;}
      else if(resolution==='auto'||native){form.elements.width.value='';form.elements.height.value='';}
      if(fpsPresets.has(fps))form.elements.frames_per_second.value=fps;else if(fps==='auto')form.elements.frames_per_second.value='';
    }
    function syncVirtualMonitorControl(userChange=false) {
      const form=$('#video-config'),managed=value(form,'virtual_monitor')!=='unmanaged',nativeOption=form.elements.resolution_preset.querySelector('option[value="native"]');
      nativeOption.disabled=!managed;
      if(!managed&&value(form,'resolution_preset')==='native'){form.elements.resolution_preset.value='auto';syncVideoPresets();if(userChange)toast('未接管 EDID 时不能使用 Native 采集',true);}
    }
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
      const form=$('#video-config'), resolution=video.follow_display?'native':video.width&&video.height?`${video.width}x${video.height}`:'auto', fps=video.frames_per_second==null?'auto':String(video.frames_per_second);
      form.elements.resolution_preset.value=resolution==='native'?'native':resolutionPresets.has(resolution)?resolution:(resolution==='auto'?'auto':'custom');
      form.elements.fps_preset.value=fpsPresets.has(fps)?fps:(fps==='auto'?'auto':'custom');
      syncVideoPresets();
    }
    function renderDisplayStatus(status) {
      const box=$('#display-status');if(!box)return;const dot=$('.status-dot',box),copy=$('span:last-child',box),state=status?.state||'unmanaged';
      box.classList.toggle('error',state==='error'||state==='unsupported');dot.classList.toggle('online',state==='applied');dot.classList.toggle('warning',state==='applying');
      copy.textContent=status?.message||(state==='applied'?'EDID RAM 已应用':state==='applying'?'正在应用 EDID…':'未接管采集卡 EDID');
    }
    function applyBootstrap() {
      const config = unwrapConfig(bootstrap), display=config.display||{}, video = config.video || {}, h264 = video.h264 || {}, hid = config.hid || {}, power = config.power || {}, media = config.media || {};
      const vf = $('#video-config'), df = $('#device-config'), mf = $('#media-config');
      vf.elements.virtual_monitor.value=display.virtual_monitor||'unmanaged';vf.elements.control_device.value=display.control_device||'';
      for (const name of ['device','width','height','frames_per_second','encoding','jpeg_quality']) if (video[name] != null && vf.elements[name]) vf.elements[name].value = video[name];
      selectVideoPresets(video);
      syncVirtualMonitorControl();latestDisplayStatus=bootstrap.display||latestDisplayStatus;renderDisplayStatus(latestDisplayStatus);
      selectH264Preset(h264);
      for (const name of ['keyboard_device','mouse_device','absolute_pointer_device']) df.elements[name].value = hid[name] || '';
      df.elements.pointer_mode.value = bootstrap.capabilities?.pointer_mode==='relative'?'relative':'absolute';
      df.elements.power_enabled.checked = !!power.enabled; df.elements.gpio_chip.value = power.gpio_chip ?? ''; df.elements.gpio_line.value = power.gpio_line ?? ''; df.elements.active_high.value = power.active_high === false ? 'false' : 'true';
      const reset=power.reset_switch||{},led=power.power_led||{}; df.elements.reset_gpio_chip.value=reset.gpio_chip??''; df.elements.reset_gpio_line.value=reset.gpio_line??''; df.elements.reset_active_high.value=reset.active_high===false?'false':'true'; df.elements.reset_pulse_ms.value=reset.pulse_ms??''; df.elements.power_led_gpio_chip.value=led.gpio_chip??''; df.elements.power_led_gpio_line.value=led.gpio_line??''; df.elements.power_led_active_low.value=led.active_low===false?'false':'true'; df.elements.power_led_bias.value=led.bias||'pull_up'; df.elements.power_led_poll_interval_ms.value=led.poll_interval_ms??''; df.elements.power_led_debounce_ms.value=led.debounce_ms??'';
      mf.elements.enabled.checked = !!media.enabled; if (media.lun_path != null) mf.elements.lun_path.value = media.lun_path; if (media.lun_file != null && !media.lun_path) mf.elements.lun_path.value = media.lun_file; if (media.image_directory != null) mf.elements.image_directory.value = media.image_directory;
      $('#quality-value').textContent = vf.elements.jpeg_quality.value; $('#session-user').textContent = bootstrap.username || bootstrap.user?.username || '管理员';if(bootstrap.capabilities?.pointer_mode==='absolute'&&relativeCaptured())document.exitPointerLock();syncInputUi();syncTransportControl(); syncGpioControls(); renderPowerLed(latestPowerStatus);
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
      event.preventDefault(); const f=event.currentTarget,button=$('button[type=submit]',f);if(button.disabled)return;const resolution=value(f,'resolution_preset'),native=resolution==='native',virtualMonitor=value(f,'virtual_monitor'),currentVirtual=unwrapConfig(bootstrap).display?.virtual_monitor||'unmanaged';if(native&&virtualMonitor==='unmanaged'){toast('Native 采集需要先选择虚拟显示器',true);return;}if(virtualMonitor!==currentVirtual&&virtualMonitor!=='unmanaged'&&!confirm('将写入易失 EDID，并短暂中断采集卡 HDMI RX。此型号尚未证实能触发被控机侧物理 HPD；若 HDMI 输入未变为目标分辨率，将自动回滚。确定继续吗？'))return;const requestId=++videoConfigRequestId,label=button.textContent,payload={display:{virtual_monitor:virtualMonitor,control_device:value(f,'control_device')||null},video:{device:value(f,'device')||null,follow_display:native,width:native?null:optionalNumber(value(f,'width')),height:native?null:optionalNumber(value(f,'height')),frames_per_second:native?null:optionalNumber(value(f,'frames_per_second')),encoding:value(f,'encoding'),jpeg_quality:Number(value(f,'jpeg_quality')),h264:{bitrate_kbps:Number(value(f,'h264_bitrate_kbps')),encoder:value(f,'h264_encoder'),allow_software:f.elements.h264_allow_software.checked,max_sessions:Number(value(f,'h264_max_sessions'))}}};
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
    function showVideoSurface(kind){feed.classList.toggle('hidden',kind!=='mjpeg');webrtcFeed.classList.toggle('hidden',kind!=='webrtc_h264');}
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
    document.addEventListener('pointermove',event=>{if(!document.fullscreenElement)return;const head=$('#console-head');if(!head)return;if(event.clientY<48)head.classList.add('revealed');else if(event.clientY>84&&!head.matches(':hover')&&!head.contains(document.activeElement))head.classList.remove('revealed');});

    let drag=null; $('#console-head').addEventListener('pointerdown',event=>{if(event.button!==0||event.target.closest('button,input,select,a')||matchMedia('(max-width:900px)').matches)return;const r=consoleBox.getBoundingClientRect(),w=$('#workspace').getBoundingClientRect();drag={x:event.clientX-r.left,y:event.clientY-r.top,wx:w.left,wy:w.top};consoleBox.classList.add('dragging');event.currentTarget.setPointerCapture(event.pointerId);});
    $('#console-head').addEventListener('pointermove',event=>{if(!drag)return;const area=$('#workspace').getBoundingClientRect();const left=Math.max(0,Math.min(event.clientX-area.left-drag.x,area.width-consoleBox.offsetWidth));const top=Math.max(0,Math.min(event.clientY-area.top-drag.y,area.height-consoleBox.offsetHeight));consoleBox.style.left=`${left}px`;consoleBox.style.top=`${top}px`;});
    const stopDrag=()=>{if(drag){drag=null;consoleBox.classList.remove('dragging');}};
    $('#console-head').addEventListener('pointerup',stopDrag); $('#console-head').addEventListener('pointercancel',stopDrag);

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
    async function sendKey(key, event={}) { if(!inputEnabled()){toast('请先开启“转发键鼠”',true);return;} await request('/api/key',{method:'POST',body:JSON.stringify({key,ctrl:!!event.ctrlKey,shift:!!event.shiftKey,alt:!!event.altKey,meta:!!event.metaKey,hold_ms:key.startsWith('F')?100:25})}); }
    async function sendQuickKey(action){if(!inputEnabled())setRemote(true);try{if(action==='cad'){await request('/api/key',{method:'POST',body:JSON.stringify({key:'Delete',ctrl:true,alt:true,hold_ms:100})});toast('已发送 Ctrl+Alt+Del');}else if(action==='win'){await request('/api/key',{method:'POST',body:JSON.stringify({key:'Meta',meta:true,hold_ms:60})});toast('已发送 Win');}else if(action==='alttab'){await request('/api/key',{method:'POST',body:JSON.stringify({key:'Tab',alt:true,hold_ms:100})});toast('已发送 Alt+Tab');}else if(action==='esc'){await request('/api/key',{method:'POST',body:JSON.stringify({key:'Escape',hold_ms:40})});toast('已发送 Esc');}}catch(e){toast(e.message,true);}}
    $$('[data-quick-key]').forEach(button=>button.addEventListener('click',()=>sendQuickKey(button.dataset.quickKey)));
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
      const surface=activeVideoSurface(),sourceWidth=surface instanceof HTMLVideoElement?surface.videoWidth:surface.naturalWidth,sourceHeight=surface instanceof HTMLVideoElement?surface.videoHeight:surface.naturalHeight,box=surface.getBoundingClientRect();if(!sourceWidth||!sourceHeight||!box.width||!box.height)return null;
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

    function setWorkspaceMode(mode){const next=mode==='terminal'?'terminal':'video',terminalMode=next==='terminal',changed=videoWorkspace!==next;videoWorkspace=next;$('#terminal-window').hidden=!terminalMode;$('#video-viewport').hidden=terminalMode;$$('[data-workspace]').forEach(tab=>tab.classList.toggle('active',tab.dataset.workspace===next));$('#terminal-clear')?.classList.toggle('hidden',!terminalMode);if(!terminalMode)$('#terminal-reconnect')?.classList.add('hidden');else if(!terminalSocket||terminalSocket.readyState>WebSocket.OPEN)$('#terminal-reconnect')?.classList.remove('hidden');$('.quick-keys')?.classList.toggle('hidden',terminalMode);$('#input-state')?.classList.toggle('hidden',terminalMode);$('#mode-button')?.classList.toggle('hidden',terminalMode);$('#video-transport-label')?.classList.toggle('hidden',terminalMode);syncInputUi();if(terminalMode){if(changed)stopVideo();terminalConnect();scheduleTerminalFit();requestAnimationFrame(()=>terminal?.focus());}else if(changed)startVideo();}
    $$('[data-workspace]').forEach(tab=>tab.addEventListener('click',()=>setWorkspaceMode(tab.dataset.workspace)));
    $$('.tab-button').forEach(button=>button.addEventListener('click',()=>{const mode=button.dataset.panelTarget;$$('.tab-button').forEach(item=>item.classList.toggle('active',item===button));$$('[data-panel]').forEach(panel=>panel.hidden=panel.dataset.panel!==mode);}));
    const inspectorToggle=$('#inspector-toggle');
    function setInspectorOpen(open){app.classList.toggle('inspector-open',open);inspectorToggle.setAttribute('aria-expanded',String(open));inspectorToggle.classList.toggle('active',open);inspectorToggle.textContent=open?t('collapse_inspector','收起面板'):t('open_inspector','控制面板');try{localStorage.setItem('wingman_inspector_open',open?'true':'false');}catch(_){}requestAnimationFrame(()=>{window.dispatchEvent(new Event('resize'));if(videoWorkspace==='terminal')scheduleTerminalFit();});}
    inspectorToggle.addEventListener('click',()=>setInspectorOpen(!app.classList.contains('inspector-open')));
    $('#inspector-close')?.addEventListener('click',()=>setInspectorOpen(false));
    $('#inspector-float-open')?.addEventListener('click',()=>setInspectorOpen(true));
    window.addEventListener('keydown',event=>{if(event.key==='Escape'&&!inputEnabled()&&app.classList.contains('inspector-open')&&!keyboardDialog?.open&&!settingsDialog?.open){setInspectorOpen(false);}});

    async function refreshStatus(){if(statusTimer===-1)return;if(statusTimer>0)clearTimeout(statusTimer);if(app.classList.contains('hidden')||document.hidden){statusTimer=0;renderVideoUi();return;}statusTimer=-1;try{const status=await request('/api/status');latestDisplayStatus=status.display||null;latestVideoStatus=status.video||null;latestPowerStatus=status.power||null;renderDisplayStatus(latestDisplayStatus);renderPowerLed(latestPowerStatus);renderVideoUi();}catch{latestDisplayStatus={state:'error',message:'EDID 状态读取失败'};latestVideoStatus={state:'offline',message:'连接失败'};latestPowerStatus={power_led:{configured:true,state:'unknown',sense_error:'连接失败'}};renderDisplayStatus(latestDisplayStatus);renderPowerLed(latestPowerStatus);renderVideoUi();}finally{statusTimer=app.classList.contains('hidden')||document.hidden?0:setTimeout(()=>{statusTimer=0;refreshStatus();},3000);}}
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
    }
    function openSettingsDialog() {
      if (!settingsDialog) return;
      syncSettingsDialogUi();
      try { settingsDialog.showModal?.() || settingsDialog.show?.(); } catch(_) { settingsDialog.show?.(); }
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
    use super::INDEX_HTML;

    #[test]
    fn pointer_mode_selectors_only_offer_absolute_and_relative() {
        let selector = r#"<select name="pointer_mode"><option value="absolute">绝对</option><option value="relative">相对</option></select>"#;
        assert_eq!(INDEX_HTML.matches(selector).count(), 2);
    }

    #[test]
    fn video_settings_keep_display_capture_and_viewer_separate() {
        assert!(INDEX_HTML.contains("<strong>Display</strong>"));
        assert!(INDEX_HTML.contains("name=\"virtual_monitor\""));
        assert!(INDEX_HTML.contains("Native · 跟随虚拟显示器"));
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
}
