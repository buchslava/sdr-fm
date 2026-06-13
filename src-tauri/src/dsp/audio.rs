use futuresdr::blocks::audio::AudioSink;

/// Prefer the host's default output rate (often 44100 on Pi); fall back to 48 kHz.
pub fn output_sample_rate() -> u32 {
    AudioSink::default_sample_rate().unwrap_or(48_000)
}

#[cfg(target_os = "linux")]
pub fn configure_linux_output() {
    if let Ok(device) = std::env::var("SDR_FM_ALSA_DEVICE") {
        // cpal's ALSA backend honours ALSA_PCM_DEVICE for the default PCM.
        unsafe { std::env::set_var("ALSA_PCM_DEVICE", device); }
    }
}

#[cfg(not(target_os = "linux"))]
pub fn configure_linux_output() {}

pub fn audio_sink_error_hint(rate: u32) -> String {
    format!(
        "Audio output failed at {rate} Hz. \
         On Linux list devices with `aplay -l`, then try e.g. \
         `export SDR_FM_ALSA_DEVICE=plughw:1,0` before starting the app."
    )
}
