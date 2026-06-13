use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::seify::Builder as SeifyBuilder;
use futuresdr::blocks::{Apply, FirBuilder};
use futuresdr::num_complex::Complex32;
use futuresdr::prelude::*;
use futuresdr::seify::{Device, GenericDevice};

use super::command::{DspCommand, apply_command};
use super::silence;

const DEFAULT_GAIN: f64 = 40.0;
const AUDIO_RATE: u32 = 48_000;
const WBFM_TARGET_RATE: u32 = 256_000;

pub(super) fn run(
    dev: Device<GenericDevice>,
    sample_rate: u32,
    initial_freq: u64,
    cmd_rx: Receiver<DspCommand>,
    quit: Arc<AtomicBool>,
    quit_rx: Receiver<()>,
    ready_tx: Sender<Result<String, String>>,
) -> Result<(), Box<dyn std::error::Error>> {
    match run_pipeline(
        dev,
        sample_rate,
        initial_freq,
        cmd_rx,
        quit,
        quit_rx,
        &ready_tx,
    ) {
        Err(e) => {
            let _ = ready_tx.send(Err(e.to_string()));
            Err(e)
        }
        Ok(()) => Ok(()),
    }
}

fn run_pipeline(
    dev: Device<GenericDevice>,
    sample_rate: u32,
    initial_freq: u64,
    cmd_rx: Receiver<DspCommand>,
    quit: Arc<AtomicBool>,
    quit_rx: Receiver<()>,
    ready_tx: &Sender<Result<String, String>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut fg = Flowgraph::new();

    let wbfm_decim = ((sample_rate as f64) / (WBFM_TARGET_RATE as f64))
        .round()
        .max(3.0) as usize;
    let wbfm_rate = sample_rate / wbfm_decim as u32;

    let src = silence::silenced(|| {
        SeifyBuilder::from_device(dev)
            .frequency(initial_freq as f64)
            .sample_rate(sample_rate as f64)
            .gain(DEFAULT_GAIN)
            .build_source()
    })?;

    let wbfm_decim_block =
        FirBuilder::decimating::<Complex32, Complex32, Vec<f32>>(wbfm_decim);

    let mut last_wbfm = Complex32::new(0.0, 0.0);
    let wbfm_gain = (wbfm_rate as f32) / (2.0 * std::f32::consts::PI * 75_000.0);
    let wbfm_demod = Apply::new(move |c: &Complex32| -> f32 {
        let phase = (c * last_wbfm.conj()).arg();
        last_wbfm = *c;
        phase * wbfm_gain
    });

    let wbfm_resamp =
        FirBuilder::resampling::<f32, f32>(AUDIO_RATE as usize, wbfm_rate as usize);

    let tau_s: f32 = 75e-6;
    let alpha: f32 = (-1.0 / (AUDIO_RATE as f32 * tau_s)).exp();
    let mut y_prev: f32 = 0.0;
    let deemph = Apply::new(move |x: &f32| -> f32 {
        let y = (1.0 - alpha) * *x + alpha * y_prev;
        y_prev = y;
        y
    });

    let volume = Apply::new(|s: &f32| -> f32 { *s * 0.3 });
    let audio_sink = AudioSink::new(AUDIO_RATE, 1).map_err(|e| {
        format!(
            "Audio output failed at {AUDIO_RATE} Hz ({e}). \
             On Linux try: aplay -l, then export SDR_FM_ALSA_DEVICE=plughw:CARD,DEV"
        )
    })?;

    let src = fg.add(src);
    let wbfm_decim_block = fg.add(wbfm_decim_block);
    let wbfm_demod = fg.add(wbfm_demod);
    let wbfm_resamp = fg.add(wbfm_resamp);
    let deemph = fg.add(deemph);
    let volume = fg.add(volume);
    let audio_sink = fg.add(audio_sink);

    fg.stream(&src, |b| b.outputs().get_mut(0).unwrap(), &wbfm_decim_block, |b| b.input())?;
    fg.stream(
        &wbfm_decim_block,
        |b| b.output(),
        &wbfm_demod,
        |b| b.input(),
    )?;
    fg.stream(&wbfm_demod, |b| b.output(), &wbfm_resamp, |b| b.input())?;
    fg.stream(&wbfm_resamp, |b| b.output(), &deemph, |b| b.input())?;
    fg.stream(&deemph, |b| b.output(), &volume, |b| b.input())?;
    fg.stream(&volume, |b| b.output(), &audio_sink, |b| b.input())?;

    let src_id: BlockId = (&src).into();

    let rt = Runtime::new();
    let running = silence::silenced(|| rt.start(fg))?;
    ready_tx
        .send(Ok(format!("{sample_rate} Hz IQ → {AUDIO_RATE} Hz audio")))
        .map_err(|e| format!("Failed to signal DSP ready: {e}"))?;
    let handle = running.handle();

    let cmd_quit = Arc::clone(&quit);
    let pump_handle = handle.clone();
    let pump_thread = std::thread::spawn(move || {
        use futuresdr::futures::executor::block_on;
        loop {
            if cmd_quit.load(Ordering::Acquire) {
                break;
            }
            match cmd_rx.recv_timeout(Duration::from_millis(200)) {
                Ok(cmd) => block_on(apply_command(&pump_handle, src_id, cmd)),
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    let _ = quit_rx.recv();

    let _ = pump_thread.join();
    let _ = futuresdr::futures::executor::block_on(running.stop_and_wait());
    drop(rt);
    Ok(())
}
