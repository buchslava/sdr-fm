use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, SupportedStreamConfig, SupportedStreamConfigRange};

#[derive(Debug, Clone)]
pub struct OutputSetup {
    pub sample_rate: u32,
    /// Channels written to the device (1 or 2).
    pub output_channels: u16,
    pub duplicate_mono: bool,
    pub device_label: String,
    pcm_id: String,
}

impl OutputSetup {
    pub fn pcm_id(&self) -> &str {
        &self.pcm_id
    }
}

pub fn prepare_linux_output() -> Result<OutputSetup, String> {
    let host = cpal::default_host();
    let device = select_output_device(&host)?;
    let pcm_id = device.name().map_err(|e| e.to_string())?;
    let device_label = device_label(&device, &pcm_id);

    let default = device
        .default_output_config()
        .map_err(|e| format!("No usable output config on {device_label}: {e}"))?;

    let (sample_rate, output_channels) = pick_stream_format(&device, &default)?;
    let duplicate_mono = output_channels == 2;

    Ok(OutputSetup {
        sample_rate,
        output_channels,
        duplicate_mono,
        device_label,
        pcm_id,
    })
}

fn select_output_device(host: &cpal::Host) -> Result<Device, String> {
    if let Ok(requested) = std::env::var("SDR_FM_ALSA_DEVICE") {
        return find_device(host, &requested).ok_or_else(|| {
            format!(
                "SDR_FM_ALSA_DEVICE={requested:?} not found. Run: aplay -l"
            )
        });
    }

    // Use the same ALSA default as the rest of the desktop (HDMI display audio,
    // PipeWire, headphones — whatever the user configured in Raspberry Pi OS).
    if let Some(device) = host.default_output_device() {
        if device_supports_playback(&device) {
            return Ok(device);
        }
    }

    host.devices()
        .map_err(|e| format!("Failed to list audio devices: {e}"))?
        .find(|device| device_supports_playback(device))
        .ok_or_else(|| "No audio output device found.".to_string())
}

fn find_device(host: &cpal::Host, pcm_id: &str) -> Option<Device> {
    host.devices().ok()?.find(|device| {
        device
            .name()
            .ok()
            .is_some_and(|name| name == pcm_id || name.contains(pcm_id))
    })
}

fn device_supports_playback(device: &Device) -> bool {
    device
        .supported_output_configs()
        .map(|mut configs| configs.next().is_some())
        .unwrap_or(false)
}

fn device_label(device: &Device, pcm_id: &str) -> String {
    device
        .description()
        .ok()
        .map(|d| d.to_string())
        .unwrap_or_else(|| pcm_id.to_string())
}

fn pick_stream_format(
    device: &Device,
    default: &SupportedStreamConfig,
) -> Result<(u32, u16), String> {
    let configs: Vec<SupportedStreamConfigRange> = device
        .supported_output_configs()
        .map_err(|e| e.to_string())?
        .collect();

    let prefer_rates = [48_000u32, 44_100, 32_000, 22_050];

    for rate in prefer_rates {
        if let Some(channels) = channels_for_rate(&configs, rate) {
            return Ok((rate, channels));
        }
    }

    Ok((
        default.sample_rate(),
        if default.channels() >= 2 { 2 } else { 1 },
    ))
}

fn channels_for_rate(configs: &[SupportedStreamConfigRange], rate: u32) -> Option<u16> {
    for config in configs {
        if rate < config.min_sample_rate() || rate > config.max_sample_rate() {
            continue;
        }
        if config.channels() >= 2 {
            return Some(2);
        }
        if config.channels() == 1 {
            return Some(1);
        }
    }
    None
}

pub fn list_linux_output_devices() -> Result<Vec<String>, String> {
    let host = cpal::default_host();
    let mut lines = Vec::new();

    if let Some(device) = host.default_output_device() {
        let name = device.name().unwrap_or_else(|_| "?".into());
        let desc = device
            .description()
            .ok()
            .map(|d| d.to_string())
            .unwrap_or_default();
        lines.push(format!("{name} — {desc} (system default)"));
    }

    for device in host.devices().map_err(|e| e.to_string())? {
        if !device_supports_playback(&device) {
            continue;
        }
        let name = device.name().unwrap_or_else(|_| "?".into());
        let desc = device
            .description()
            .ok()
            .map(|d| d.to_string())
            .unwrap_or_default();
        let line = format!("{name} — {desc}");
        if !lines.iter().any(|l| l.starts_with(&name)) {
            lines.push(line);
        }
    }

    if lines.is_empty() {
        lines.push("(no playback devices found)".into());
    }

    Ok(lines)
}
