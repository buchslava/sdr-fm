import { Component } from "@angular/core";
import { invoke } from "@tauri-apps/api/core";

import { FM_STATIONS } from "./fm-stations";

@Component({
  selector: "app-root",
  imports: [],
  templateUrl: "./app.component.html",
  styleUrl: "./app.component.css",
})
export class AppComponent {
  readonly stations = FM_STATIONS;

  frequencyKhz = 101_500;
  status = "Ready.";
  isPlaying = false;
  error = "";

  get statusLine(): string {
    return this.error || this.status;
  }

  formatMhz(khz: number): string {
    return (khz / 1000).toFixed(1);
  }

  selectStation(khz: number): void {
    if (this.isPlaying) {
      return;
    }
    this.frequencyKhz = khz;
  }

  async listen(): Promise<void> {
    this.error = "";
    const frequency = Math.round(this.frequencyKhz);

    if (!Number.isFinite(frequency) || frequency <= 0) {
      this.error = "Invalid frequency.";
      return;
    }

    try {
      this.status = "Tuning...";
      const message = await invoke<string>("start_fm", {
        frequencyKhz: frequency,
      });
      this.isPlaying = true;
      this.status = message;
    } catch (err) {
      this.isPlaying = false;
      this.error = String(err);
      this.status = "Stopped.";
    }
  }

  async stop(): Promise<void> {
    this.error = "";

    try {
      await invoke("stop_fm");
      this.isPlaying = false;
      this.status = "Stopped.";
    } catch (err) {
      this.error = String(err);
    }
  }
}
