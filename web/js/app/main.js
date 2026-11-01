// Entry module: index.html loads it with <script type="module">, the browser
// fetches the rest through the static imports. Evaluating a feature module only
// declares its functions and module state (the evaluation order of a module
// graph with cycles is not the old fragment order, so no module touches
// another module's bindings while it is evaluated). Each module's DOM wiring
// runs in its init(), called below in the old fragment order, and the boot
// sequence runs last, once everything is wired.
import { getSetting } from './00-settings.js';
import { collectStaticPhrases, setLanguage } from './01-i18n.js';
import { applyTheme, getPreferredTheme, init as initThemeLayout } from './02-theme-layout.js';
import { consumeSetupToken, start, init as initSession } from './03-session.js';
import { readVideoUiPreference, setDisplayScale, init as initVideoSettings } from './04-video-settings.js';
import { init as initPower } from './05-power.js';
import { init as initDevices } from './06-devices.js';
import { init as initVideo } from './07-video.js';
import { init as initPip } from './07a-pip.js';
import { init as initQuality } from './07b-quality.js';
import { init as initKeyboardMacros } from './07z-keyboard-macros.js';
import { init as initAdaptiveBitrate } from './07c-adaptive-bitrate.js';
import { init as initAudio } from './07d-audio.js';
import { init as initInput } from './08-input.js';
import { init as initTouch } from './08b-touch.js';
import { init as initLatency } from './08c-latency.js';
import { init as initTerminal } from './09-terminal.js';
import { init as initWorkspaceStatus } from './10-workspace-status.js';
import { init as initMedia } from './11-media.js';
import { init as initPasskey } from './12-passkey.js';
import { init as initSessionsAudit } from './12a-sessions-audit.js';
import { init as initNotifications } from './12b-notifications.js';
import { init as initOnlineUpdate } from './12c-online-update.js';
import { applyRenderPixelated, init as initSettingsBoot } from './13-settings-boot.js';

initThemeLayout();
initSession();
initVideoSettings();
initPower();
initDevices();
initVideo();
initPip();
initQuality();
initKeyboardMacros();
initAdaptiveBitrate();
initAudio();
initInput();
initTouch();
initLatency();
initTerminal();
initWorkspaceStatus();
initMedia();
initPasskey();
initSessionsAudit();
initNotifications();
initOnlineUpdate();
initSettingsBoot();

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
