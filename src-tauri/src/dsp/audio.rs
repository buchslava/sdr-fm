use futuresdr::blocks::audio::AudioSink;

/// Prefer the host's default output rate; fall back to 48 kHz.
pub fn output_sample_rate() -> u32 {
    AudioSink::default_sample_rate().unwrap_or(48_000)
}

pub fn audio_sink_error_hint(rate: u32) -> String {
    format!("Audio output failed at {rate} Hz.")
}
