use super::*;

#[derive(Serialize)]
pub(super) struct Capabilities {
    pub(super) video: bool,
    pub(super) keyboard: bool,
    pub(super) mouse: bool,
    pub(super) mouse_relative: bool,
    pub(super) mouse_absolute: bool,
    pub(super) pointer_mode: PointerMode,
    pub(super) gpio_power: bool,
    pub(super) gpio_reset: bool,
    pub(super) gpio_power_led: bool,
    pub(super) mass_storage: bool,
    pub(super) video_passthrough: bool,
    pub(super) video_transcode: bool,
    pub(super) video_webrtc_h264: bool,
    /// HDMI audio is enabled; it is only ever carried by WebRTC.
    pub(super) audio: bool,
}

pub(super) async fn system_status(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let system = blocking_system_snapshot(state.system.clone()).await?;
    Ok(no_store(Json(system)))
}

/// Audio status reads procfs/sysfs to tell "idle" from "no device".
async fn blocking_audio_status(
    audio: AudioManager,
) -> Result<crate::devices::audio::AudioStatus, ApiError> {
    tokio::task::spawn_blocking(move || audio.status())
        .await
        .map_err(ApiError::internal)
}

async fn blocking_system_snapshot(
    monitor: Arc<crate::system::SystemMonitor>,
) -> Result<crate::system::SystemMetrics, ApiError> {
    tokio::task::spawn_blocking(move || monitor.snapshot())
        .await
        .map_err(ApiError::internal)
}

pub(super) async fn status(
    State(state): State<AppState>,
    ConnectInfo(connection): ConnectInfo<ClientConnection>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let client = state.client_address(connection, &headers).await;
    let config = state.config.read().await.clone();
    let webrtc = state.webrtc.status();
    let system = blocking_system_snapshot(state.system.clone()).await?;
    let hid_link = state.hid.link_status();
    let audio = blocking_audio_status(state.webrtc.audio().clone()).await?;
    Ok(no_store(Json(json!({
        "video": state.video.status(),
        "power": state.power.status().await,
        "webrtc": webrtc,
        "audio": audio,
        "capabilities": capabilities_with_webrtc(&config, webrtc.available),
        "hid": {
            "keyboard_leds": state.hid.leds(),
            "keyboard_reenumerating": hid_link.keyboard_reenumerating,
            "mouse_reenumerating": hid_link.mouse_reenumerating,
        },
        "system": system,
        "runtime": state.metrics.snapshot(),
        "notifications": { "dropped": state.notifier.dropped() },
        "client": {
            "ip": client.ip.to_string(),
            "port": client.port,
            "source": client.source,
            "proxy_protocol": config.server.proxy_protocol,
        },
    }))))
}

#[allow(dead_code)]
pub(super) fn capabilities(config: &Config) -> Capabilities {
    capabilities_with_webrtc(config, false)
}

pub(super) fn capabilities_with_webrtc(config: &Config, video_webrtc_h264: bool) -> Capabilities {
    let pointer_mode = resolved_pointer_mode(&config.hid);
    let mouse_relative = config.hid.mouse_device.is_some();
    let mouse_absolute = config.hid.absolute_pointer_device.is_some();
    Capabilities {
        video: config.video.device.is_some(),
        keyboard: config.hid.keyboard_device.is_some(),
        mouse: match pointer_mode {
            PointerMode::Absolute => mouse_absolute,
            PointerMode::Auto | PointerMode::Relative => mouse_relative,
        },
        mouse_relative,
        mouse_absolute,
        pointer_mode,
        gpio_power: config.power.enabled
            && config.power.gpio_chip.is_some()
            && config.power.gpio_line.is_some(),
        gpio_reset: config
            .power
            .reset_switch
            .as_ref()
            .is_some_and(gpio_pulse_is_configured),
        gpio_power_led: config
            .power
            .power_led
            .as_ref()
            .is_some_and(gpio_input_is_configured),
        mass_storage: config.media.enabled
            && config.media.lun_path.is_some()
            && config.media.image_directory.is_some(),
        video_passthrough: true,
        video_transcode: true,
        video_webrtc_h264,
        audio: config.audio.enabled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_resolve_automatic_pointer_mode() {
        let mut config = Config::default();
        config.hid.mouse_device = Some(PathBuf::from("/dev/hidg1"));
        let relative = capabilities(&config);
        assert!(relative.mouse);
        assert!(relative.mouse_relative);
        assert!(!relative.mouse_absolute);
        assert_eq!(relative.pointer_mode, PointerMode::Relative);

        config.hid.absolute_pointer_device = Some(PathBuf::from("/dev/hidg2"));
        let absolute = capabilities(&config);
        assert!(absolute.mouse);
        assert!(absolute.mouse_relative);
        assert!(absolute.mouse_absolute);
        assert_eq!(absolute.pointer_mode, PointerMode::Absolute);

        config.media.enabled = true;
        config.media.lun_path = Some(PathBuf::from(
            "/sys/kernel/config/usb_gadget/wingman/functions/mass_storage.0/lun.0",
        ));
        assert!(!capabilities(&config).mass_storage);
        config.media.image_directory = Some(PathBuf::from("/var/lib/wingmankvm/images"));
        assert!(capabilities(&config).mass_storage);

        assert!(!capabilities(&config).gpio_reset);
        assert!(!capabilities(&config).gpio_power_led);
        config.power.reset_switch = Some(GpioPulseConfig {
            gpio_chip: Some("gpiochip1".to_owned()),
            gpio_line: Some(10),
            ..GpioPulseConfig::default()
        });
        config.power.power_led = Some(GpioInputConfig {
            gpio_chip: Some("gpiochip1".to_owned()),
            gpio_line: Some(12),
            ..GpioInputConfig::default()
        });
        let gpio = capabilities(&config);
        assert!(gpio.gpio_reset);
        assert!(gpio.gpio_power_led);
    }

    #[test]
    fn audio_capability_follows_the_audio_switch_only() {
        let mut config = Config::default();
        assert!(!capabilities(&config).audio, "audio is off by default");
        config.audio.enabled = true;
        let enabled = capabilities(&config);
        assert!(enabled.audio);
        // MJPEG and the other capabilities are unaffected.
        assert!(enabled.video_passthrough && enabled.video_transcode);
        assert!(!enabled.video_webrtc_h264);
    }
}
