//! FM band power sweep and peak picking (bounded, crash-safe).

use std::time::{Duration, Instant};

use futuresdr::num_complex::Complex32;
use futuresdr::seify::{Device, Direction, GenericDevice, RxStreamer};
use serde::Serialize;

use crate::config::Station;

use super::rds::RdsPsDecoder;
use super::{is_rtlsdr_valid_sample_rate, open_device};

pub const SCAN_START_KHZ: u32 = 87_500;
pub const SCAN_END_KHZ: u32 = 108_000;
/// 200 kHz steps → ~103 points (faster, still fine for FM channel spacing).
pub const SCAN_STEP_KHZ: u32 = 200;
pub const MIN_PEAK_SEP_KHZ: u32 = 300;
pub const MAX_PEAKS: usize = 20;
/// Only the strongest peaks get an RDS dwell (keeps scan under ~1–2 minutes).
const MAX_RDS_PEAKS: usize = 8;
const SCAN_SAMPLE_RATE: u32 = 256_000;
const DEFAULT_GAIN_DB: f64 = 40.0;
const BUF_LEN: usize = 8_192;
/// Hard cap per read attempt so Soapy/RTL cannot block forever.
const READ_TIMEOUT_US: i64 = 200_000;
const MAX_READ_ATTEMPTS: u32 = 6;
const RDS_DWELL_SECS: f32 = 1.5;
const POWER_MARGIN_DB: f32 = 3.0;
/// Whole-scan wall-clock budget (power + RDS).
const SCAN_DEADLINE: Duration = Duration::from_secs(90);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub phase: String,
    pub current: u32,
    pub total: u32,
    pub mhz: f64,
}

/// Pick local maxima from a power vector (dB). Pure function for unit tests.
pub fn pick_peaks(
    freqs_khz: &[u32],
    power_db: &[f32],
    threshold_db: f32,
    min_sep_khz: u32,
    max_peaks: usize,
) -> Vec<(u32, f32)> {
    assert_eq!(freqs_khz.len(), power_db.len());
    if freqs_khz.len() < 3 {
        return Vec::new();
    }

    let mut candidates: Vec<(u32, f32)> = Vec::new();
    for i in 1..power_db.len() - 1 {
        let p = power_db[i];
        if p < threshold_db {
            continue;
        }
        if p >= power_db[i - 1] && p >= power_db[i + 1] {
            candidates.push((freqs_khz[i], p));
        }
    }

    select_spaced(&mut candidates, min_sep_khz, max_peaks)
}

pub fn pick_top_powers(
    freqs_khz: &[u32],
    power_db: &[f32],
    threshold_db: f32,
    min_sep_khz: u32,
    max_peaks: usize,
) -> Vec<(u32, f32)> {
    assert_eq!(freqs_khz.len(), power_db.len());
    let mut candidates: Vec<(u32, f32)> = freqs_khz
        .iter()
        .zip(power_db.iter())
        .filter(|(_, p)| **p >= threshold_db)
        .map(|(&f, &p)| (f, p))
        .collect();
    select_spaced(&mut candidates, min_sep_khz, max_peaks)
}

fn select_spaced(
    candidates: &mut Vec<(u32, f32)>,
    min_sep_khz: u32,
    max_peaks: usize,
) -> Vec<(u32, f32)> {
    candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut selected: Vec<(u32, f32)> = Vec::new();
    for (freq, power) in candidates.drain(..) {
        if selected
            .iter()
            .any(|(f, _)| f.abs_diff(freq) < min_sep_khz)
        {
            continue;
        }
        selected.push((freq, power));
        if selected.len() >= max_peaks {
            break;
        }
    }

    selected.sort_by_key(|(f, _)| *f);
    selected
}

fn percentile_f32(values: &[f32], pct: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((pct.clamp(0.0, 1.0) * (sorted.len() as f32 - 1.0)).round() as usize)
        .min(sorted.len() - 1);
    sorted[idx]
}

fn center_power_db(samples: &[Complex32]) -> f32 {
    if samples.len() < 64 {
        if samples.is_empty() {
            return -120.0;
        }
        let sum: f32 = samples.iter().map(|c| c.norm_sqr()).sum();
        return 10.0 * ((sum / samples.len() as f32).max(1e-20)).log10();
    }
    let block = 32usize;
    let mut sum = 0.0f32;
    let mut n = 0usize;
    let mut i = 0;
    while i + block <= samples.len() {
        let mut acc = Complex32::new(0.0, 0.0);
        for s in &samples[i..i + block] {
            acc += *s;
        }
        acc /= block as f32;
        sum += acc.norm_sqr();
        n += 1;
        i += block;
    }
    10.0 * ((sum / n.max(1) as f32).max(1e-20)).log10()
}

/// Non-blocking-ish read: returns None on timeout / empty after a few tries.
fn read_some(
    rx: &mut Box<dyn RxStreamer>,
    buf: &mut [Complex32],
) -> Option<usize> {
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

fn tune(
    dev: &Device<GenericDevice>,
    rx: &mut Box<dyn RxStreamer>,
    buf: &mut [Complex32],
    freq_hz: f64,
) -> Result<(), String> {
    // Retune while inactive is more reliable on RTL/Soapy and avoids USB wedging.
    let _ = rx.deactivate();
    dev.set_frequency(Direction::Rx, 0, freq_hz)
        .map_err(|e| format!("Tune failed: {e}"))?;
    std::thread::sleep(Duration::from_millis(5));
    rx.activate()
        .map_err(|e| format!("RX activate failed: {e}"))?;
    discard_reads(rx, buf, 2);
    Ok(())
}

fn measure_power(
    rx: &mut Box<dyn RxStreamer>,
    buf: &mut [Complex32],
) -> Option<f32> {
    discard_reads(rx, buf, 1);
    let n = read_some(rx, buf)?;
    Some(center_power_db(&buf[..n]))
}

/// Feed RDS decoder from live IQ without allocating multi-second buffers.
fn try_rds_name(
    rx: &mut Box<dyn RxStreamer>,
    buf: &mut [Complex32],
    sample_rate: u32,
    dwell_secs: f32,
) -> String {
    let mut decoder = RdsPsDecoder::new(sample_rate);
    let target = ((sample_rate as f32) * dwell_secs) as usize;
    let mut got_total = 0usize;
    let deadline = Instant::now() + Duration::from_secs_f32(dwell_secs + 1.0);

    discard_reads(rx, buf, 2);

    while got_total < target && Instant::now() < deadline {
        let Some(n) = read_some(rx, buf) else {
            break;
        };
        decoder.feed_iq(&buf[..n]);
        got_total += n;
        if decoder.ps_name().is_some() {
            break;
        }
    }

    decoder
        .ps_name()
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn scan_sample_rate() -> u32 {
    if is_rtlsdr_valid_sample_rate(SCAN_SAMPLE_RATE) {
        SCAN_SAMPLE_RATE
    } else {
        1_024_000
    }
}

/// Full band scan: power sweep, peak pick, short RDS dwell on strongest peaks.
pub fn scan_band<F>(mut on_progress: F) -> Result<Vec<Station>, String>
where
    F: FnMut(ScanProgress),
{
    let started = Instant::now();
    let sample_rate = scan_sample_rate();
    let dev = open_device()?;
    configure_device(&dev, sample_rate)?;

    let mut rx = dev
        .rx_streamer(&[0])
        .map_err(|e| format!("Failed to create RX streamer: {e}"))?;
    rx.activate()
        .map_err(|e| format!("Failed to activate RX: {e}"))?;

    let mut buf = vec![Complex32::new(0.0, 0.0); BUF_LEN];
    discard_reads(&mut rx, &mut buf, 2);

    let freqs: Vec<u32> = (SCAN_START_KHZ..=SCAN_END_KHZ)
        .step_by(SCAN_STEP_KHZ as usize)
        .collect();
    let total_power = freqs.len() as u32;
    let mut powers = Vec::with_capacity(freqs.len());

    for (i, &freq_khz) in freqs.iter().enumerate() {
        if started.elapsed() > SCAN_DEADLINE {
            break;
        }
        on_progress(ScanProgress {
            phase: "power".into(),
            current: (i as u32) + 1,
            total: total_power,
            mhz: freq_khz as f64 / 1_000.0,
        });

        let hz = freq_khz as f64 * 1_000.0;
        if let Err(e) = tune(&dev, &mut rx, &mut buf, hz) {
            eprintln!("scan tune {freq_khz}: {e}");
            powers.push(-120.0);
            continue;
        }
        let p = measure_power(&mut rx, &mut buf).unwrap_or(-120.0);
        powers.push(p);
    }

    // Align lengths if we aborted early.
    let freqs = &freqs[..powers.len()];
    if freqs.is_empty() {
        let _ = rx.deactivate();
        return Err("Scan aborted before any frequencies were measured.".into());
    }

    let noise = percentile_f32(&powers, 0.20);
    let max_p = powers
        .iter()
        .cloned()
        .fold(f32::NEG_INFINITY, f32::max);
    let threshold = noise + POWER_MARGIN_DB;

    let mut peaks = pick_peaks(freqs, &powers, threshold, MIN_PEAK_SEP_KHZ, MAX_PEAKS);
    if peaks.is_empty() {
        peaks = pick_top_powers(freqs, &powers, threshold, MIN_PEAK_SEP_KHZ, MAX_PEAKS);
    }
    if peaks.is_empty() && max_p.is_finite() {
        peaks = pick_top_powers(freqs, &powers, max_p - 12.0, MIN_PEAK_SEP_KHZ, MAX_PEAKS);
    }

    eprintln!(
        "FM scan power done in {:.1}s: noise={noise:.1} max={max_p:.1} thr={threshold:.1} peaks={}",
        started.elapsed().as_secs_f32(),
        peaks.len()
    );

    if peaks.is_empty() {
        let _ = rx.deactivate();
        return Err(format!(
            "No FM carriers found (noise={noise:.1} dB, max={max_p:.1} dB). \
             Try another antenna position."
        ));
    }

    // RDS only on strongest few peaks.
    let mut peaks_by_power = peaks.clone();
    peaks_by_power.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    peaks_by_power.truncate(MAX_RDS_PEAKS);

    let mut name_by_freq: std::collections::HashMap<u32, String> =
        std::collections::HashMap::with_capacity(peaks_by_power.len());

    let total_rds = peaks_by_power.len() as u32;
    for (i, &(freq_khz, _)) in peaks_by_power.iter().enumerate() {
        if started.elapsed() > SCAN_DEADLINE {
            eprintln!("FM scan: RDS phase cut short by deadline");
            break;
        }
        on_progress(ScanProgress {
            phase: "rds".into(),
            current: (i as u32) + 1,
            total: total_rds.max(1),
            mhz: freq_khz as f64 / 1_000.0,
        });

        let hz = freq_khz as f64 * 1_000.0;
        if tune(&dev, &mut rx, &mut buf, hz).is_err() {
            name_by_freq.insert(freq_khz, String::new());
            continue;
        }
        let name = try_rds_name(&mut rx, &mut buf, sample_rate, RDS_DWELL_SECS);
        name_by_freq.insert(freq_khz, name);
    }

    let _ = rx.deactivate();

    let mut stations: Vec<Station> = peaks
        .into_iter()
        .map(|(frequency_khz, _)| Station {
            id: format!("scan-{frequency_khz}"),
            name: name_by_freq
                .remove(&frequency_khz)
                .unwrap_or_default(),
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

fn configure_device(dev: &Device<GenericDevice>, sample_rate: u32) -> Result<(), String> {
    if let Ok(true) = dev.supports_agc(Direction::Rx, 0) {
        let _ = dev.enable_agc(Direction::Rx, 0, false);
    }
    dev.set_sample_rate(Direction::Rx, 0, sample_rate as f64)
        .map_err(|e| format!("set_sample_rate failed: {e}"))?;
    if let Err(e) = dev.set_gain(Direction::Rx, 0, DEFAULT_GAIN_DB) {
        eprintln!("scan set_gain({DEFAULT_GAIN_DB}): {e}; continuing");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_separated_local_maxima() {
        let freqs: Vec<u32> = (87_500..=88_500).step_by(100).collect();
        let mut power = vec![0.0f32; freqs.len()];
        for (i, f) in freqs.iter().enumerate() {
            if *f == 87_800 || *f == 88_300 {
                power[i] = 20.0;
            } else if *f == 87_700 || *f == 87_900 || *f == 88_200 || *f == 88_400 {
                power[i] = 10.0;
            }
        }
        let peaks = pick_peaks(&freqs, &power, 8.0, 200, 10);
        let freqs_only: Vec<u32> = peaks.iter().map(|(f, _)| *f).collect();
        assert_eq!(freqs_only, vec![87_800, 88_300]);
    }

    #[test]
    fn respects_min_separation() {
        let freqs = vec![100_000u32, 100_100, 100_200, 100_300];
        let power = vec![5.0, 20.0, 19.0, 5.0];
        let peaks = pick_peaks(&freqs, &power, 8.0, 200, 10);
        assert_eq!(peaks.len(), 1);
        assert_eq!(peaks[0].0, 100_100);
    }

    #[test]
    fn top_powers_finds_carriers_without_sharp_peaks() {
        let freqs: Vec<u32> = (90_000..=91_000).step_by(100).collect();
        let power: Vec<f32> = freqs
            .iter()
            .map(|&f| if (90_400..=90_600).contains(&f) { 12.0 } else { 2.0 })
            .collect();
        let peaks = pick_top_powers(&freqs, &power, 5.0, 200, 5);
        assert!(!peaks.is_empty());
        assert!(peaks.iter().any(|(f, _)| (90_400..=90_600).contains(f)));
    }
}
