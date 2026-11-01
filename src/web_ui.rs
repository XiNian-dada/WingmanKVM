//! Browser UI assets. Sources live under `web/` and are embedded at compile
//! time so the service still ships as a single binary.

pub static INDEX_HTML: &str = include_str!("../web/index.html");
/// Stylesheet fragments in cascade order; concatenated so later rules keep
/// overriding earlier ones exactly as in a single file.
pub static APP_CSS_PARTS: &[&str] = &[
    include_str!("../web/css/app/00-tokens-theme.css"),
    include_str!("../web/css/app/01-base-controls.css"),
    include_str!("../web/css/app/02-auth.css"),
    include_str!("../web/css/app/03-topbar-shell.css"),
    include_str!("../web/css/app/04-console-video.css"),
    include_str!("../web/css/app/05-inspector-panels.css"),
    include_str!("../web/css/app/06-dialogs-keyboard.css"),
    include_str!("../web/css/app/07-terminal-power.css"),
    include_str!("../web/css/app/08-popovers-diagnostics.css"),
    include_str!("../web/css/app/09-settings.css"),
    include_str!("../web/css/app/10-responsive.css"),
    include_str!("../web/css/app/11-late-overrides.css"),
    include_str!("../web/css/app/12-component-overrides.css"),
];

pub static APP_CSS: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| APP_CSS_PARTS.concat());

/// Embeds each app module as `(file name, source)`.
macro_rules! app_js_modules {
    ($($name:literal),* $(,)?) => {
        &[$(($name, include_str!(concat!("../web/js/app/", $name)))),*]
    };
}

/// The app script ships as native ES modules: index.html loads `main.js` with
/// `<script type="module">` and the browser follows the static imports. Every
/// file of web/js/app/ is embedded here (a test keeps the list complete) and
/// served unchanged at `/assets/js/<name>`; there is no bundler. Load order is
/// not set here: `main.js` runs each module's `init()` and then the boot
/// sequence.
pub static APP_JS_MODULES: &[(&str, &str)] = app_js_modules![
    "main.js",
    "state.js",
    "00-core.js",
    "00-settings.js",
    "01-i18n-phrases.js",
    "01-i18n.js",
    "02-theme-layout.js",
    "03-session.js",
    "04-video-settings.js",
    "05-power.js",
    "06-devices.js",
    "07-video.js",
    "07a-pip.js",
    "07b-quality.js",
    "07z-keyboard-macros.js",
    "07c-adaptive-bitrate.js",
    "07d-audio.js",
    "08-input.js",
    "08a-input-socket.js",
    "08b-touch.js",
    "08c-latency.js",
    "09-terminal.js",
    "10-workspace-status.js",
    "11-media.js",
    "12-passkey.js",
    "12a-sessions-audit.js",
    "12b-notifications.js",
    "12c-online-update.js",
    "13-settings-boot.js",
];

/// Source of the embedded app module `name` (e.g. `"main.js"`).
pub fn app_js_module(name: &str) -> Option<&'static str> {
    APP_JS_MODULES
        .iter()
        .find(|(module, _)| *module == name)
        .map(|(_, source)| *source)
}

pub static ICON_SVG: &str = include_str!("../web/icon.svg");

#[cfg(test)]
mod tests {
    use super::{APP_CSS, APP_JS_MODULES, ICON_SVG};

    /// The full UI source the browser loads (markup, stylesheet and every app
    /// module), with all whitespace removed so assertions check content rather
    /// than source formatting.
    static INDEX_HTML: std::sync::LazyLock<UiSource> = std::sync::LazyLock::new(|| {
        let modules: String = APP_JS_MODULES.iter().map(|(_, source)| *source).collect();
        UiSource(compact(&[super::INDEX_HTML, APP_CSS.as_str(), modules.as_str()].concat()))
    });

    fn compact(text: &str) -> String {
        let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        // Formatted CSS ends each block's last declaration with `;`.
        compact.replace(";}", "}")
    }

    struct UiSource(String);

    impl UiSource {
        fn contains(&self, needle: &str) -> bool {
            self.0.contains(&compact(needle))
        }

        fn matches(&self, needle: &str) -> std::vec::IntoIter<()> {
            vec![(); self.0.matches(&compact(needle)).count()].into_iter()
        }
    }

    /// Specifiers of the static imports that open a module, plus the source
    /// after them. Imports are `import { a } from './x.js';` or
    /// `import './x.js';`; their name lists never contain `;`.
    fn module_imports(source: &str) -> (Vec<&str>, &str) {
        let mut specifiers = Vec::new();
        let mut rest = source;
        loop {
            let trimmed = rest.trim_start();
            if trimmed.starts_with("//") {
                rest = trimmed.split_once('\n').map_or("", |(_, tail)| tail);
            } else if trimmed.starts_with("import ") || trimmed.starts_with("import{") {
                let (statement, tail) = trimmed.split_once(';').expect("import statement ends with ;");
                let quoted = statement.rsplit('\'').nth(1).expect("import has a quoted specifier");
                specifiers.push(quoted);
                rest = tail;
            } else {
                return (specifiers, trimmed);
            }
        }
    }

    #[test]
    fn every_module_import_resolves_to_an_embedded_module() {
        let names: std::collections::HashSet<&str> = APP_JS_MODULES.iter().map(|(name, _)| *name).collect();
        assert_eq!(names.len(), APP_JS_MODULES.len(), "duplicate module name");
        for (name, source) in APP_JS_MODULES {
            let (imports, body) = module_imports(source);
            // A later import would be missed above, so none may exist.
            assert!(
                !body.lines().any(|line| line.trim_start().starts_with("import ") || line.contains("from './")),
                "{name}: imports must open the module"
            );
            assert!(!body.contains("import("), "{name}: dynamic import is not embedded-checked");
            for specifier in imports {
                let target = specifier
                    .strip_prefix("./")
                    .filter(|target| !target.contains('/'))
                    .unwrap_or_else(|| panic!("{name}: import {specifier:?} must name a sibling module"));
                assert!(names.contains(target), "{name} imports {specifier}, which is not embedded");
            }
        }
    }

    #[test]
    fn index_loads_the_entry_module_and_every_module_is_reachable() {
        assert!(super::INDEX_HTML.contains(r#"<script src="/assets/js/main.js" type="module"></script>"#));
        let mut seen = vec!["main.js"];
        let mut queue = vec!["main.js"];
        while let Some(name) = queue.pop() {
            let source = super::app_js_module(name).expect("embedded module");
            for specifier in module_imports(source).0 {
                let target = specifier.trim_start_matches("./");
                if !seen.contains(&target) {
                    seen.push(target);
                    queue.push(target);
                }
            }
        }
        for (name, _) in APP_JS_MODULES {
            assert!(seen.contains(name), "{name} is embedded but never imported");
        }
    }

    #[test]
    fn every_module_file_is_embedded() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("web/js/app");
        let mut on_disk: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .filter(|name| name.ends_with(".js"))
            .collect();
        on_disk.sort();
        let mut embedded: Vec<&str> = APP_JS_MODULES.iter().map(|(name, _)| *name).collect();
        embedded.sort();
        assert_eq!(on_disk, embedded, "web/js/app and APP_JS_MODULES differ");
    }

    #[test]
    fn stylesheet_fragments_have_balanced_braces() {
        // A missing `}` silently nests every later rule under the previous
        // selector (native CSS nesting), so the rules stop applying.
        for (index, part) in super::APP_CSS_PARTS.iter().enumerate() {
            assert_eq!(part.matches('{').count(), part.matches('}').count(), "CSS fragment #{index} has unbalanced braces");
        }
    }

    #[test]
    fn index_markup_container_tags_are_balanced() {
        // An unbalanced container silently re-parents later sections (an
        // orphan `</div>` once pushed the power controls out of their panel).
        let html = super::INDEX_HTML;
        for tag in ["div", "section", "aside", "main", "form", "details", "dialog", "nav", "header", "label"] {
            let opens = html.matches(&format!("<{tag}>")).count() + html.matches(&format!("<{tag} ")).count();
            let closes = html.matches(&format!("</{tag}>")).count();
            assert_eq!(opens, closes, "<{tag}> open/close count mismatch");
        }
    }

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
        assert!(INDEX_HTML.contains("conditionalAbortController"));
        assert!(INDEX_HTML.contains("explicitPasskeyBusy"));
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
        assert!(INDEX_HTML.contains("error?.status === 504"));
        assert!(INDEX_HTML.contains("getSetting"));
        assert!(INDEX_HTML.contains("setSetting"));
        assert!(INDEX_HTML.contains("settings-modal-pop"));
        assert!(INDEX_HTML.contains("place-items:center"));
    }

    #[test]
    fn config_backup_ui_and_scripts_present() {
        assert!(INDEX_HTML.contains("id=\"config-export-btn\""));
        assert!(INDEX_HTML.contains("id=\"config-import-btn\""));
        assert!(INDEX_HTML.contains("id=\"config-import-file\""));
        assert!(INDEX_HTML.contains("id=\"config-import-dialog\""));
        assert!(INDEX_HTML.contains("id=\"config-import-diff\""));
        assert!(INDEX_HTML.contains("id=\"config-import-confirm\""));
        assert!(INDEX_HTML.contains("/api/config/export"));
        assert!(INDEX_HTML.contains("/api/config/import/preview"));
        assert!(INDEX_HTML.contains("/api/config/import"));
        assert!(INDEX_HTML.contains("renderConfigChanges"));
        assert!(INDEX_HTML.contains("config-diff-item"));
        assert!(INDEX_HTML.contains("导入失败：${}"));
    }

    #[test]
    fn no_emojis_in_ui_and_clean_dialog_animation() {
        let has_emoji = INDEX_HTML.0.chars().any(|c| {
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
        // The console header strip is the single LED indicator.
        assert!(!INDEX_HTML.contains("inspector-led-num"));
        assert!(INDEX_HTML.contains("renderKeyboardLeds"));
                assert!(INDEX_HTML.contains("led-badge"));
        assert!(INDEX_HTML.contains("led-dot"));
    }

    #[test]
    fn hdmi_audio_control_and_settings_present() {
        // The control is hidden until audio is enabled, then explains itself.
        assert!(INDEX_HTML.contains("id=\"audio-toggle\""));
        assert!(INDEX_HTML.contains("aria-disabled=\"true\""));
        assert!(INDEX_HTML.contains("name=\"audio_enabled\""));
        assert!(INDEX_HTML.contains("name=\"audio_device\""));
        assert!(INDEX_HTML.contains("id=\"audio-scan\""));
        assert!(INDEX_HTML.contains("id=\"diag-audio\""));
        assert!(INDEX_HTML.contains("MJPEG 模式不支持音频"));
        assert!(INDEX_HTML.contains("peer.addTransceiver('audio', { direction: 'recvonly' })"));
        assert!(INDEX_HTML.contains("webrtcFeed.muted = !active || shared.audioMuted"));
        // The <video> element starts muted for autoplay.
        assert!(INDEX_HTML.contains("autoplay muted playsinline"));
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
        assert!(INDEX_HTML.contains("@media(max-width:1180px)"));
        assert!(INDEX_HTML.contains("@media(max-width:1080px)"));
        assert!(INDEX_HTML.contains("@media(max-width:768px)"));
        assert!(INDEX_HTML.contains("@media(max-width:440px)"));
        assert!(INDEX_HTML.contains("id=\"inspector-backdrop\""));
        assert!(INDEX_HTML.contains(".inspector-backdrop"));
        assert!(INDEX_HTML.contains("power-menu-icon"));
        assert!(INDEX_HTML.contains(".inspector-float-handle{position:absolute"));
        assert!(INDEX_HTML.contains("display:none!important"));
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

    #[test]
    fn virtual_keyboard_sticky_modifiers_and_pan_mode_present() {
        assert!(INDEX_HTML.contains("VIRTUAL_MODIFIERS"));
        assert!(INDEX_HTML.contains("activeModifiers"));
        assert!(INDEX_HTML.contains("resetVirtualModifiers"));
        assert!(INDEX_HTML.contains("updateVirtualModifierButtons"));
        assert!(INDEX_HTML.contains("id=\"pan-toggle\""));
        assert!(INDEX_HTML.contains("setPanMode"));
        assert!(INDEX_HTML.contains("centerNativeViewport"));
        assert!(INDEX_HTML.contains("isDraggingPan"));
        assert!(INDEX_HTML.contains("wsResizeObserver"));
        assert!(INDEX_HTML.contains(".mode-native #video-viewport"));
        assert!(INDEX_HTML.contains("panning"));
    }

    #[test]
    fn inspector_overlay_drawer_and_container_query_resilience() {
        assert!(INDEX_HTML.contains(".shell{position:relative;display:block;width:100%;height:100%"));
        assert!(INDEX_HTML.contains(".inspector{position:absolute;z-index:30;right:0;top:0;bottom:0;width:360px"));
        assert!(INDEX_HTML.contains("container-type:inline-size"));
        assert!(INDEX_HTML.contains("@container (max-width:580px)"));
        assert!(!INDEX_HTML.contains("#app.inspector-open .console-actions #mode-button .mode-text"));
        assert!(!INDEX_HTML.contains("#app.inspector-open .keyboard-led-strip .led-name"));
        assert!(!INDEX_HTML.contains("#app.inspector-open .console-actions #pan-toggle .pan-text"));
    }

    #[test]
    fn host_performance_monitoring_ui_and_scripts_present() {
        // Metrics live in the top-bar popover and diagnostics, not the video panel.
        assert!(!INDEX_HTML.contains("host-perf-card"));
        assert!(INDEX_HTML.contains("id=\"pop-cpu-val\""));
        assert!(INDEX_HTML.contains("id=\"pop-mem-val\""));
        assert!(INDEX_HTML.contains("id=\"pop-disk-val\""));
        assert!(INDEX_HTML.contains("id=\"diag-cpu\""));
        assert!(INDEX_HTML.contains("id=\"diag-mem\""));
        assert!(INDEX_HTML.contains("id=\"diag-disk\""));
        assert!(INDEX_HTML.contains("id=\"diag-load\""));
        assert!(INDEX_HTML.contains("id=\"topbar-perf-btn\""));
        assert!(INDEX_HTML.contains("id=\"topbar-perf-popover\""));
        assert!(INDEX_HTML.contains("id=\"topbar-cpu-val\""));
        assert!(INDEX_HTML.contains("id=\"pop-cpu-val\""));
        assert!(INDEX_HTML.contains("toggleTopbarPerf"));
        assert!(INDEX_HTML.contains("renderSystemMetrics"));
        assert!(INDEX_HTML.contains("formatUptime"));
        assert!(INDEX_HTML.contains("host_performance"));
        assert!(INDEX_HTML.contains("cpu_usage"));
    }
}

