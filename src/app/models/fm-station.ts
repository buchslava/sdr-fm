export interface FmStation {
  id: string;
  name: string;
  frequencyKhz: number;
}

/** Raster (100 kHz) stays one decimal; off-raster fine-tunes show kilohertz. */
export function formatMhz(khz: number): string {
  const mhz = khz / 1000;
  if (khz % 100 === 0) {
    return mhz.toFixed(1);
  }
  return mhz.toFixed(3);
}

export function newStationId(): string {
  return crypto.randomUUID();
}
