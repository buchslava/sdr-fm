//! Wideband PSD estimate for FM-band survey (the same picture SDR++ shows).
//!
//! RTL-SDR cannot capture 87.5–108 MHz in one shot. Each hop records complex IQ
//! at a few Msps, a Hann-windowed Welch periodogram is formed, DC and analog
//! filter edges are discarded, and hops are averaged onto a common frequency
//! grid. That composite PSD is what the detector sees.

use std::cmp::Ordering;
use std::f32::consts::PI;
use std::sync::Arc;

use futuresdr::num_complex::Complex32;
use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

/// ITU Region 1 broadcast FM (CCIR).
pub const BAND_START_HZ: f64 = 87_500_000.0;
pub const BAND_END_HZ: f64 = 108_000_000.0;

/// Preferred scan rate: 2.048 Msps is a native RTL2832 rate with ~1.6 MHz usable.
pub const PREFERRED_SAMPLE_RATES: &[u32] = &[2_048_000, 1_024_000];

pub const FFT_SIZE: usize = 4096;
/// Composite grid: 5 kHz is fine vs Carson BW (~180 kHz) and the 100 kHz raster.
pub const GRID_HZ: f64 = 5_000.0;
/// Blank the LO/ADC spike and near-DC 1/f (well inside one FM channel).
pub const DC_BLANK_HZ: f64 = 80_000.0;
/// Keep the inner 80 % of Nyquist; RTL-SDR analog filters roll off outside that.
const USABLE_NYQUIST_FRAC: f64 = 0.80;

pub(super) fn cmp_f32(a: f32, b: f32) -> Ordering {
    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
}

pub struct SpectrumEngine {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    spec_scratch: Vec<Complex<f32>>,
    fft_buf: Vec<Complex<f32>>,
}

impl SpectrumEngine {
    pub fn new() -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);
        let scratch_len = fft.get_inplace_scratch_len();
        Self {
            spec_scratch: vec![Complex::new(0.0, 0.0); scratch_len],
            fft,
            window: hann(FFT_SIZE),
            fft_buf: vec![Complex::new(0.0, 0.0); FFT_SIZE],
        }
    }

    /// Welch PSD, native FFT order (bin 0 = DC / LO). Linear power, arbitrary scale.
    pub fn welch_power(&mut self, iq: &[Complex32]) -> Vec<f64> {
        let n = self.fft.len();
        let hop = n / 2;
        let mut acc = vec![0.0f64; n];
        let mut count = 0usize;
        let mut offset = 0usize;

        while offset + n <= iq.len() {
            let frame = &iq[offset..offset + n];
            for ((slot, sample), &w) in self.fft_buf.iter_mut().zip(frame).zip(&self.window) {
                *slot = Complex::new(sample.re * w, sample.im * w);
            }
            self.fft
                .process_with_scratch(&mut self.fft_buf, &mut self.spec_scratch);
            for (acc_bin, sample) in acc.iter_mut().zip(self.fft_buf.iter()) {
                *acc_bin += sample.norm_sqr() as f64;
            }
            count += 1;
            offset += hop;
        }

        if count == 0 {
            return acc;
        }
        let scale = 1.0 / count as f64;
        for p in &mut acc {
            *p *= scale;
        }
        acc
    }
}

/// Half-width of the usable (non-edge) sideband around the LO, in Hz.
pub fn usable_half_hz(sample_rate: u32) -> f64 {
    (sample_rate as f64 / 2.0) * USABLE_NYQUIST_FRAC
}

/// LO step: large enough to clear the DC hole of the previous hop, small enough
/// to stay inside the usable sideband (`DC_BLANK < step ≤ usable_half`).
pub fn hop_step_hz(usable_half: f64) -> f64 {
    (usable_half * 0.73).clamp(DC_BLANK_HZ * 2.0, usable_half)
}

pub fn hop_lo_hz(start_hz: f64, end_hz: f64, usable_half: f64, step: f64) -> Vec<f64> {
    let first = start_hz + usable_half;
    let last = end_hz - usable_half;
    if last <= first {
        return vec![(start_hz + end_hz) * 0.5];
    }

    let estimate = ((last - first) / step).ceil() as usize + 2;
    let mut los = Vec::with_capacity(estimate);
    let mut lo = first;
    while lo < last - 1.0 {
        los.push(lo);
        lo += step;
    }
    match los.last_mut() {
        Some(prev) if (last - *prev).abs() < step * 0.25 => *prev = last,
        Some(_) => los.push(last),
        None => los.push(last),
    }
    los
}

fn hann(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / n as f32).cos())
        .collect()
}

fn bin_freq_hz(k: usize, fft_size: usize, sample_rate: u32, lo_hz: f64) -> f64 {
    let n = fft_size as f64;
    let fs = sample_rate as f64;
    let offset = if k < fft_size / 2 {
        k as f64 * fs / n
    } else {
        k as f64 * fs / n - fs
    };
    lo_hz + offset
}

/// Running average of hop PSDs onto a uniform grid spanning the FM band.
pub struct SpectrumAccumulator {
    start_hz: f64,
    end_hz: f64,
    bin_hz: f64,
    power: Vec<f64>,
    weight: Vec<f64>,
}

impl SpectrumAccumulator {
    pub fn new(start_hz: f64, end_hz: f64, bin_hz: f64) -> Self {
        let n = ((end_hz - start_hz) / bin_hz).round() as usize + 1;
        Self {
            start_hz,
            end_hz,
            bin_hz,
            power: vec![0.0; n],
            weight: vec![0.0; n],
        }
    }

    pub fn accumulate_hop(
        &mut self,
        lo_hz: f64,
        power: &[f64],
        sample_rate: u32,
        usable_half: f64,
    ) {
        let fft_size = power.len();
        for (k, &p) in power.iter().enumerate() {
            if p <= 0.0 {
                continue;
            }
            let freq = bin_freq_hz(k, fft_size, sample_rate, lo_hz);
            if freq < self.start_hz || freq > self.end_hz {
                continue;
            }
            let df = (freq - lo_hz).abs();
            if df <= DC_BLANK_HZ || df > usable_half {
                continue;
            }
            let idx = ((freq - self.start_hz) / self.bin_hz).round();
            if idx < 0.0 {
                continue;
            }
            let idx = idx as usize;
            if let (Some(acc), Some(w)) = (self.power.get_mut(idx), self.weight.get_mut(idx)) {
                *acc += p;
                *w += 1.0;
            }
        }
    }

    pub fn finish(self) -> BandSpectrum {
        let mut power_db = vec![f32::NEG_INFINITY; self.power.len()];
        for ((acc, weight), out) in self.power.iter().zip(&self.weight).zip(power_db.iter_mut()) {
            if *weight > 0.0 {
                *out = (10.0 * (*acc / *weight).log10()) as f32;
            }
        }
        fill_gaps(&mut power_db);
        BandSpectrum {
            start_hz: self.start_hz,
            bin_hz: self.bin_hz,
            power_db,
        }
    }
}

fn fill_gaps(power_db: &mut [f32]) {
    let n = power_db.len();
    let mut last: Option<(usize, f32)> = None;
    for i in 0..n {
        let value = power_db[i];
        if !value.is_finite() {
            continue;
        }
        if let Some((j, v)) = last {
            if i > j + 1 {
                let span = (i - j) as f32;
                let (head, _) = power_db.split_at_mut(i);
                for (k, slot) in head.iter_mut().enumerate().skip(j + 1) {
                    let t = (k - j) as f32 / span;
                    *slot = v + t * (value - v);
                }
            }
        }
        last = Some((i, value));
    }

    let finite: Vec<f32> = power_db.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        power_db.fill(-200.0);
        return;
    }
    let coverage = finite.len() as f32 / n as f32;
    let fill = if coverage > 0.5 {
        percentile(&finite, 0.2)
    } else {
        finite.iter().copied().fold(f32::INFINITY, f32::min) - 20.0
    };
    for v in power_db.iter_mut() {
        if !v.is_finite() {
            *v = fill;
        }
    }
}

pub fn percentile(values: &[f32], pct: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| cmp_f32(*a, *b));
    let idx = ((pct.clamp(0.0, 1.0) * (sorted.len() as f32 - 1.0)).round() as usize)
        .min(sorted.len() - 1);
    sorted[idx]
}

#[derive(Debug, Clone)]
pub struct BandSpectrum {
    pub start_hz: f64,
    pub bin_hz: f64,
    pub power_db: Vec<f32>,
}

impl BandSpectrum {
    pub fn freq_hz(&self, idx: usize) -> f64 {
        self.start_hz + idx as f64 * self.bin_hz
    }

    pub fn index_of(&self, freq_hz: f64) -> Option<usize> {
        let idx = ((freq_hz - self.start_hz) / self.bin_hz).round();
        if idx < 0.0 {
            return None;
        }
        let idx = idx as usize;
        (idx < self.power_db.len()).then_some(idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hops_cover(sample_rate: u32) -> bool {
        let usable = usable_half_hz(sample_rate);
        let step = hop_step_hz(usable);
        let los = hop_lo_hz(BAND_START_HZ, BAND_END_HZ, usable, step);
        let mut f = BAND_START_HZ;
        while f <= BAND_END_HZ + 1.0 {
            let covered = los.iter().any(|&lo| {
                let df = (f - lo).abs();
                df > DC_BLANK_HZ && df <= usable + 1.0
            });
            if !covered {
                return false;
            }
            f += GRID_HZ;
        }
        true
    }

    #[test]
    fn hann_is_periodic_and_peaks_at_centre() {
        let w = hann(32);
        assert!(w[0].abs() < 1e-6);
        assert!((w[16] - 1.0).abs() < 1e-5);
        assert!(w[31] < 0.02);
    }

    #[test]
    fn preferred_rates_cover_the_fm_band() {
        assert!(hops_cover(2_048_000));
        assert!(hops_cover(1_024_000));
    }

    #[test]
    fn hop_step_clears_dc_and_stays_in_sideband() {
        let usable = usable_half_hz(2_048_000);
        let step = hop_step_hz(usable);
        assert!(step > DC_BLANK_HZ);
        assert!(step <= usable);
    }

    #[test]
    fn sinusoid_welch_peaks_at_offset() {
        let fs = 2_048_000u32;
        let tone_hz = 200_000.0f32;
        let n = FFT_SIZE * 8;
        let mut iq = Vec::with_capacity(n);
        for i in 0..n {
            let phase = 2.0 * PI * tone_hz * i as f32 / fs as f32;
            iq.push(Complex32::new(phase.cos(), phase.sin()));
        }
        let mut engine = SpectrumEngine::new();
        let psd = engine.welch_power(&iq);
        let bin_hz = fs as f64 / FFT_SIZE as f64;
        let expected = (tone_hz as f64 / bin_hz).round() as usize;
        let peak = psd
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(Ordering::Equal))
            .map(|(i, _)| i)
            .unwrap();
        assert!(
            peak.abs_diff(expected) <= 1,
            "peak bin {peak}, expected {expected}"
        );
    }

    #[test]
    fn accumulator_maps_dc_offset_bin_to_absolute_frequency() {
        let mut acc = SpectrumAccumulator::new(BAND_START_HZ, BAND_END_HZ, GRID_HZ);
        let mut power = vec![0.0f64; FFT_SIZE];
        let tone_bin = 400usize;
        power[tone_bin] = 1.0;
        let lo = 98_000_000.0;
        let fs = 2_048_000u32;
        acc.accumulate_hop(lo, &power, fs, usable_half_hz(fs));
        let spec = acc.finish();
        let expected = lo + tone_bin as f64 * fs as f64 / FFT_SIZE as f64;
        let idx = spec.index_of(expected).unwrap();
        let peak = spec
            .power_db
            .iter()
            .enumerate()
            .max_by(|a, b| cmp_f32(*a.1, *b.1))
            .map(|(i, _)| i)
            .unwrap();
        assert!((peak as i32 - idx as i32).abs() <= 1);
    }

    #[test]
    fn freq_hz_round_trips_with_index_of() {
        let spec = BandSpectrum {
            start_hz: BAND_START_HZ,
            bin_hz: GRID_HZ,
            power_db: vec![0.0; 16],
        };
        assert_eq!(spec.index_of(spec.freq_hz(7)), Some(7));
    }
}
