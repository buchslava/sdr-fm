import { DecimalPipe } from "@angular/common";
import { Component } from "@angular/core";
import { FormsModule } from "@angular/forms";
import { invoke } from "@tauri-apps/api/core";

@Component({
  selector: "app-root",
  imports: [FormsModule, DecimalPipe],
  templateUrl: "./app.component.html",
  styleUrl: "./app.component.css",
})
export class AppComponent {
  frequencyKhz = 101500;
  status = "Enter an FM frequency in kHz and press Listen.";
  isPlaying = false;
  error = "";

  async listen(): Promise<void> {
    this.error = "";
    const frequency = Math.round(this.frequencyKhz);

    if (!Number.isFinite(frequency) || frequency <= 0) {
      this.error = "Enter a valid frequency in kHz.";
      return;
    }

    try {
      this.status = "Tuning RTL-SDR...";
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
