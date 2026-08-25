//! FM band scan: wideband Welch PSD hops → OS-CFAR occupancy detector → RDS names.
//!
//! Detection is done on the stitched spectrum (see `spectrum` / `detect`). This
//! module only talks to the RTL-SDR, reports progress, and optionally fills
//! RDS PS names on the strongest hits.

use std::collections::HashMap;
use std::f32::consts::PI;
use std::ops::{Deref, DerefMut};
use std::time::{Duration, Instant};

use desperado::dsp::{decimator::Decimator, rotate::Rotate, DspBlock};
use fmradio::fm::PhaseExtractor;
use fmradio::rds::{RdsDecoder, RdsResamplerCustom, StereoDecoderPLL};
use futuresdr::num_complex::Complex32;
use futuresdr::seify::{Device, Direction, GenericDevice, RxStreamer};
use num_complex::Complex;
use serde::Serialize;

use crate::config::Station;

use super::detect::detect_fm_channels;
use super::spectrum::{
    cmp_f32, hop_lo_hz, hop_step_hz, usable_half_hz, SpectrumAccumulator, SpectrumEngine,
    BAND_END_HZ, BAND_START_HZ, FFT_SIZE, GRID_HZ, PREFERRED_SAMPLE_RATES,
};
use super::{is_rtlsdr_valid_sample_rate, open_device};

const MAX_RDS_PEAKS: usize = 15;
const OFFSET_FREQ_HZ: u64 = 200_000;
const FM_BANDWIDTH: f32 = 256_000.0;
const DEFAULT_GAIN_DB: f64 = 40.0;
const BUF_LEN: usize = 16_384;
const READ_TIMEOUT_US: i64 = 200_000;
const MAX_READ_ATTEMPTS: u32 = 8;
const SETTLE_READS: u32 = 4;
const TUNE_SETTLE: Duration = Duration::from_millis(15);
const SPECTRUM_DWELL: Duration = Duration::from_millis(150);
const RDS_DWELL: Duration = Duration::from_millis(2500);
const SCAN_DEADLINE: Duration = Duration::from_secs(150);
const MAX_IQ_SAMPLES: usize = 400_000;
const RDS_TARGET_RATE: f32 = 171_000.0;
const RELEASE_SLEEP: Duration = Duration::from_millis(400);

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanPhase {
    Spectrum,
    Rds,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub phase: ScanPhase,
    pub current: u32,
    pub total: u32,
    pub mhz: f64,
}

impl ScanProgress {
    fn new(phase: ScanPhase, current: u32, total: u32, hz: f64) -> Self {
        Self {
            phase,
            current,
            total: total.max(1),
            mhz: hz / 1_000_000.0,
        }
    }
}

struct RxSession {
    rx: Box<dyn RxStreamer>,
}

impl RxSession {
    fn open(dev: &Device<GenericDevice>) -> Result<Self, String> {
        let mut rx = dev
            .rx_streamer(&[0])
            .map_err(|e| format!("Failed to create RX streamer: {e}"))?;
        rx.activate()
            .map_err(|e| format!("Failed to activate RX: {e}"))?;
        Ok(Self { rx })
    }
}

impl Drop for RxSession {
    fn drop(&mut self) {
        let _ = self.rx.deactivate();
    }
}

impl Deref for RxSession {
    type Target = dyn RxStreamer;

    fn deref(&self) -> &Self::Target {
        &*self.rx
    }
}

impl DerefMut for RxSession {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.rx
    }
}

struct RdsDsp {
    rotate: Rotate,
    decimator: Decimator,
    phase_extractor: PhaseExtractor,
    stereo: StereoDecoderPLL,
    mix_buf: Vec<Complex<f32>>,
    mpx_rate: f32,
    sample_rate: u32,
}

impl RdsDsp {
    fn new(sample_rate: u32) -> Self {
        let factor = (sample_rate as f32 / FM_BANDWIDTH).round().max(1.0) as usize;
        let mpx_rate = sample_rate as f32 / factor as f32;
        Self {
            rotate: rotate_for(sample_rate),
            decimator: Decimator::new(factor),
            phase_extractor: PhaseExtractor::new(),
            stereo: StereoDecoderPLL::new(mpx_rate),
            mix_buf: Vec::new(),
            mpx_rate,
            sample_rate,
        }
    }

    fn reset_after_tune(&mut self) {
        self.rotate = rotate_for(self.sample_rate);
        self.decimator.reset();
        self.phase_extractor.reset();
        self.stereo = StereoDecoderPLL::new(self.mpx_rate);
    }

    fn feed_rds(
        &mut self,
        chunk: &[Complex32],
        rds_resampler: &mut RdsResamplerCustom,
        rds: &mut RdsDecoder,
    ) {
        self.mix_buf.clear();
        self.mix_buf
            .extend(chunk.iter().map(|c| Complex::new(c.re, c.im)));
        let shifted = self.rotate.process(&self.mix_buf);
        let decimated = self.decimator.process(&shifted);
        let phase = self.phase_extractor.process(&decimated);
        let (_, _, pilot_phases) = self.stereo.process(&phase);
        let (rds_i, rds_q) = rds_resampler.process_with_pilot(&phase, &pilot_phases);
        rds.process_iq(&rds_i, &rds_q);
    }
}

fn rotate_for(sample_rate: u32) -> Rotate {
    Rotate::new(-2.0 * PI * OFFSET_FREQ_HZ as f32 / sample_rate as f32)
}

fn tuning_lo_hz(air_hz: u64) -> f64 {
    air_hz.saturating_sub(OFFSET_FREQ_HZ) as f64
}

fn read_some(rx: &mut dyn RxStreamer, buf: &mut [Complex32]) -> Option<usize> {
    for _ in 0..MAX_READ_ATTEMPTS {
        match rx.read(&mut [&mut buf[..]], READ_TIMEOUT_US) {
            Ok(n) if n > 0 => return Some(n),
            _ => std::thread::sleep(Duration::from_millis(2)),
        }
    }
    None
}

fn discard_reads(rx: &mut dyn RxStreamer, buf: &mut [Complex32], count: u32) {
    for _ in 0..count {
        let _ = read_some(rx, buf);
    }
}

fn tune_lo(
    dev: &Device<GenericDevice>,
    rx: &mut dyn RxStreamer,
    buf: &mut [Complex32],
    lo_hz: f64,
) -> Result<(), String> {
    let _ = rx.deactivate();
    dev.set_frequency(Direction::Rx, 0, lo_hz)
        .map_err(|e| format!("Tune failed: {e}"))?;
    std::thread::sleep(TUNE_SETTLE);
    rx.activate()
        .map_err(|e| format!("RX activate failed: {e}"))?;
    discard_reads(rx, buf, SETTLE_READS);
    Ok(())
}

fn collect_iq(
    rx: &mut dyn RxStreamer,
    buf: &mut [Complex32],
    dwell: Duration,
    sample_rate: u32,
) -> Vec<Complex32> {
    let expected = ((dwell.as_secs_f64() * f64::from(sample_rate)) as usize).min(MAX_IQ_SAMPLES);
    let mut iq = Vec::with_capacity(expected);
    let deadline = Instant::now() + dwell;
    while Instant::now() < deadline && iq.len() < MAX_IQ_SAMPLES {
        let Some(n) = read_some(rx, buf) else {
            break;
        };
        iq.extend_from_slice(&buf[..n]);
    }
    iq
}

fn try_rds_name(
    dev: &Device<GenericDevice>,
    rx: &mut dyn RxStreamer,
    buf: &mut [Complex32],
    dsp: &mut RdsDsp,
    air_hz: u64,
    dwell: Duration,
) -> String {
    if tune_lo(dev, rx, buf, tuning_lo_hz(air_hz)).is_err() {
        return String::new();
    }
    dsp.reset_after_tune();

    let mut rds_resampler = RdsResamplerCustom::new(dsp.mpx_rate, RDS_TARGET_RATE);
    let mut rds = RdsDecoder::new(RDS_TARGET_RATE, false);
    rds.set_print_json_output(false);

    let deadline = Instant::now() + dwell;
    while Instant::now() < deadline {
        let Some(n) = read_some(rx, buf) else {
            break;
        };
        dsp.feed_rds(&buf[..n], &mut rds_resampler, &mut rds);
        if let Some(name) = rds.station_name() {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }

    rds.station_name()
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn configure_device(dev: &Device<GenericDevice>) -> Result<u32, String> {
    if let Ok(true) = dev.supports_agc(Direction::Rx, 0) {
        let _ = dev.enable_agc(Direction::Rx, 0, false);
    }
    if let Err(e) = dev.set_gain(Direction::Rx, 0, DEFAULT_GAIN_DB) {
        eprintln!("scan set_gain({DEFAULT_GAIN_DB}): {e}; continuing");
    }

    let mut last_err = String::new();
    for &rate in PREFERRED_SAMPLE_RATES {
        if !is_rtlsdr_valid_sample_rate(rate) {
            continue;
        }
        match dev.set_sample_rate(Direction::Rx, 0, rate as f64) {
            Ok(()) => return Ok(rate),
            Err(e) => last_err = e.to_string(),
        }
    }

    Err(format!("set_sample_rate failed: {last_err}"))
}

pub fn scan_band<F>(mut on_progress: F) -> Result<Vec<Station>, String>
where
    F: FnMut(ScanProgress),
{
    let started = Instant::now();
    let dev = open_device()?;
    let sample_rate = configure_device(&dev)?;

    let mut rx = RxSession::open(&dev)?;
    let mut buf = vec![Complex32::new(0.0, 0.0); BUF_LEN];
    discard_reads(&mut *rx, &mut buf, SETTLE_READS);

    let usable_half = usable_half_hz(sample_rate);
    let step = hop_step_hz(usable_half);
    let hops = hop_lo_hz(BAND_START_HZ, BAND_END_HZ, usable_half, step);
    let total_hops = hops.len() as u32;

    let mut engine = SpectrumEngine::new();
    let mut acc = SpectrumAccumulator::new(BAND_START_HZ, BAND_END_HZ, GRID_HZ);

    for (i, &lo_hz) in hops.iter().enumerate() {
        if started.elapsed() > SCAN_DEADLINE {
            break;
        }
        on_progress(ScanProgress::new(
            ScanPhase::Spectrum,
            i as u32 + 1,
            total_hops,
            lo_hz,
        ));

        if tune_lo(&dev, &mut *rx, &mut buf, lo_hz).is_err() {
            continue;
        }
        let iq = collect_iq(&mut *rx, &mut buf, SPECTRUM_DWELL, sample_rate);
        if iq.len() < FFT_SIZE {
            continue;
        }
        let power = engine.welch_power(&iq);
        acc.accumulate_hop(lo_hz, &power, sample_rate, usable_half);
    }

    let spectrum = acc.finish();
    let detected = detect_fm_channels(&spectrum);

    eprintln!(
        "FM scan spectrum done in {:.1}s ({} hops @ {} Hz): {} channels",
        started.elapsed().as_secs_f32(),
        hops.len(),
        sample_rate,
        detected.len()
    );
    for ch in &detected {
        eprintln!(
            "  {:.1} MHz  SNR={:.1} dB  occ={:.0}%  Beq={:.0} kHz",
            ch.frequency_khz as f64 / 1_000.0,
            ch.snr_db,
            ch.occupancy * 100.0,
            ch.bandwidth_hz / 1_000.0
        );
    }

    if detected.is_empty() {
        return Err("No FM stations found. Try another antenna position or higher gain.".into());
    }

    let mut rds_order: Vec<usize> = (0..detected.len()).collect();
    rds_order.sort_by(|&a, &b| cmp_f32(detected[b].snr_db, detected[a].snr_db));
    rds_order.truncate(MAX_RDS_PEAKS);

    let mut dsp = RdsDsp::new(sample_rate);
    let mut name_by_freq = HashMap::with_capacity(rds_order.len());
    let total_rds = rds_order.len() as u32;

    for (i, &idx) in rds_order.iter().enumerate() {
        if started.elapsed() > SCAN_DEADLINE {
            break;
        }
        let ch = detected[idx];
        on_progress(ScanProgress::new(
            ScanPhase::Rds,
            i as u32 + 1,
            total_rds,
            ch.frequency_khz as f64 * 1_000.0,
        ));
        let name = try_rds_name(
            &dev,
            &mut *rx,
            &mut buf,
            &mut dsp,
            ch.frequency_khz as u64 * 1_000,
            RDS_DWELL,
        );
        name_by_freq.insert(ch.frequency_khz, name);
    }

    drop(rx);
    drop(dev);
    std::thread::sleep(RELEASE_SLEEP);

    let mut stations: Vec<Station> = detected
        .into_iter()
        .map(|ch| Station {
            id: format!("scan-{}", ch.frequency_khz),
            name: name_by_freq.remove(&ch.frequency_khz).unwrap_or_default(),
            frequency_khz: ch.frequency_khz,
        })
        .collect();
    stations.sort_by_key(|s| s.frequency_khz);

    eprintln!(
        "FM scan finished in {:.1}s with {} stations",
        started.elapsed().as_secs_f32(),
        stations.len()
    );

    Ok(stations)
}
