mod command;
mod flowgraph;
mod silence;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread;

use crossbeam_channel::Receiver;
use futuresdr::seify::{Device, GenericDevice};

pub use command::DspCommand;

pub const RTL_SDR_OPEN_ARGS: &[&str] = &[
    "driver=soapy,soapy_driver=rtlsdr",
    "driver=rtlsdr",
];
pub const DEFAULT_SAMPLE_RATE: u32 = 1_024_000;

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
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let _ = flowgraph::run(dev, sample_rate, initial_freq, cmd_rx, quit);
    })
}
