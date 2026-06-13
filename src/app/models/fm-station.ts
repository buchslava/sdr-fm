export interface FmStation {
  id: string;
  name: string;
  frequencyKhz: number;
}

export function formatMhz(khz: number): string {
  return (khz / 1000).toFixed(1);
}

export function newStationId(): string {
  return crypto.randomUUID();
}
