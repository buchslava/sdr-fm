import {
  Component,
  computed,
  inject,
  OnDestroy,
  OnInit,
  signal,
} from "@angular/core";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";

import { RigondaDialComponent } from "./components/rigonda-dial/rigonda-dial.component";
import {
  StationFormModalComponent,
  StationFormMode,
} from "./components/station-form-modal/station-form-modal.component";
import { FmStation, formatMhz } from "./models/fm-station";
import { StationStoreService } from "./services/station-store.service";

interface ScanProgressEvent {
  phase: string;
  current: number;
  total: number;
  mhz: number;
}

@Component({
  selector: "app-root",
  imports: [RigondaDialComponent, StationFormModalComponent],
  templateUrl: "./app.component.html",
  styleUrl: "./app.component.css",
})
export class AppComponent implements OnInit, OnDestroy {
  readonly store = inject(StationStoreService);

  readonly isPlaying = signal(false);
  readonly isScanning = signal(false);
  readonly status = signal("Ready.");
  readonly error = signal("");
  readonly modalOpen = signal(false);
  readonly modalMode = signal<StationFormMode>("add");
  readonly modalStation = signal<FmStation | null>(null);
  readonly scanHits = signal<FmStation[]>([]);

  readonly statusLine = computed(() => this.error() || this.status());
  readonly crudDisabled = computed(() => this.isPlaying() || this.isScanning());

  readonly formatMhz = formatMhz;

  private unlistenScan: UnlistenFn | null = null;

  async ngOnInit(): Promise<void> {
    try {
      this.unlistenScan = await listen<ScanProgressEvent>(
        "scan-progress",
        (event) => {
          const p = event.payload;
          const phase = p.phase === "rds" ? "RDS" : "Power";
          this.status.set(
            `Scanning ${phase} ${p.current}/${p.total} @ ${p.mhz.toFixed(1)} MHz…`,
          );
        },
      );
      await this.store.load();
      this.status.set("Ready.");
    } catch (err) {
      this.error.set(String(err));
    }
  }

  ngOnDestroy(): void {
    if (this.unlistenScan) {
      this.unlistenScan();
      this.unlistenScan = null;
    }
  }

  showReadoutName(station: FmStation): boolean {
    const name = station.name.trim();
    return name.length > 0 && name !== formatMhz(station.frequencyKhz);
  }

  onNoPresetNearby(): void {
    if (this.error()) {
      return;
    }
    this.status.set("No preset near that position.");
  }

  async selectStation(id: string): Promise<void> {
    this.store.select(id);

    if (!this.isPlaying()) {
      return;
    }

    const frequency = this.store.selectedFrequencyKhz();
    if (frequency === null) {
      return;
    }

    this.error.set("");
    try {
      this.status.set("Tuning...");
      const message = await invoke<string>("start_fm", {
        frequencyKhz: frequency,
      });
      this.status.set(message);
    } catch (err) {
      this.error.set(String(err));
    }
  }

  openAdd(): void {
    if (this.isPlaying() || this.isScanning()) {
      return;
    }

    this.modalMode.set("add");
    this.modalStation.set(null);
    this.modalOpen.set(true);
  }

  openEditSelected(): void {
    const station = this.store.selectedStation();
    if (!station || this.isPlaying() || this.isScanning()) {
      return;
    }

    this.openEdit(station);
  }

  openEdit(station: FmStation): void {
    if (this.isPlaying() || this.isScanning()) {
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
      this.scanHits.set([]);
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
      this.scanHits.set([]);
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

  async scanBand(): Promise<void> {
    if (this.isPlaying() || this.isScanning()) {
      return;
    }

    this.error.set("");
    this.isScanning.set(true);
    this.scanHits.set([]);
    this.status.set("Scanning FM band…");

    try {
      const found = await invoke<FmStation[]>("scan_fm_band");
      this.scanHits.set(found);
      const named = found.filter((s) => s.name.trim().length > 0).length;
      const confirmed = await ask(
        `Replace station list with ${found.length} scanned stations` +
          (named > 0 ? ` (${named} with RDS names)` : "") +
          "?",
        {
          title: "Scan complete",
          kind: "warning",
        },
      );

      if (!confirmed) {
        this.scanHits.set([]);
        this.status.set("Scan discarded.");
        return;
      }

      await this.store.replaceStations(found);
      this.scanHits.set([]);
      this.status.set(
        `Loaded ${found.length} scanned stations` +
          (named > 0 ? ` (${named} named).` : "."),
      );
    } catch (err) {
      this.scanHits.set([]);
      this.error.set(String(err));
      this.status.set("Scan failed.");
    } finally {
      this.isScanning.set(false);
    }
  }
}
