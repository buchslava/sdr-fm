import {
  Component,
  computed,
  inject,
  OnInit,
  signal,
} from "@angular/core";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";

import {
  StationFormModalComponent,
  StationFormMode,
} from "./components/station-form-modal/station-form-modal.component";
import { FmStation, formatMhz } from "./models/fm-station";
import { StationStoreService } from "./services/station-store.service";

@Component({
  selector: "app-root",
  imports: [StationFormModalComponent],
  templateUrl: "./app.component.html",
  styleUrl: "./app.component.css",
})
export class AppComponent implements OnInit {
  readonly store = inject(StationStoreService);

  readonly isPlaying = signal(false);
  readonly status = signal("Ready.");
  readonly error = signal("");
  readonly modalOpen = signal(false);
  readonly modalMode = signal<StationFormMode>("add");
  readonly modalStation = signal<FmStation | null>(null);

  readonly statusLine = computed(() => this.error() || this.status());
  readonly crudDisabled = computed(() => this.isPlaying());

  readonly formatMhz = formatMhz;

  async ngOnInit(): Promise<void> {
    try {
      await this.store.load();
      this.status.set("Ready.");
    } catch (err) {
      this.error.set(String(err));
    }
  }

  selectStation(id: string): void {
    if (this.isPlaying()) {
      return;
    }
    this.store.select(id);
  }

  openAdd(): void {
    if (this.isPlaying()) {
      return;
    }

    this.modalMode.set("add");
    this.modalStation.set(null);
    this.modalOpen.set(true);
  }

  openEdit(station: FmStation): void {
    if (this.isPlaying()) {
      return;
    }

    this.modalMode.set("edit");
    this.modalStation.set(station);
    this.modalOpen.set(true);
  }

  closeModal(): void {
    this.modalOpen.set(false);
    this.modalStation.set(null);
  }

  async onModalSave(station: FmStation): Promise<void> {
    this.error.set("");

    try {
      if (this.modalMode() === "add") {
        await this.store.add(station);
        this.status.set("Station added.");
      } else {
        await this.store.update(station);
        this.status.set("Station updated.");
      }
      this.closeModal();
    } catch (err) {
      this.error.set(String(err));
    }
  }

  async deleteSelected(): Promise<void> {
    if (this.isPlaying()) {
      return;
    }

    const station = this.store.selectedStation();
    if (!station) {
      return;
    }

    const label = station.name
      ? `${formatMhz(station.frequencyKhz)} MHz (${station.name})`
      : `${formatMhz(station.frequencyKhz)} MHz`;

    const confirmed = await ask(`Remove ${label}?`, {
      title: "Remove station",
      kind: "warning",
    });

    if (!confirmed) {
      return;
    }

    this.error.set("");

    try {
      await this.store.remove(station.id);
      this.status.set("Station removed.");
    } catch (err) {
      this.error.set(String(err));
    }
  }

  async listen(): Promise<void> {
    this.error.set("");
    const frequency = this.store.selectedFrequencyKhz();

    if (frequency === null || !Number.isFinite(frequency) || frequency <= 0) {
      this.error.set("Select a station.");
      return;
    }

    try {
      this.status.set("Tuning...");
      const message = await invoke<string>("start_fm", {
        frequencyKhz: frequency,
      });
      this.isPlaying.set(true);
      this.status.set(message);
    } catch (err) {
      this.isPlaying.set(false);
      this.error.set(String(err));
      this.status.set("Stopped.");
    }
  }

  async stop(): Promise<void> {
    this.error.set("");

    try {
      await invoke("stop_fm");
      this.isPlaying.set(false);
      this.status.set("Stopped.");
    } catch (err) {
      this.error.set(String(err));
    }
  }
}
