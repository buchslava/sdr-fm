import { FmStation } from "../models/fm-station";
import { SNAP_RADIUS_KHZ } from "./fm-dial";

/**
 * When replacing the station list (e.g. after scan), copy names from the
 * previous list onto incoming entries at similar frequencies.
 */
export function mergePreservedLabels(
  incoming: FmStation[],
  previous: FmStation[],
  maxDistanceKhz = SNAP_RADIUS_KHZ,
): FmStation[] {
  const namedPrevious = previous.filter(
    (station) => station.name.trim().length > 0,
  );
  const usedPreviousIds = new Set<string>();

  return incoming.map((station) => {
    if (station.name.trim().length > 0) {
      return station;
    }

    let best: FmStation | null = null;
    let bestDistance = Infinity;

    for (const old of namedPrevious) {
      if (usedPreviousIds.has(old.id)) {
        continue;
      }

      const distance = Math.abs(old.frequencyKhz - station.frequencyKhz);
      if (distance <= maxDistanceKhz && distance < bestDistance) {
        bestDistance = distance;
        best = old;
      }
    }

    if (best === null) {
      return station;
    }

    usedPreviousIds.add(best.id);
    return { ...station, name: best.name.trim() };
  });
}
