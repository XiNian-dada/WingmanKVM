//! Browser UI assets. Sources live under `web/` and are embedded at compile
//! time so the service still ships as a single binary.

pub static INDEX_HTML: &str = include_str!("../web/index.html");
pub static APP_CSS: &str = include_str!("../web/css/app.css");
pub static APP_JS: &str = include_str!("../web/js/app.js");
pub static ICON_SVG: &str = include_str!("../web/icon.svg");

#[cfg(test)]
mod tests {
    use super::{APP_CSS, APP_JS, ICON_SVG};

    /// The full UI source as the browser assembles it, with all whitespace
    /// removed so assertions check content rather than source formatting.
    static INDEX_HTML: std::sync::LazyLock<UiSource> = std::sync::LazyLock::new(|| {
        UiSource(compact(&[super::INDEX_HTML, APP_CSS, APP_JS].concat()))
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
        assert!(INDEX_HTML.contains("isHidWritableTimeoutError"));
        assert!(INDEX_HTML.contains("getSetting"));
        assert!(INDEX_HTML.contains("setSetting"));
        assert!(INDEX_HTML.contains("settings-modal-pop"));
        assert!(INDEX_HTML.contains("place-items:center"));
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
        assert!(INDEX_HTML.contains("host-perf-card"));
        assert!(INDEX_HTML.contains("perf-cpu-val"));
        assert!(INDEX_HTML.contains("perf-mem-val"));
        assert!(INDEX_HTML.contains("perf-disk-val"));
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

