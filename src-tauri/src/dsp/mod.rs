mod audio;
mod command;
mod flowgraph;
#[cfg(target_os = "linux")]
mod linux_audio;
#[cfg(target_os = "linux")]
mod linux_audio_sink;
mod silence;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread;

use crossbeam_channel::{Receiver, Sender};
use futuresdr::seify::{Device, GenericDevice};

pub use command::DspCommand;

pub const RTL_SDR_OPEN_ARGS: &[&str] = &[
    "driver=soapy,soapy_driver=rtlsdr",
    "driver=rtlsdr",
];
pub const DEFAULT_SAMPLE_RATE: u32 = 1_024_000;

const MIN_SAMPLE_RATE: u32 = 768_000;
const MAX_SAMPLE_RATE: u32 = 3_200_000;

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const PLATFORM_DEFAULT_SAMPLE_RATE: u32 = 768_000;

#[cfg(all(target_os = "linux", not(target_arch = "aarch64")))]
const PLATFORM_DEFAULT_SAMPLE_RATE: u32 = DEFAULT_SAMPLE_RATE;

#[cfg(not(target_os = "linux"))]
const PLATFORM_DEFAULT_SAMPLE_RATE: u32 = DEFAULT_SAMPLE_RATE;

/// Effective IQ sample rate: `SDR_FM_SAMPLE_RATE` env override, else platform default.
pub fn effective_sample_rate() -> u32 {
    if let Ok(raw) = std::env::var("SDR_FM_SAMPLE_RATE") {
        if let Ok(rate) = raw.parse::<u32>() {
            if (MIN_SAMPLE_RATE..=MAX_SAMPLE_RATE).contains(&rate) {
                return rate;
            }
        }
    }
    PLATFORM_DEFAULT_SAMPLE_RATE
}

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
            eprintln!("SDR FM DSP error: {e}");
        }
    })
}

#[cfg(target_os = "linux")]
pub fn list_output_devices() -> Result<Vec<String>, String> {
    linux_audio::list_linux_output_devices()
}

#[cfg(not(target_os = "linux"))]
pub fn list_output_devices() -> Result<Vec<String>, String> {
    Ok(vec!["Use system default audio output.".into()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_default_is_in_valid_range() {
        let rate = effective_sample_rate();
        assert!((MIN_SAMPLE_RATE..=MAX_SAMPLE_RATE).contains(&rate));
    }
}
