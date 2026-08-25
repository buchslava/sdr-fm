//! FM band scan: wideband Welch PSD hops → OS-CFAR occupancy detector → RDS names.
//!
//! Detection is done on the stitched spectrum (see `spectrum` / `detect`). This
//! module only talks to the RTL-SDR, reports progress, and optionally fills
//! RDS PS names on the strongest hits.

use std::collections::HashMap;
use std::f32::consts::PI;
use std::time::{Duration, Instant};

use desperado::dsp::{decimator::Decimator, rotate::Rotate, DspBlock};
use fmradio::fm::PhaseExtractor;
use fmradio::rds::{RdsDecoder, RdsResamplerCustom, StereoDecoderPLL};
use futuresdr::num_complex::Complex32;
use futuresdr::seify::{Device, Direction, GenericDevice, RxStreamer};
use serde::Serialize;

use crate::config::Station;

use super::detect::{detect_fm_channels, DetectedChannel};
use super::spectrum::{
    hop_lo_hz, hop_step_hz, usable_half_hz, SpectrumAccumulator, SpectrumEngine, BAND_END_HZ,
    BAND_START_HZ, FFT_SIZE, GRID_HZ, PREFERRED_SAMPLE_RATES,
};
use super::{is_rtlsdr_valid_sample_rate, open_device};

const MAX_RDS_PEAKS: usize = 15;
const OFFSET_FREQ: i32 = 200_000;
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub phase: String,
    pub current: u32,
    pub total: u32,
    pub mhz: f64,
}

struct RdsDsp {
    rotate: Rotate,
    decimator: Decimator,
    phase_extractor: PhaseExtractor,
    stereo: StereoDecoderPLL,
    mpx_rate: f32,
    offset_freq: i32,
    sample_rate: u32,
}

impl RdsDsp {
    fn new(sample_rate: u32, offset_freq: i32) -> Self {
        let factor = (sample_rate as f32 / FM_BANDWIDTH).round().max(1.0) as usize;
        let mpx_rate = sample_rate as f32 / factor as f32;
        Self {
            rotate: Rotate::new(-2.0 * PI * offset_freq as f32 / sample_rate as f32),
            decimator: Decimator::new(factor),
            phase_extractor: PhaseExtractor::new(),
            stereo: StereoDecoderPLL::new(mpx_rate),
            mpx_rate,
            offset_freq,
            sample_rate,
        }
    }

    fn reset_after_tune(&mut self) {
        self.rotate = Rotate::new(-2.0 * PI * self.offset_freq as f32 / self.sample_rate as f32);
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
        let complex: Vec<num_complex::Complex<f32>> = chunk
            .iter()
            .map(|c| num_complex::Complex::new(c.re, c.im))
            .collect();
        let shifted = self.rotate.process(&complex);
        let decimated = self.decimator.process(&shifted);
        let phase = self.phase_extractor.process(&decimated);
        let (_, _, pilot_phases) = self.stereo.process(&phase);
        let (rds_i, rds_q) = rds_resampler.process_with_pilot(&phase, &pilot_phases);
        rds.process_iq(&rds_i, &rds_q);
    }
}

fn tuning_lo_hz(air_hz: u64, offset_freq_hz: i32) -> f64 {
    if offset_freq_hz >= 0 {
        air_hz.saturating_sub(offset_freq_hz as u64) as f64
    } else {
        air_hz.saturating_add((-offset_freq_hz) as u64) as f64
    }
}

fn read_some(rx: &mut Box<dyn RxStreamer>, buf: &mut [Complex32]) -> Option<usize> {
    for _ in 0..MAX_READ_ATTEMPTS {
        match rx.read(&mut [&mut buf[..]], READ_TIMEOUT_US) {
            Ok(n) if n > 0 => return Some(n),
            Ok(_) => std::thread::sleep(Duration::from_millis(2)),
            Err(_) => std::thread::sleep(Duration::from_millis(2)),
        }
    }
    None
}

fn discard_reads(rx: &mut Box<dyn RxStreamer>, buf: &mut [Complex32], count: u32) {
    for _ in 0..count {
        let _ = read_some(rx, buf);
    }
}

fn tune_lo(
    dev: &Device<GenericDevice>,
    rx: &mut Box<dyn RxStreamer>,
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
    rx: &mut Box<dyn RxStreamer>,
    buf: &mut [Complex32],
    dwell: Duration,
) -> Vec<Complex32> {
    let mut iq = Vec::with_capacity(MAX_IQ_SAMPLES.min((dwell.as_secs_f64() * 2.5e6) as usize));
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
    rx: &mut Box<dyn RxStreamer>,
    buf: &mut [Complex32],
    dsp: &mut RdsDsp,
    air_hz: u64,
    dwell: Duration,
) -> String {
    if tune_lo(dev, rx, buf, tuning_lo_hz(air_hz, OFFSET_FREQ)).is_err() {
        return String::new();
    }
    dsp.reset_after_tune();

    let rds_target_rate = 171_000.0_f32;
    let mut rds_resampler = RdsResamplerCustom::new(dsp.mpx_rate, rds_target_rate);
    let mut rds = RdsDecoder::new(rds_target_rate, false);
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

    let mut rx = dev
        .rx_streamer(&[0])
        .map_err(|e| format!("Failed to create RX streamer: {e}"))?;
    rx.activate()
        .map_err(|e| format!("Failed to activate RX: {e}"))?;

    let mut buf = vec![Complex32::new(0.0, 0.0); BUF_LEN];
    discard_reads(&mut rx, &mut buf, SETTLE_READS);

    let usable_half = usable_half_hz(sample_rate);
    let step = hop_step_hz(usable_half);
    let hops = hop_lo_hz(BAND_START_HZ, BAND_END_HZ, usable_half, step);
    let total_hops = hops.len() as u32;

    let mut engine = SpectrumEngine::new(FFT_SIZE);
    let mut acc = SpectrumAccumulator::new(BAND_START_HZ, BAND_END_HZ, GRID_HZ);

    for (i, &lo_hz) in hops.iter().enumerate() {
        if started.elapsed() > SCAN_DEADLINE {
            break;
        }
        on_progress(ScanProgress {
            phase: "spectrum".into(),
            current: (i as u32) + 1,
            total: total_hops.max(1),
            mhz: lo_hz / 1_000_000.0,
        });

        if tune_lo(&dev, &mut rx, &mut buf, lo_hz).is_err() {
            continue;
        }
        let iq = collect_iq(&mut rx, &mut buf, SPECTRUM_DWELL);
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
        let _ = rx.deactivate();
        return Err("No FM stations found. Try another antenna position or higher gain.".into());
    }

    let mut by_score = detected.clone();
    by_score.sort_by(|a, b| {
        b.snr_db
            .partial_cmp(&a.snr_db)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    by_score.truncate(MAX_RDS_PEAKS);

    let mut dsp = RdsDsp::new(sample_rate, OFFSET_FREQ);
    let mut name_by_freq: HashMap<u32, String> = HashMap::with_capacity(by_score.len());
    let total_rds = by_score.len() as u32;

    for (i, ch) in by_score.iter().enumerate() {
        if started.elapsed() > SCAN_DEADLINE {
            break;
        }
        on_progress(ScanProgress {
            phase: "rds".into(),
            current: (i as u32) + 1,
            total: total_rds.max(1),
            mhz: ch.frequency_khz as f64 / 1_000.0,
        });
        let name = try_rds_name(
            &dev,
            &mut rx,
            &mut buf,
            &mut dsp,
            ch.frequency_khz as u64 * 1_000,
            RDS_DWELL,
        );
        name_by_freq.insert(ch.frequency_khz, name);
    }

    let _ = rx.deactivate();
    drop(rx);
    drop(dev);
    std::thread::sleep(Duration::from_millis(400));

    let mut stations: Vec<Station> = detected
        .into_iter()
        .map(|DetectedChannel { frequency_khz, .. }| Station {
            id: format!("scan-{frequency_khz}"),
            name: name_by_freq.remove(&frequency_khz).unwrap_or_default(),
            frequency_khz,
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
