use super::*;

pub(super) fn validate_config(config: &Config) -> Result<(), ApiError> {
    if config.version != CONFIG_VERSION {
        return Err(ApiError::bad_request("配置版本不受支持"));
    }
    config.server.socket_address().map_err(|error| match error {
        config::ConfigError::InvalidListenAddress { .. } => ApiError::bad_request(
            "server.listen_address 必须是不带方括号或空白的 IPv4/IPv6 地址",
        ),
        config::ConfigError::InvalidServerPort { .. } => {
            ApiError::bad_request("server.port 必须在 1 到 65535 之间")
        }
        _ => ApiError::bad_request("无效的服务器监听地址"),
    })?;
    if let Some(origin) = &config.server.public_origin {
        config::NormalizedOrigin::parse(origin)
            .map_err(|reason| ApiError::bad_request(format!("无效的 server.public_origin: {reason}")))?;
    }
    config
        .notifications
        .validate()
        .map_err(ApiError::bad_request)?;
    if config.server.trusted_http_proxies.len() > 64
        || config.server.trusted_http_proxies.iter().any(|ip| ip.is_unspecified() || ip.is_multicast())
    {
        return Err(ApiError::bad_request("可信 HTTP 代理最多配置 64 个明确的单播 IP 地址"));
    }
    // Width and height are optional independently; validate each on its own
    // so a partial override cannot persist an out-of-range capture size.
    if config.video.width.is_some_and(|width| !(160..=7680).contains(&width))
        || config.video.height.is_some_and(|height| !(120..=4320).contains(&height))
    {
        return Err(ApiError::bad_request("视频分辨率超出允许范围"));
    }
    if config
        .video
        .frames_per_second
        .is_some_and(|fps| !(1..=120).contains(&fps))
    {
        return Err(ApiError::bad_request("视频帧率必须在 1 到 120 之间"));
    }
    if !(1..=100).contains(&config.video.jpeg_quality) {
        return Err(ApiError::bad_request("JPEG 质量必须在 1 到 100 之间"));
    }
    if !(256..=50_000).contains(&config.video.h264.bitrate_kbps) {
        return Err(ApiError::bad_request(
            "H.264 码率必须在 256 到 50000 Kbps 之间",
        ));
    }
    if !(1..=4).contains(&config.video.h264.max_sessions) {
        return Err(ApiError::bad_request("H.264 同时会话数必须在 1 到 4 之间"));
    }
    config
        .video
        .h264
        .validate_udp_mapping()
        .map_err(ApiError::bad_request)?;
    for path in [
        config.video.device.as_ref(),
        config.video.h264.ffmpeg_path.as_ref(),
        config.hid.keyboard_device.as_ref(),
        config.hid.mouse_device.as_ref(),
        config.hid.absolute_pointer_device.as_ref(),
        config.media.image_directory.as_ref(),
        config.media.lun_path.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        if !path.is_absolute() {
            return Err(ApiError::bad_request("设备和存储路径必须是绝对路径"));
        }
    }
    if config.hid.pointer_mode == PointerMode::Absolute
        && config.hid.absolute_pointer_device.is_none()
        && !config.hid.auto_detect
    {
        return Err(ApiError::bad_request("绝对指针模式需要配置绝对指针设备"));
    }
    if config.hid.pointer_mode == PointerMode::Relative
        && config.hid.mouse_device.is_none()
        && !config.hid.auto_detect
    {
        return Err(ApiError::bad_request("相对指针模式需要配置相对鼠标设备"));
    }
    if config.power.enabled
        && configured_gpio(config.power.gpio_chip.as_ref(), config.power.gpio_line).is_none()
    {
        return Err(ApiError::bad_request(
            "启用电源控制前必须配置 GPIO 芯片和线路",
        ));
    }
    if config.power.gpio_chip.is_some() != config.power.gpio_line.is_some() {
        return Err(ApiError::bad_request("电源 GPIO 芯片和线路必须同时配置"));
    }
    if config
        .power
        .gpio_chip
        .as_deref()
        .is_some_and(|chip| chip.trim().is_empty())
    {
        return Err(ApiError::bad_request("电源 GPIO 芯片不能为空"));
    }
    if !(50..=2_000).contains(&config.power.short_press_ms) {
        return Err(ApiError::bad_request(
            "电源短按时长必须在 50 到 2000 毫秒之间",
        ));
    }
    if !(1_000..=15_000).contains(&config.power.long_press_ms)
        || config.power.long_press_ms < config.power.short_press_ms
    {
        return Err(ApiError::bad_request(
            "电源长按时长必须在 1000 到 15000 毫秒之间且不短于短按",
        ));
    }
    if let Some(reset) = &config.power.reset_switch {
        if reset.gpio_chip.is_some() != reset.gpio_line.is_some() {
            return Err(ApiError::bad_request("重启 GPIO 芯片和线路必须同时配置"));
        }
        if !gpio_pulse_is_configured(reset) {
            return Err(ApiError::bad_request(
                "重启 GPIO 配置必须同时包含芯片和线路",
            ));
        }
        if !(50..=2_000).contains(&reset.pulse_ms) {
            return Err(ApiError::bad_request(
                "重启脉冲时长必须在 50 到 2000 毫秒之间",
            ));
        }
    }
    if let Some(power_led) = &config.power.power_led {
        if power_led.gpio_chip.is_some() != power_led.gpio_line.is_some() {
            return Err(ApiError::bad_request("PWR LED GPIO 芯片和线路必须同时配置"));
        }
        if !gpio_input_is_configured(power_led) {
            return Err(ApiError::bad_request(
                "PWR LED GPIO 配置必须同时包含芯片和线路",
            ));
        }
        if !(100..=5_000).contains(&power_led.poll_interval_ms) {
            return Err(ApiError::bad_request(
                "PWR LED 轮询间隔必须在 100 到 5000 毫秒之间",
            ));
        }
        if power_led.debounce_ms > 5_000 {
            return Err(ApiError::bad_request(
                "PWR LED 去抖时长必须在 0 到 5000 毫秒之间",
            ));
        }
    }
    let mut gpio_lines: HashMap<(String, u32), &str> = HashMap::new();
    let mut check_gpio = |label: &'static str, chip: Option<&String>, line: Option<u32>| {
        if let (Some(chip), Some(line)) = (chip, line) {
            let key = (normalize_gpio_chip(chip), line);
            if gpio_lines.insert(key, label).is_some() {
                return Err(ApiError::bad_request(
                    "电源、重启和 PWR LED 不能共用同一个 GPIO 线路",
                ));
            }
        }
        Ok(())
    };
    check_gpio(
        "power",
        config.power.gpio_chip.as_ref(),
        config.power.gpio_line,
    )?;
    if let Some(reset) = &config.power.reset_switch {
        check_gpio("reset", reset.gpio_chip.as_ref(), reset.gpio_line)?;
    }
    if let Some(power_led) = &config.power.power_led {
        check_gpio(
            "power_led",
            power_led.gpio_chip.as_ref(),
            power_led.gpio_line,
        )?;
    }
    if config.media.enabled
        && (config.media.lun_path.is_none() || config.media.image_directory.is_none())
    {
        return Err(ApiError::bad_request(
            "启用虚拟介质前必须配置 LUN 和镜像目录",
        ));
    }
    if config.media.max_upload_bytes == 0 {
        return Err(ApiError::bad_request("镜像上传大小上限必须大于零"));
    }
    let hid_paths = [
        config.hid.keyboard_device.as_ref(),
        config.hid.mouse_device.as_ref(),
        config.hid.absolute_pointer_device.as_ref(),
    ];
    for (index, path) in hid_paths.iter().enumerate() {
        if path.is_some() && hid_paths[index + 1..].contains(path) {
            return Err(ApiError::bad_request("每个 HID 功能必须使用不同的设备路径"));
        }
    }
    Ok(())
}

pub(super) fn normalize_gpio_chip(chip: &str) -> String {
    let chip = chip.trim();
    Path::new(chip)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(chip)
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_validation_rejects_unsafe_values() {
        let mut config = Config::default();
        config.video.frames_per_second = Some(121);
        assert!(validate_config(&config).is_err());

        for (width, height) in [(Some(0), None), (Some(99_999), None), (None, Some(0)), (None, Some(9_999))] {
            let mut config = Config::default();
            config.video.width = width;
            config.video.height = height;
            assert!(validate_config(&config).is_err(), "{width:?}x{height:?} must be rejected");
        }
        let mut config = Config::default();
        config.video.width = Some(1920);
        assert!(validate_config(&config).is_ok());

        let mut config = Config::default();
        config.hid.keyboard_device = Some(PathBuf::from("dev/hidg0"));
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.hid.pointer_mode = PointerMode::Absolute;
        config.hid.auto_detect = false;
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.hid.pointer_mode = PointerMode::Relative;
        config.hid.auto_detect = false;
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.hid.pointer_mode = PointerMode::Absolute;
        assert!(validate_config(&config).is_ok());

        let mut config = Config::default();
        config.hid.pointer_mode = PointerMode::Relative;
        assert!(validate_config(&config).is_ok());

        let mut config = Config::default();
        config.video.h264.bitrate_kbps = 0;
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.video.h264.max_sessions = 0;
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.power.enabled = true;
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.hid.keyboard_device = Some(PathBuf::from("/dev/hidg0"));
        config.hid.absolute_pointer_device = Some(PathBuf::from("/dev/hidg0"));
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.media.enabled = true;
        config.media.lun_path = Some(PathBuf::from(
            "/sys/kernel/config/usb_gadget/wingman/functions/mass_storage.0/lun.0",
        ));
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.media.max_upload_bytes = 0;
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.server.public_origin = Some("https://kvm.example.com".to_owned());
        assert!(validate_config(&config).is_ok());
        config.server.public_origin = Some("https://user@kvm.example.com".to_owned());
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn configuration_validation_rejects_invalid_server_listener() {
        for address in ["localhost", "kvm.example.com", "[::1]", " 127.0.0.1"] {
            let mut config = Config::default();
            config.server.listen_address = address.to_owned();
            let error = validate_config(&config).unwrap_err();
            assert!(error.message.contains("listen_address"), "{address:?}");
        }

        let mut config = Config::default();
        config.server.port = 0;
        let error = validate_config(&config).unwrap_err();
        assert!(error.message.contains("server.port"));

        let mut config = Config::default();
        config.server.listen_address = "::1".to_owned();
        config.server.port = 9090;
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn gpio_validation_rejects_duplicate_lines_and_unsafe_timing() {
        let mut config = Config::default();
        config.power.enabled = true;
        config.power.gpio_chip = Some("   ".to_owned());
        config.power.gpio_line = Some(7);
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.power.gpio_chip = Some("/dev/gpiochip1".to_owned());
        config.power.gpio_line = Some(7);
        config.power.reset_switch = Some(GpioPulseConfig {
            gpio_chip: Some("gpiochip1".to_owned()),
            gpio_line: Some(7),
            ..GpioPulseConfig::default()
        });
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.power.short_press_ms = 49;
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.power.long_press_ms = 999;
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.power.reset_switch = Some(GpioPulseConfig {
            gpio_chip: Some("gpiochip1".to_owned()),
            gpio_line: Some(10),
            pulse_ms: 2_001,
            ..GpioPulseConfig::default()
        });
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.power.power_led = Some(GpioInputConfig {
            gpio_chip: Some("gpiochip1".to_owned()),
            gpio_line: Some(12),
            poll_interval_ms: 99,
            ..GpioInputConfig::default()
        });
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.power.power_led = Some(GpioInputConfig {
            gpio_chip: Some("gpiochip1".to_owned()),
            gpio_line: Some(12),
            debounce_ms: 5_001,
            ..GpioInputConfig::default()
        });
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn proxy_trust_list_rejects_unspecified_multicast_and_unbounded_lists() {
        let mut config = Config::default();
        for invalid in ["0.0.0.0", "::", "224.0.0.1", "ff02::1"] {
            config.server.trusted_http_proxies = vec![invalid.parse().unwrap()];
            assert!(validate_config(&config).is_err());
        }
        config.server.trusted_http_proxies = vec!["127.0.0.1".parse().unwrap(); 65];
        assert!(validate_config(&config).is_err());
        config.server.trusted_http_proxies = vec!["127.0.0.1".parse().unwrap(), "192.168.1.2".parse().unwrap()];
        assert!(validate_config(&config).is_ok());
    }
}
