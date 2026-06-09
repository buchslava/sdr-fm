use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use crossbeam_channel::bounded;

use crate::dsp::{self, DspCommand, DEFAULT_SAMPLE_RATE};

const FM_MIN_KHZ: u32 = 64_000;
const FM_MAX_KHZ: u32 = 1_080_000;

pub struct SdrPlayer {
    inner: Mutex<Supervisor>,
}

struct Supervisor {
    thread: Option<DspThread>,
}

struct DspThread {
    handle: std::thread::JoinHandle<()>,
    quit: Arc<AtomicBool>,
}

impl Default for SdrPlayer {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Supervisor { thread: None }),
        }
    }
}

impl SdrPlayer {
    pub fn start(&self, frequency_khz: u32) -> Result<String, String> {
        if !(FM_MIN_KHZ..=FM_MAX_KHZ).contains(&frequency_khz) {
            return Err(format!(
                "Frequency must be between {FM_MIN_KHZ} and {FM_MAX_KHZ} kHz (FM band)"
            ));
        }

        self.stop_internal()?;

        let frequency_hz = frequency_khz as u64 * 1_000;
        let mut supervisor = self.lock_inner()?;
        supervisor.connect(frequency_hz)?;

        Ok(format!(
            "Tuned to {:.3} MHz (WBFM)",
            frequency_khz as f64 / 1000.0
        ))
    }

    pub fn stop(&self) -> Result<(), String> {
        self.stop_internal()
    }

    fn stop_internal(&self) -> Result<(), String> {
        self.lock_inner()?.disconnect();
        Ok(())
    }

    fn lock_inner(&self) -> Result<std::sync::MutexGuard<'_, Supervisor>, String> {
        self.inner
            .lock()
            .map_err(|e: PoisonError<_>| e.to_string())
    }
}

impl Supervisor {
    fn connect(&mut self, frequency_hz: u64) -> Result<(), String> {
        self.disconnect();

        let dev = dsp::open_device()?;
        let (_cmd_tx, cmd_rx) = bounded::<DspCommand>(16);
        let quit = Arc::new(AtomicBool::new(false));

        let handle = dsp::spawn_dsp_thread(
            dev,
            DEFAULT_SAMPLE_RATE,
            frequency_hz,
            cmd_rx,
            Arc::clone(&quit),
        );

        self.thread = Some(DspThread { handle, quit });

        Ok(())
    }

    fn disconnect(&mut self) {
        if let Some(thread) = self.thread.take() {
            thread.quit.store(true, Ordering::Relaxed);
            let _ = thread.handle.join();
        }
    }
}
