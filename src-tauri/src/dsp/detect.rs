//! Broadcast-FM detector on a wideband PSD.
//!
//! Hypothesis per ITU Region 1 channel (87.5 MHz + n × 100 kHz):
//! H1 = a WBFM signal whose RF occupancy is Carson's bandwidth
//!      B = 2(Δf + f_m) = 2(75 kHz + 15 kHz) = 180 kHz
//!      (stereo MPX / RDS energy sits in the same haystack).
//!
//! Test (textbook, not a heuristic score):
//! 1. OS-CFAR noise from training cells with a guard band of one FM channel
//!    so the station under test is not counted as noise.
//! 2. Radiometer: mean *linear* power in the 180 kHz test cell versus that
//!    noise (Neyman–Pearson for unknown amplitude in additive noise).
//! 3. Occupancy / equivalent rectangular bandwidth to reject LO birdies
//!    and CW spurs (a haystack is wide; a spike is not).
//! 4. Connected occupancy blobs: adjacent eligible raster channels are one
//!    haystack. Keep the highest-SNR channel in each blob. A deep SNR valley
//!    (≥ 6 dB) splits two real transmitters that happen to touch.
//!
//! This is the discrete version of “click the centre of the haystack in SDR++”.

use super::spectrum::{cmp_f32, percentile, BandSpectrum, GRID_HZ};

/// ITU-R BS.450 Region 1 channel step.
pub const RASTER_KHZ: u32 = 100;
pub const BAND_START_KHZ: u32 = 87_500;
pub const BAND_END_KHZ: u32 = 108_000;

/// Carson mono bandwidth; stereo looks the same on a 5 kHz grid.
const TEST_HALF_HZ: f64 = 90_000.0;
/// Exclude the station (and Bessel skirts) from the noise estimate.
const GUARD_HZ: f64 = 120_000.0;
/// Training cells on each side (~0.8 MHz).
const TRAIN_HZ: f64 = 800_000.0;
/// 25th percentile: robust when a few other stations sit in the training window.
const NOISE_PERCENTILE: f32 = 0.25;
/// Mean excess in the test cell. After Welch averaging this is many σ above H0.
const SNR_THRESHOLD_DB: f32 = 4.5;
/// Bins in the test cell that must sit ≥ 3 dB above CFAR noise.
const MIN_OCCUPANCY: f32 = 0.30;
const OCCUPANCY_BIN_DB: f32 = 3.0;
/// Equivalent rectangular bandwidth of excess power inside the test cell.
const MIN_BEQ_HZ: f32 = 55_000.0;
/// One WBFM occupancy is ~180 kHz, so 200 kHz raster twins are the same station.
const MIN_SEP_KHZ: u32 = 250;
/// Split a blob only when two peaks are separated by a real valley, not a stitch dip.
const VALLEY_DB: f32 = 6.0;
const MAX_CHANNELS: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectedChannel {
    pub frequency_khz: u32,
    pub snr_db: f32,
    pub occupancy: f32,
    pub bandwidth_hz: f32,
}

impl DetectedChannel {
    fn rejected(frequency_khz: u32) -> Self {
        Self {
            frequency_khz,
            snr_db: f32::NEG_INFINITY,
            occupancy: 0.0,
            bandwidth_hz: 0.0,
        }
    }

    fn is_eligible(self) -> bool {
        self.snr_db >= SNR_THRESHOLD_DB
            && self.occupancy >= MIN_OCCUPANCY
            && self.bandwidth_hz >= MIN_BEQ_HZ
    }

    fn quality_cmp(self, other: Self) -> std::cmp::Ordering {
        cmp_f32(self.snr_db, other.snr_db).then_with(|| cmp_f32(self.occupancy, other.occupancy))
    }
}

pub fn detect_fm_channels(spectrum: &BandSpectrum) -> Vec<DetectedChannel> {
    let stats: Vec<DetectedChannel> = (BAND_START_KHZ..=BAND_END_KHZ)
        .step_by(RASTER_KHZ as usize)
        .map(|khz| measure_channel(spectrum, khz))
        .collect();
    let eligible: Vec<bool> = stats
        .iter()
        .copied()
        .map(DetectedChannel::is_eligible)
        .collect();
    let peaks = best_per_blob(&stats, &eligible);
    merge_spaced(peaks, MIN_SEP_KHZ, MAX_CHANNELS)
}

fn best_per_blob(stats: &[DetectedChannel], eligible: &[bool]) -> Vec<DetectedChannel> {
    let mut peaks = Vec::new();
    let mut i = 0;
    while i < stats.len() {
        if !eligible[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i < stats.len() && eligible[i] {
            i += 1;
        }
        for (lo, hi) in split_valleys(stats, start, i) {
            if let Some(best) = argmax_quality(stats, lo, hi) {
                peaks.push(stats[best]);
            }
        }
    }
    peaks
}

fn split_valleys(stats: &[DetectedChannel], start: usize, end: usize) -> Vec<(usize, usize)> {
    if end <= start + 2 {
        return vec![(start, end)];
    }

    let mut maxima = Vec::new();
    for i in start..end {
        let left = if i == start {
            f32::NEG_INFINITY
        } else {
            stats[i - 1].snr_db
        };
        let right = if i + 1 == end {
            f32::NEG_INFINITY
        } else {
            stats[i + 1].snr_db
        };
        if stats[i].snr_db >= left && stats[i].snr_db >= right {
            maxima.push(i);
        }
    }
    if maxima.len() <= 1 {
        return vec![(start, end)];
    }

    let mut segments = Vec::new();
    let mut seg_start = start;
    for pair in maxima.windows(2) {
        let left_peak = pair[0];
        let right_peak = pair[1];
        let Some((valley, valley_snr)) = (left_peak..=right_peak)
            .map(|i| (i, stats[i].snr_db))
            .min_by(|a, b| cmp_f32(a.1, b.1))
        else {
            continue;
        };
        let weaker = stats[left_peak].snr_db.min(stats[right_peak].snr_db);
        if weaker - valley_snr >= VALLEY_DB && valley > seg_start && valley + 1 < end {
            segments.push((seg_start, valley));
            seg_start = valley + 1;
        }
    }
    if seg_start < end {
        segments.push((seg_start, end));
    }
    if segments.is_empty() {
        vec![(start, end)]
    } else {
        segments
    }
}

fn argmax_quality(stats: &[DetectedChannel], start: usize, end: usize) -> Option<usize> {
    (start..end).max_by(|&a, &b| stats[a].quality_cmp(stats[b]))
}

fn measure_channel(spectrum: &BandSpectrum, frequency_khz: u32) -> DetectedChannel {
    let center_hz = frequency_khz as f64 * 1_000.0;
    let Some(center_idx) = spectrum.index_of(center_hz) else {
        return DetectedChannel::rejected(frequency_khz);
    };
    if (spectrum.freq_hz(center_idx) - center_hz).abs() > spectrum.bin_hz {
        return DetectedChannel::rejected(frequency_khz);
    };

    let test_radius = bins_for(TEST_HALF_HZ);
    let guard = bins_for(GUARD_HZ);
    let train = bins_for(TRAIN_HZ);

    let lo = center_idx.saturating_sub(test_radius);
    let hi = (center_idx + test_radius + 1).min(spectrum.power_db.len());
    if hi <= lo {
        return DetectedChannel::rejected(frequency_khz);
    }

    let test = &spectrum.power_db[lo..hi];
    let noise = os_cfar_noise(&spectrum.power_db, center_idx, guard, train);

    let mut occupied = 0usize;
    let mut sum_lin = 0.0f32;
    let mut sum_excess = 0.0f32;
    let mut sum_excess2 = 0.0f32;
    for &p in test {
        let excess = p - noise;
        let lin = 10.0f32.powf(excess / 10.0);
        sum_lin += lin;
        if excess >= OCCUPANCY_BIN_DB {
            occupied += 1;
        }
        let excess_lin = (lin - 1.0).max(0.0);
        sum_excess += excess_lin;
        sum_excess2 += excess_lin * excess_lin;
    }

    let snr_db = 10.0 * (sum_lin / test.len() as f32).max(1e-12).log10();
    let occupancy = occupied as f32 / test.len() as f32;
    let bandwidth_hz = if sum_excess2 > 1e-12 {
        GRID_HZ as f32 * (sum_excess * sum_excess) / sum_excess2
    } else {
        0.0
    };

    DetectedChannel {
        frequency_khz,
        snr_db,
        occupancy,
        bandwidth_hz,
    }
}

fn bins_for(hz: f64) -> usize {
    (hz / GRID_HZ).round().max(1.0) as usize
}

fn os_cfar_noise(psd_db: &[f32], center: usize, guard: usize, train: usize) -> f32 {
    let n = psd_db.len();
    let mut cells = Vec::with_capacity(train.saturating_mul(2));

    if center > guard {
        let left_end = center - guard;
        let left_start = left_end.saturating_sub(train);
        cells.extend_from_slice(&psd_db[left_start..left_end]);
    }

    let right_start = (center + guard).min(n);
    let right_end = (right_start + train).min(n);
    if right_start < right_end {
        cells.extend_from_slice(&psd_db[right_start..right_end]);
    }

    if cells.len() < 8 {
        return percentile(psd_db, NOISE_PERCENTILE);
    }
    percentile(&cells, NOISE_PERCENTILE)
}

fn merge_spaced(
    mut peaks: Vec<DetectedChannel>,
    min_sep_khz: u32,
    max_peaks: usize,
) -> Vec<DetectedChannel> {
    peaks.sort_by(|a, b| b.quality_cmp(*a));

    let mut selected = Vec::with_capacity(peaks.len().min(max_peaks));
    for peak in peaks {
        if selected
            .iter()
            .any(|s: &DetectedChannel| s.frequency_khz.abs_diff(peak.frequency_khz) < min_sep_khz)
        {
            continue;
        }
        selected.push(peak);
        if selected.len() >= max_peaks {
            break;
        }
    }
    selected.sort_by_key(|s| s.frequency_khz);
    selected
}

#[cfg(test)]
mod tests {
    use super::super::spectrum::{BAND_END_HZ, BAND_START_HZ, GRID_HZ};
    use super::*;

    fn empty_floor(floor_db: f32) -> BandSpectrum {
        let n = ((BAND_END_HZ - BAND_START_HZ) / GRID_HZ).round() as usize + 1;
        BandSpectrum {
            start_hz: BAND_START_HZ,
            bin_hz: GRID_HZ,
            power_db: vec![floor_db; n],
        }
    }

    fn paint_haystack(spec: &mut BandSpectrum, center_hz: f64, peak_db: f32, floor_db: f32) {
        let half_top = 60_000.0;
        let half_bot = 100_000.0;
        let start = spec.start_hz;
        let step = spec.bin_hz;
        for (i, bin) in spec.power_db.iter_mut().enumerate() {
            let f = start + i as f64 * step;
            let d = (f - center_hz).abs();
            let excess = if d <= half_top {
                peak_db - floor_db
            } else if d >= half_bot {
                0.0
            } else {
                let t = ((half_bot - d) / (half_bot - half_top)) as f32;
                (peak_db - floor_db) * t
            };
            *bin = bin.max(floor_db + excess);
        }
    }

    fn paint_spur(spec: &mut BandSpectrum, center_hz: f64, peak_db: f32, width_hz: f64) {
        let start = spec.start_hz;
        let step = spec.bin_hz;
        for (i, bin) in spec.power_db.iter_mut().enumerate() {
            let f = start + i as f64 * step;
            if (f - center_hz).abs() <= width_hz * 0.5 {
                *bin = bin.max(peak_db);
            }
        }
    }

    fn freqs(hits: &[DetectedChannel]) -> Vec<u32> {
        hits.iter().map(|h| h.frequency_khz).collect()
    }

    fn channel(
        frequency_khz: u32,
        snr_db: f32,
        occupancy: f32,
        bandwidth_hz: f32,
    ) -> DetectedChannel {
        DetectedChannel {
            frequency_khz,
            snr_db,
            occupancy,
            bandwidth_hz,
        }
    }

    #[test]
    fn noise_only_yields_no_channels() {
        let spec = empty_floor(-80.0);
        assert!(detect_fm_channels(&spec).is_empty());
    }

    #[test]
    fn two_haystacks_snap_to_itu_raster() {
        let mut spec = empty_floor(-80.0);
        paint_haystack(&mut spec, 88_000_000.0, -55.0, -80.0);
        paint_haystack(&mut spec, 105_200_000.0, -52.0, -80.0);
        assert_eq!(freqs(&detect_fm_channels(&spec)), vec![88_000, 105_200]);
    }

    #[test]
    fn narrow_spur_is_rejected() {
        let mut spec = empty_floor(-80.0);
        paint_spur(&mut spec, 92_000_000.0, -40.0, 10_000.0);
        assert!(detect_fm_channels(&spec).is_empty());
    }

    #[test]
    fn spur_next_to_a_station_does_not_shift_the_channel() {
        let mut spec = empty_floor(-80.0);
        paint_haystack(&mut spec, 101_500_000.0, -50.0, -80.0);
        paint_spur(&mut spec, 101_350_000.0, -35.0, 8_000.0);
        assert_eq!(freqs(&detect_fm_channels(&spec)), vec![101_500]);
    }

    #[test]
    fn one_haystack_is_one_channel() {
        let mut spec = empty_floor(-80.0);
        paint_haystack(&mut spec, 105_000_000.0, -58.0, -80.0);
        assert_eq!(freqs(&detect_fm_channels(&spec)), vec![105_000]);
    }

    #[test]
    fn weak_haystack_still_detected() {
        let mut spec = empty_floor(-80.0);
        paint_haystack(&mut spec, 96_000_000.0, -70.0, -80.0);
        assert_eq!(freqs(&detect_fm_channels(&spec)), vec![96_000]);
    }

    #[test]
    fn stations_400_khz_apart_are_both_kept() {
        let mut spec = empty_floor(-80.0);
        paint_haystack(&mut spec, 102_000_000.0, -50.0, -80.0);
        paint_haystack(&mut spec, 102_400_000.0, -52.0, -80.0);
        assert_eq!(freqs(&detect_fm_channels(&spec)), vec![102_000, 102_400]);
    }

    #[test]
    fn weaker_200_khz_sidelobe_is_dropped() {
        let mut spec = empty_floor(-80.0);
        paint_haystack(&mut spec, 105_000_000.0, -48.0, -80.0);
        paint_haystack(&mut spec, 105_200_000.0, -62.0, -80.0);
        assert_eq!(freqs(&detect_fm_channels(&spec)), vec![105_000]);
    }

    #[test]
    fn plateau_with_small_dip_is_one_station() {
        let mut spec = empty_floor(-80.0);
        paint_haystack(&mut spec, 104_900_000.0, -50.0, -80.0);
        paint_haystack(&mut spec, 105_000_000.0, -51.0, -80.0);
        paint_haystack(&mut spec, 105_100_000.0, -50.5, -80.0);
        let hits = detect_fm_channels(&spec);
        assert_eq!(hits.len(), 1, "got {hits:?}");
        assert!((104_800..=105_200).contains(&hits[0].frequency_khz));
    }

    #[test]
    fn off_grid_centre_snaps_to_nearest_raster() {
        let mut spec = empty_floor(-80.0);
        paint_haystack(&mut spec, 89_330_000.0, -54.0, -80.0);
        assert_eq!(freqs(&detect_fm_channels(&spec)), vec![89_300]);
    }

    #[test]
    fn light_modulation_haystack_is_still_fm() {
        let mut spec = empty_floor(-80.0);
        let center = 97_500_000.0;
        let half_top = 25_000.0;
        let half_bot = 70_000.0;
        let start = spec.start_hz;
        let step = spec.bin_hz;
        for (i, bin) in spec.power_db.iter_mut().enumerate() {
            let f = start + i as f64 * step;
            let d = (f - center).abs();
            let excess = if d <= half_top {
                12.0
            } else if d >= half_bot {
                0.0
            } else {
                let t = ((half_bot - d) / (half_bot - half_top)) as f32;
                12.0 * t
            };
            *bin = -80.0 + excess;
        }
        assert_eq!(freqs(&detect_fm_channels(&spec)), vec![97_500]);
    }

    #[test]
    fn city_cluster_resolves_each_haystack() {
        let mut spec = empty_floor(-80.0);
        let mhz = [
            88.0, 89.3, 90.0, 90.4, 102.0, 102.4, 103.0, 103.5, 104.5, 105.2, 105.7, 107.0, 107.4,
            107.9,
        ];
        for (i, m) in mhz.iter().enumerate() {
            paint_haystack(&mut spec, m * 1_000_000.0, -48.0 - (i as f32) * 0.4, -80.0);
        }
        let got = freqs(&detect_fm_channels(&spec));
        let want: Vec<u32> = mhz.iter().map(|m| (*m * 1000.0).round() as u32).collect();
        assert_eq!(got, want);
    }

    #[test]
    fn merge_drops_100_khz_duplicate() {
        let peaks = vec![
            channel(105_000, 12.0, 0.7, 160_000.0),
            channel(105_100, 9.0, 0.5, 140_000.0),
        ];
        let merged = merge_spaced(peaks, MIN_SEP_KHZ, MAX_CHANNELS);
        assert_eq!(freqs(&merged), vec![105_000]);
    }

    #[test]
    fn merge_keeps_stronger_of_200_khz_pair() {
        let peaks = vec![
            channel(88_000, 18.0, 0.8, 170_000.0),
            channel(88_200, 9.0, 0.4, 90_000.0),
        ];
        let merged = merge_spaced(peaks, MIN_SEP_KHZ, MAX_CHANNELS);
        assert_eq!(freqs(&merged), vec![88_000]);
    }
}
