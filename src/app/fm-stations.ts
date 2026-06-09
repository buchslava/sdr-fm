export interface FmStation {
  name: string;
  frequencyKhz: number;
  region: string;
}

/** Well-known FM broadcast presets — strong carriers and common SDR demo targets. */
export const FM_STATIONS: FmStation[] = [
  { name: "BBC Radio 2", frequencyKhz: 88_000, region: "London, UK" },
  { name: "NPR / public radio", frequencyKhz: 89_100, region: "US (common slot)" },
  { name: "Classic FM", frequencyKhz: 90_000, region: "UK national" },
  { name: "KJZZ / classical", frequencyKhz: 91_500, region: "US Southwest" },
  { name: "WNYC", frequencyKhz: 93_900, region: "New York" },
  { name: "KLOS", frequencyKhz: 95_500, region: "Los Angeles" },
  { name: "KROQ", frequencyKhz: 97_100, region: "Los Angeles" },
  { name: "WBZ-FM", frequencyKhz: 98_500, region: "Boston" },
  { name: "WHTZ (Z100)", frequencyKhz: 100_300, region: "New York" },
  { name: "WXXL", frequencyKhz: 101_500, region: "New York" },
  { name: "WNEW-FM", frequencyKhz: 102_700, region: "New York" },
  { name: "WAXQ", frequencyKhz: 104_300, region: "New York" },
  { name: "WLTW (Lite FM)", frequencyKhz: 106_700, region: "New York" },
  { name: "Band edge", frequencyKhz: 107_900, region: "108 MHz limit" },
];
