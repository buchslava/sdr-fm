import { FmStation } from "./models/fm-station";

export const FM_MIN_KHZ = 64_000;
export const FM_MAX_KHZ = 1_080_000;

export interface StationFieldErrors {
  frequency?: string;
  name?: string;
}

export function parseFrequencyMhz(
  input: string,
): { frequencyKhz: number } | { error: string } {
  const trimmed = input.trim();
  if (!trimmed) {
    return { error: "Frequency is required." };
  }

  const mhz = Number(trimmed);
  if (!Number.isFinite(mhz)) {
    return { error: "Enter a valid number." };
  }

  const frequencyKhz = Math.round(mhz * 1000);
  if (frequencyKhz <= 0) {
    return { error: "Frequency is required." };
  }

  return { frequencyKhz };
}

export function validateStation(
  frequencyKhz: number,
  allStations: FmStation[],
  editingId?: string,
): StationFieldErrors {
  const errors: StationFieldErrors = {};

  if (!Number.isFinite(frequencyKhz) || frequencyKhz <= 0) {
    errors.frequency = "Frequency is required.";
    return errors;
  }

  if (frequencyKhz < FM_MIN_KHZ || frequencyKhz > FM_MAX_KHZ) {
    errors.frequency = `Frequency must be ${FM_MIN_KHZ / 1000}–${FM_MAX_KHZ / 1000} MHz.`;
  } else if (
    allStations.some(
      (station) =>
        station.frequencyKhz === frequencyKhz && station.id !== editingId,
    )
  ) {
    errors.frequency = "This frequency is already in the list.";
  }

  return errors;
}

export function hasFieldErrors(errors: StationFieldErrors): boolean {
  return Boolean(errors.frequency || errors.name);
}
