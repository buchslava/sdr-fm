import { FmStation, formatMhz } from "../models/fm-station";

export const DIAL_MIN_MHZ = 87.5;
export const DIAL_MAX_MHZ = 108.0;
export const DIAL_MIN_KHZ = DIAL_MIN_MHZ * 1000;
export const DIAL_MAX_KHZ = DIAL_MAX_MHZ * 1000;
export const SNAP_RADIUS_KHZ = 250;

export function stationLabel(station: FmStation): string {
  return station.name.trim() || formatMhz(station.frequencyKhz);
}

export function khzToMhz(khz: number): number {
  return khz / 1000;
}

export function mhzToPercent(mhz: number): number {
  const clamped = Math.max(DIAL_MIN_MHZ, Math.min(DIAL_MAX_MHZ, mhz));
  return (
    ((clamped - DIAL_MIN_MHZ) / (DIAL_MAX_MHZ - DIAL_MIN_MHZ)) * 100
  );
}

export function khzToPercent(khz: number): number {
  return mhzToPercent(khzToMhz(khz));
}

export function percentToMhz(percent: number): number {
  const clamped = Math.max(0, Math.min(100, percent));
  return DIAL_MIN_MHZ + (clamped / 100) * (DIAL_MAX_MHZ - DIAL_MIN_MHZ);
}

export function percentToKhz(percent: number): number {
  return Math.round(percentToMhz(percent) * 1000);
}

export function nearestStation(
  frequencyKhz: number,
  stations: FmStation[],
  maxDistanceKhz = SNAP_RADIUS_KHZ,
): FmStation | null {
  if (stations.length === 0) {
    return null;
  }

  let best: FmStation | null = null;
  let bestDistance = Infinity;

  for (const station of stations) {
    const distance = Math.abs(station.frequencyKhz - frequencyKhz);
    if (distance < bestDistance) {
      bestDistance = distance;
      best = station;
    } else if (distance === bestDistance && best !== null) {
      if (station.id.localeCompare(best.id) < 0) {
        best = station;
      }
    }
  }

  if (best === null || bestDistance > maxDistanceKhz) {
    return null;
  }

  return best;
}

/** Pick the preset whose dial position is closest to the clicked point. */
export function nearestStationAtPercent(
  percent: number,
  stations: FmStation[],
  maxDistanceKhz = SNAP_RADIUS_KHZ,
): FmStation | null {
  const targetKhz = percentToKhz(percent);
  const candidates = stations.filter(
    (station) => Math.abs(station.frequencyKhz - targetKhz) <= maxDistanceKhz,
  );

  if (candidates.length === 0) {
    return null;
  }

  if (candidates.length === 1) {
    return candidates[0];
  }

  return candidates.reduce((best, station) => {
    const dialDist = Math.abs(khzToPercent(station.frequencyKhz) - percent);
    const bestDialDist = Math.abs(khzToPercent(best.frequencyKhz) - percent);
    if (dialDist < bestDialDist) {
      return station;
    }
    if (dialDist > bestDialDist) {
      return best;
    }
    if (station.frequencyKhz !== best.frequencyKhz) {
      return Math.abs(station.frequencyKhz - targetKhz) <
        Math.abs(best.frequencyKhz - targetKhz)
        ? station
        : best;
    }
    return station.id.localeCompare(best.id) < 0 ? station : best;
  });
}

export function percentToNearestStation(
  percent: number,
  stations: FmStation[],
  maxDistanceKhz = SNAP_RADIUS_KHZ,
): FmStation | null {
  return nearestStationAtPercent(percent, stations, maxDistanceKhz);
}

export interface DialTick {
  mhz: number;
  major: boolean;
  xPercent: number;
}

export function buildDialTicks(): DialTick[] {
  const ticks: DialTick[] = [];
  for (let mhz = DIAL_MIN_MHZ; mhz <= DIAL_MAX_MHZ + 0.001; mhz += 0.5) {
    const rounded = Math.round(mhz * 10) / 10;
    ticks.push({
      mhz: rounded,
      major: Math.abs(rounded - Math.round(rounded)) < 0.001,
      xPercent: mhzToPercent(rounded),
    });
  }
  return ticks;
}

export interface DialNumeral {
  mhz: number;
  xPercent: number;
}

/** Integer MHz labels every 2 MHz; skip 88 — too close to 87.5 at the left edge. */
export function buildDialNumerals(): DialNumeral[] {
  const numerals: DialNumeral[] = [
    { mhz: DIAL_MIN_MHZ, xPercent: mhzToPercent(DIAL_MIN_MHZ) },
  ];

  for (let mhz = 90; mhz < DIAL_MAX_MHZ; mhz += 2) {
    numerals.push({ mhz, xPercent: mhzToPercent(mhz) });
  }

  numerals.push({ mhz: DIAL_MAX_MHZ, xPercent: mhzToPercent(DIAL_MAX_MHZ) });
  return numerals;
}

export const LABEL_LANE_COUNT = 5;
export const DEFAULT_TRACK_WIDTH_PX = 720;

const LABEL_CHAR_WIDTH_PX = 5.4;
const LABEL_PADDING_PX = 10;
const LABEL_MIN_GAP_PX = 6;

/** Widest half hit area for a tick, in dial percent (~7 px on a 720 px track). */
const MAX_TICK_HIT_HALF_PERCENT = 1;

export interface PresetMarker {
  station: FmStation;
  label: string;
  /** Frequency position on the scale (tick). */
  xPercent: number;
  /** Horizontal center of the label (may shift to avoid edge clip). */
  labelXPercent: number;
  labelTier: number;
  /** False when no lane has room — tick + tooltip only. */
  showLabel: boolean;
  /** Half width of the tick hit area; never reaches a neighbouring tick. */
  hitHalfPercent: number;
  /** Horizontal distance from the label centre back to its own tick. */
  leaderShiftPx: number;
}

export function clampLabelPercent(xPercent: number): number {
  return Math.max(3, Math.min(97, xPercent));
}

function estimateLabelWidthPx(label: string): number {
  return LABEL_PADDING_PX + label.length * LABEL_CHAR_WIDTH_PX;
}

function halfWidthPercent(label: string, trackWidthPx: number): number {
  const width = trackWidthPx > 0 ? trackWidthPx : DEFAULT_TRACK_WIDTH_PX;
  return (estimateLabelWidthPx(label) / 2 / width) * 100;
}

function gapPercent(trackWidthPx: number): number {
  const width = trackWidthPx > 0 ? trackWidthPx : DEFAULT_TRACK_WIDTH_PX;
  return (LABEL_MIN_GAP_PX / width) * 100;
}

function intervalsOverlap(
  aLeft: number,
  aRight: number,
  bLeft: number,
  bRight: number,
  gap: number,
): boolean {
  return !(aRight + gap < bLeft || aLeft > bRight + gap);
}

function fitLabelInterval(
  center: number,
  half: number,
): { center: number; left: number; right: number } {
  let c = center;
  let left = c - half;
  let right = c + half;

  if (left < 1) {
    c = half + 1;
    left = c - half;
    right = c + half;
  } else if (right > 99) {
    c = 99 - half;
    left = c - half;
    right = c + half;
  }

  return { center: c, left, right };
}

interface LaneSlot {
  left: number;
  right: number;
}

export function buildPresetMarkers(
  stations: FmStation[],
  trackWidthPx = DEFAULT_TRACK_WIDTH_PX,
): PresetMarker[] {
  const sorted = [...stations].sort(
    (a, b) => a.frequencyKhz - b.frequencyKhz || a.id.localeCompare(b.id),
  );

  if (sorted.length === 0) {
    return [];
  }

  const gap = gapPercent(trackWidthPx);
  const lanes: LaneSlot[][] = Array.from(
    { length: LABEL_LANE_COUNT },
    () => [],
  );

  const layout = new Map<
    string,
    { labelTier: number; labelXPercent: number; showLabel: boolean }
  >();

  for (const station of sorted) {
    const label = stationLabel(station);
    const tickX = khzToPercent(station.frequencyKhz);
    const half = halfWidthPercent(label, trackWidthPx);
    const fitted = fitLabelInterval(clampLabelPercent(tickX), half);

    let assignedLane = -1;

    for (let lane = 0; lane < LABEL_LANE_COUNT; lane++) {
      const blocked = lanes[lane].some((slot) =>
        intervalsOverlap(
          fitted.left,
          fitted.right,
          slot.left,
          slot.right,
          gap,
        ),
      );
      if (!blocked) {
        assignedLane = lane;
        lanes[lane].push({ left: fitted.left, right: fitted.right });
        break;
      }
    }

    if (assignedLane < 0) {
      layout.set(station.id, {
        labelTier: 0,
        labelXPercent: fitted.center,
        showLabel: false,
      });
      continue;
    }

    layout.set(station.id, {
      labelTier: assignedLane,
      labelXPercent: fitted.center,
      showLabel: true,
    });
  }

  const tickXs = sorted.map((station) => khzToPercent(station.frequencyKhz));
  const width = trackWidthPx > 0 ? trackWidthPx : DEFAULT_TRACK_WIDTH_PX;

  return sorted.map((station, index) => {
    const label = stationLabel(station);
    const tickX = tickXs[index];
    const placed = layout.get(station.id);
    const labelX = placed?.labelXPercent ?? clampLabelPercent(tickX);

    return {
      station,
      label,
      xPercent: tickX,
      labelXPercent: labelX,
      labelTier: placed?.labelTier ?? 0,
      showLabel: placed?.showLabel ?? true,
      hitHalfPercent: tickHitHalfPercent(tickXs, index),
      leaderShiftPx: ((tickX - labelX) / 100) * width,
    };
  });
}

/**
 * Hit areas may not overlap, otherwise a click lands on whichever neighbour
 * happens to paint last instead of the tick under the cursor.
 */
function tickHitHalfPercent(tickXs: number[], index: number): number {
  const toPrev =
    index > 0 ? (tickXs[index] - tickXs[index - 1]) / 2 : Infinity;
  const toNext =
    index < tickXs.length - 1
      ? (tickXs[index + 1] - tickXs[index]) / 2
      : Infinity;

  return Math.min(toPrev, toNext, MAX_TICK_HIT_HALF_PERCENT);
}

export interface ScanMarker {
  station: FmStation;
  xPercent: number;
  overlapsPreset: boolean;
}

export function buildScanMarkers(
  scanHits: FmStation[],
  presets: FmStation[],
): ScanMarker[] {
  const presetFreqs = new Set(presets.map((s) => s.frequencyKhz));
  return [...scanHits]
    .sort((a, b) => a.frequencyKhz - b.frequencyKhz)
    .map((station) => ({
      station,
      xPercent: khzToPercent(station.frequencyKhz),
      overlapsPreset: presetFreqs.has(station.frequencyKhz),
    }));
}

export function neighborStation(
  stations: FmStation[],
  currentId: string | null,
  direction: -1 | 1,
): FmStation | null {
  if (stations.length === 0) {
    return null;
  }

  const sorted = [...stations].sort(
    (a, b) => a.frequencyKhz - b.frequencyKhz,
  );

  if (!currentId) {
    return direction === 1 ? sorted[0] : sorted[sorted.length - 1];
  }

  const index = sorted.findIndex((s) => s.id === currentId);
  if (index < 0) {
    return sorted[0];
  }

  const nextIndex = index + direction;
  if (nextIndex < 0 || nextIndex >= sorted.length) {
    return sorted[index];
  }

  return sorted[nextIndex];
}
