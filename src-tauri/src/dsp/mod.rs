mod command;
mod detect;
mod flowgraph;
mod scan;
mod silence;
mod spectrum;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::thread;

use crossbeam_channel::{Receiver, Sender};
use futuresdr::seify::{Device, GenericDevice};

pub use command::DspCommand;
pub use scan::{scan_band, ScanProgress};

pub const RTL_SDR_OPEN_ARGS: &[&str] = &["driver=soapy,soapy_driver=rtlsdr", "driver=rtlsdr"];
pub const DEFAULT_SAMPLE_RATE: u32 = 1_024_000;

/// RTL2832 valid bands: 225_001–300_000 Hz and 900_001–3_200_000 Hz.
/// Rates in (300_000, 900_000] are rejected by librtlsdr (e.g. 768_000, 900_000).
const RTL_SDR_MIN_SAMPLE_RATE: u32 = 225_001;
const RTL_SDR_MAX_SAMPLE_RATE: u32 = 3_200_000;

const RTL_SDR_PREFERRED_RATES: &[u32] = &[
    256_000, 1_024_000, 1_536_000, 1_792_000, 1_920_000, 2_048_000, 2_160_000, 2_560_000,
];

const PLATFORM_DEFAULT_SAMPLE_RATE: u32 = DEFAULT_SAMPLE_RATE;
const SAMPLE_RATE_ENV: &str = "SDR_KITCHEN_SAMPLE_RATE";
const LEGACY_SAMPLE_RATE_ENV: &str = "SDR_FM_SAMPLE_RATE";
const ALSA_DEVICE_ENV: &str = "SDR_KITCHEN_ALSA_DEVICE";
#[cfg(target_os = "linux")]
const LEGACY_ALSA_DEVICE_ENV: &str = "SDR_FM_ALSA_DEVICE";

/// True if `rate` is accepted by the RTL2832 resampler (librtlsdr rules).
pub fn is_rtlsdr_valid_sample_rate(rate: u32) -> bool {
    if !(RTL_SDR_MIN_SAMPLE_RATE..=RTL_SDR_MAX_SAMPLE_RATE).contains(&rate) {
        return false;
    }
    !(300_000 < rate && rate <= 900_000)
}

fn nearest_preferred_rate(requested: u32) -> u32 {
    RTL_SDR_PREFERRED_RATES
        .iter()
        .min_by_key(|&&rate| rate.abs_diff(requested))
        .copied()
        .unwrap_or(DEFAULT_SAMPLE_RATE)
}

fn compatible_env_var(current: &str, legacy: &str) -> Option<String> {
    std::env::var(current)
        .ok()
        .or_else(|| std::env::var(legacy).ok())
}

/// Effective IQ sample rate: `SDR_KITCHEN_SAMPLE_RATE` override, else platform default.
/// The former `SDR_FM_SAMPLE_RATE` name remains supported for compatibility.
/// Invalid RTL-SDR rates (e.g. 768_000) snap to the nearest supported rate.
pub fn effective_sample_rate() -> u32 {
    if let Some(raw) = compatible_env_var(SAMPLE_RATE_ENV, LEGACY_SAMPLE_RATE_ENV) {
        if let Ok(requested) = raw.parse::<u32>() {
            if is_rtlsdr_valid_sample_rate(requested) {
                return requested;
            }
            let snapped = nearest_preferred_rate(requested);
            eprintln!(
                "{SAMPLE_RATE_ENV}={requested} is invalid for RTL-SDR; using {snapped} Hz \
                 (valid bands: 225001–300000 and 900001–3200000)"
            );
            return snapped;
        }
    }
    PLATFORM_DEFAULT_SAMPLE_RATE
}

/// Optional ALSA device override for cpal/rodio (Linux only).
#[cfg(target_os = "linux")]
pub fn configure_linux_audio_env() {
    if let Some(device) = compatible_env_var(ALSA_DEVICE_ENV, LEGACY_ALSA_DEVICE_ENV) {
        unsafe {
            std::env::set_var("ALSA_PCM_DEVICE", device);
        }
    }
}

#[cfg(not(target_os = "linux"))]
pub fn configure_linux_audio_env() {}

pub fn open_device() -> Result<Device<GenericDevice>, String> {
    let mut last_err = String::new();

    for args in RTL_SDR_OPEN_ARGS {
        match silence::silenced(|| Device::from_args(*args)) {
            Ok(dev) => return Ok(dev),
            Err(err) => last_err = err.to_string(),
        }
    }

    Err(format!(
        "Failed to open RTL-SDR via SoapySDR: {last_err}{}",
        missing_module_hint(&last_err)
    ))
}

fn missing_module_hint(err: &str) -> &'static str {
    if err.contains("no match") || err.contains("No devices found") {
        "\n\nInstall the SoapySDR RTL-SDR module:\n  brew install soapyrtlsdr\n\nThen verify the dongle is visible:\n  SoapySDRUtil --probe=\"driver=rtlsdr\""
    } else {
        ""
    }
}

pub fn spawn_dsp_thread(
    dev: Device<GenericDevice>,
    sample_rate: u32,
    initial_freq: u64,
    cmd_rx: Receiver<DspCommand>,
    quit: Arc<AtomicBool>,
    quit_rx: Receiver<()>,
    ready_tx: Sender<Result<String, String>>,
) -> thread::JoinHandle<()> {
    configure_linux_audio_env();
    thread::spawn(move || {
        if let Err(e) = flowgraph::run(
            dev,
            sample_rate,
            initial_freq,
            cmd_rx,
            quit,
            quit_rx,
            ready_tx,
        ) {
            eprintln!("SDR Kitchen DSP error: {e}");
        }
    })
}

pub fn list_output_devices() -> Result<Vec<String>, String> {
    Ok(vec![
        "Audio uses the system default output (cpal/ALSA).".into(),
        "List cards: aplay -l".into(),
        format!("Override: export {ALSA_DEVICE_ENV}=plughw:CARD,DEV"),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_default_is_valid_for_rtlsdr() {
        assert!(is_rtlsdr_valid_sample_rate(PLATFORM_DEFAULT_SAMPLE_RATE));
    }

    #[test]
    fn rejects_rtlsdr_dead_band() {
        assert!(!is_rtlsdr_valid_sample_rate(768_000));
        assert!(!is_rtlsdr_valid_sample_rate(900_000));
        assert!(!is_rtlsdr_valid_sample_rate(500_000));
    }

    #[test]
    fn accepts_common_rates() {
        assert!(is_rtlsdr_valid_sample_rate(256_000));
        assert!(is_rtlsdr_valid_sample_rate(1_024_000));
        assert!(is_rtlsdr_valid_sample_rate(2_048_000));
    }

    #[test]
    fn nearest_preferred_from_dead_band() {
        assert_eq!(nearest_preferred_rate(768_000), 1_024_000);
        assert_eq!(nearest_preferred_rate(900_000), 1_024_000);
    }
}
