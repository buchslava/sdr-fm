import { Injectable, computed, signal } from "@angular/core";
import { invoke } from "@tauri-apps/api/core";

import { FmStation } from "../models/fm-station";
import { mergePreservedLabels } from "../utils/station-label-merge";

export interface CityInfo {
  id: string;
  name: string;
}

function sortByFrequency(stations: FmStation[]): FmStation[] {
  return [...stations].sort((a, b) => a.frequencyKhz - b.frequencyKhz);
}

@Injectable({ providedIn: "root" })
export class StationStoreService {
  readonly stations = signal<FmStation[]>([]);
  readonly selectedId = signal<string | null>(null);
  readonly cities = signal<CityInfo[]>([]);
  readonly cityId = signal("kharkiv");

  readonly selectedStation = computed(
    () => this.stations().find((s) => s.id === this.selectedId()) ?? null,
  );

  readonly selectedFrequencyKhz = computed(
    () => this.selectedStation()?.frequencyKhz ?? null,
  );

  async load(): Promise<void> {
    // Load stations first so missing settings trigger IP detect + persist,
    // then read the resolved city.
    const stations = await invoke<FmStation[]>("get_stations");
    const [cities, city] = await Promise.all([
      invoke<CityInfo[]>("get_cities"),
      invoke<string>("get_city"),
    ]);
    this.stations.set(sortByFrequency(stations));
    this.cities.set(cities);
    this.cityId.set(city);
    this.ensureSelection();
  }

  select(id: string): void {
    this.selectedId.set(id);
  }

  async setCity(cityId: string): Promise<void> {
    const stations = await invoke<FmStation[]>("set_city", { city: cityId });
    this.stations.set(sortByFrequency(stations));
    this.cityId.set(cityId);
    this.selectedId.set(null);
    this.ensureSelection();
  }

  async replaceStations(stations: FmStation[]): Promise<void> {
    const merged = mergePreservedLabels(stations, this.stations());
    const next = sortByFrequency(merged);
    await invoke("set_stations", { stations: next });
    this.stations.set(next);
    this.selectedId.set(null);
    this.ensureSelectionFromScan();
  }

  async add(station: FmStation): Promise<void> {
    const previous = this.stations();
    const next = sortByFrequency([...previous, station]);

    try {
      await invoke("set_stations", { stations: next });
      this.stations.set(next);
      this.selectedId.set(station.id);
    } catch (error) {
      this.stations.set(previous);
      throw error;
    }
  }

  async update(station: FmStation): Promise<void> {
    const previous = this.stations();
    const next = sortByFrequency(
      previous.map((item) => (item.id === station.id ? station : item)),
    );

    try {
      await invoke("set_stations", { stations: next });
      this.stations.set(next);
      this.selectedId.set(station.id);
    } catch (error) {
      this.stations.set(previous);
      throw error;
    }
  }

  async remove(id: string): Promise<void> {
    const previous = sortByFrequency(this.stations());
    const index = previous.findIndex((station) => station.id === id);
    if (index < 0) {
      return;
    }

    const next = sortByFrequency(previous.filter((station) => station.id !== id));

    try {
      await invoke("set_stations", { stations: next });
      this.stations.set(next);
      this.selectAfterRemoval(next, index);
    } catch (error) {
      this.stations.set(previous);
      throw error;
    }
  }

  private ensureSelection(): void {
    const stations = this.stations();
    if (stations.length === 0) {
      this.selectedId.set(null);
      return;
    }

    const current = this.selectedId();
    if (current && stations.some((station) => station.id === current)) {
      return;
    }

    const preferred =
      stations.find((station) => station.name === "Хіт FM") ?? stations[0];
    this.selectedId.set(preferred.id);
  }

  private ensureSelectionFromScan(): void {
    const stations = this.stations();
    if (stations.length === 0) {
      this.selectedId.set(null);
      return;
    }

    const preferred =
      stations.find((station) => station.name.trim().length > 0) ?? stations[0];
    this.selectedId.set(preferred.id);
  }

  private selectAfterRemoval(stations: FmStation[], removedIndex: number): void {
    if (stations.length === 0) {
      this.selectedId.set(null);
      return;
    }

    const nextIndex = Math.min(removedIndex, stations.length - 1);
    this.selectedId.set(stations[nextIndex].id);
  }
}
