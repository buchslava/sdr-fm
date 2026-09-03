import { Injectable, computed, signal } from "@angular/core";
import { invoke } from "@tauri-apps/api/core";

import { FmStation } from "../models/fm-station";
import { correctStationFrequency } from "../utils/fine-tune";
import { mergePreservedLabels } from "../utils/station-label-merge";

interface StationsFile {
  stations: FmStation[];
  selectedStationId?: string | null;
}

function sortByFrequency(stations: FmStation[]): FmStation[] {
  return [...stations].sort((a, b) => a.frequencyKhz - b.frequencyKhz);
}

@Injectable({ providedIn: "root" })
export class StationStoreService {
  readonly stations = signal<FmStation[]>([]);
  readonly selectedId = signal<string | null>(null);

  readonly selectedStation = computed(
    () => this.stations().find((s) => s.id === this.selectedId()) ?? null,
  );

  readonly selectedFrequencyKhz = computed(
    () => this.selectedStation()?.frequencyKhz ?? null,
  );

  async load(): Promise<void> {
    const file = await invoke<StationsFile>("get_stations");
    this.stations.set(sortByFrequency(file.stations ?? []));
    this.selectedId.set(file.selectedStationId ?? null);
    this.ensureSelection();
    if (this.selectedId() !== (file.selectedStationId ?? null)) {
      await this.persist();
    }
  }

  select(id: string): void {
    if (this.selectedId() === id) {
      return;
    }

    this.selectedId.set(id);
    void this.persist();
  }

  async replaceStations(stations: FmStation[]): Promise<void> {
    const merged = mergePreservedLabels(stations, this.stations());
    const next = sortByFrequency(merged);
    this.stations.set(next);
    this.selectedId.set(null);
    this.ensureSelectionFromScan();
    await this.persist();
  }

  async add(station: FmStation): Promise<void> {
    const previous = this.stations();
    const previousId = this.selectedId();
    const next = sortByFrequency([...previous, station]);

    try {
      this.stations.set(next);
      this.selectedId.set(station.id);
      await this.persist();
    } catch (error) {
      this.stations.set(previous);
      this.selectedId.set(previousId);
      throw error;
    }
  }

  async nudgeSelected(deltaKhz: number): Promise<FmStation | null> {
    const station = this.selectedStation();
    if (!station) {
      return null;
    }

    const next = correctStationFrequency(station, deltaKhz, this.stations());
    if (!next) {
      return null;
    }

    await this.update(next);
    return next;
  }

  async update(station: FmStation): Promise<void> {
    const previous = this.stations();
    const previousId = this.selectedId();
    const next = sortByFrequency(
      previous.map((item) => (item.id === station.id ? station : item)),
    );

    try {
      this.stations.set(next);
      this.selectedId.set(station.id);
      await this.persist();
    } catch (error) {
      this.stations.set(previous);
      this.selectedId.set(previousId);
      throw error;
    }
  }

  async remove(id: string): Promise<void> {
    const previous = sortByFrequency(this.stations());
    const previousId = this.selectedId();
    const index = previous.findIndex((station) => station.id === id);
    if (index < 0) {
      return;
    }

    const next = sortByFrequency(previous.filter((station) => station.id !== id));

    try {
      this.stations.set(next);
      this.selectAfterRemoval(next, index);
      await this.persist();
    } catch (error) {
      this.stations.set(previous);
      this.selectedId.set(previousId);
      throw error;
    }
  }

  private persistQueue: Promise<void> = Promise.resolve();

  private persist(): Promise<void> {
    const stations = this.stations();
    const selectedStationId = this.selectedId();
    const write = this.persistQueue.then(async () => {
      await invoke("set_stations", { stations, selectedStationId });
    });
    this.persistQueue = write.catch(() => {
      /* keep the queue alive after a failed write */
    });
    return write;
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
