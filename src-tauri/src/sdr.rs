use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use crossbeam_channel::{Sender, bounded};

use crate::dsp::{self, DspCommand};

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
    cmd_tx: Sender<DspCommand>,
    quit_tx: Sender<()>,
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

        let frequency_hz = frequency_khz as u64 * 1_000;
        let message = format!(
            "Tuned to {:.3} MHz (WBFM)",
            frequency_khz as f64 / 1000.0
        );

        {
            let supervisor = self.lock_inner()?;
            if let Some(thread) = &supervisor.thread {
                thread
                    .cmd_tx
                    .send(DspCommand::TuneFrequency(frequency_hz as u32))
                    .map_err(|e| format!("Failed to tune: {e}"))?;
                return Ok(message);
            }
        }

        self.stop_internal()?;

        let dev = dsp::open_device()?;
        let sample_rate = dsp::effective_sample_rate();

        let mut supervisor = self.lock_inner()?;
        supervisor.spawn_pipeline(dev, sample_rate, frequency_hz)?;

        Ok(message)
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
    fn spawn_pipeline(
        &mut self,
        dev: futuresdr::seify::Device<futuresdr::seify::GenericDevice>,
        sample_rate: u32,
        frequency_hz: u64,
    ) -> Result<(), String> {
        self.disconnect();

        let (cmd_tx, cmd_rx) = bounded::<DspCommand>(16);
        let (quit_tx, quit_rx) = bounded::<()>(1);
        let quit = Arc::new(AtomicBool::new(false));

        let handle = dsp::spawn_dsp_thread(
            dev,
            sample_rate,
            frequency_hz,
            cmd_rx,
            Arc::clone(&quit),
            quit_rx,
        );

        self.thread = Some(DspThread {
            handle,
            quit,
            cmd_tx,
            quit_tx,
        });

        Ok(())
    }

    fn disconnect(&mut self) {
        if let Some(thread) = self.thread.take() {
            thread.quit.store(true, Ordering::Release);
            drop(thread.cmd_tx);
            let _ = thread.quit_tx.send(());
            let _ = thread.handle.join();
        }
    }
}
