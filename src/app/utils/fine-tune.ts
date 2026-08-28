import { FmStation, formatMhz } from "../models/fm-station";
import { hasFieldErrors, validateStation } from "../station-validator";

/** One click of the selected-station fine-tune control. */
export const FINE_TUNE_STEP_KHZ = 5;

export function nameIsFrequencyLabel(station: FmStation): boolean {
  return station.name.trim() === formatMhz(station.frequencyKhz);
}

export function applyFrequencyCorrection(
  station: FmStation,
  frequencyKhz: number,
): FmStation {
  const name = nameIsFrequencyLabel(station)
    ? formatMhz(frequencyKhz)
    : station.name;
  return { ...station, frequencyKhz, name };
}

export function correctStationFrequency(
  station: FmStation,
  deltaKhz: number,
  allStations: FmStation[],
): FmStation | null {
  const frequencyKhz = station.frequencyKhz + deltaKhz;
  if (hasFieldErrors(validateStation(frequencyKhz, allStations, station.id))) {
    return null;
  }
  return applyFrequencyCorrection(station, frequencyKhz);
}
