import {
  afterNextRender,
  Component,
  effect,
  ElementRef,
  input,
  output,
  signal,
  viewChild,
} from "@angular/core";
import { FormsModule } from "@angular/forms";

import {
  FmStation,
  formatMhz,
  newStationId,
} from "../../models/fm-station";
import {
  hasFieldErrors,
  parseFrequencyMhz,
  validateStation,
} from "../../station-validator";

export type StationFormMode = "add" | "edit";

@Component({
  selector: "app-station-form-modal",
  imports: [FormsModule],
  templateUrl: "./station-form-modal.component.html",
  styleUrl: "./station-form-modal.component.css",
})
export class StationFormModalComponent {
  readonly mode = input.required<StationFormMode>();
  readonly station = input<FmStation | null>(null);
  readonly stations = input.required<FmStation[]>();

  readonly saved = output<FmStation>();
  readonly closed = output<void>();

  readonly frequencyMhz = signal("");
  readonly name = signal("");
  readonly fieldErrors = signal<{ frequency?: string; name?: string }>({});

  readonly title = signal("Add station");

  private readonly frequencyInput =
    viewChild.required<ElementRef<HTMLInputElement>>("frequencyInput");

  constructor() {
    afterNextRender(() => {
      const input = this.frequencyInput().nativeElement;
      input.spellcheck = false;
      input.focus({ preventScroll: true });
      const length = input.value.length;
      try {
        input.setSelectionRange(length, length);
      } catch {
        // number inputs may not support selection ranges
      }
    });

    effect(() => {
      const mode = this.mode();
      const station = this.station();

      this.title.set(mode === "add" ? "Add station" : "Edit station");
      this.frequencyMhz.set(station ? formatMhz(station.frequencyKhz) : "");
      this.name.set(station?.name ?? "");
      this.fieldErrors.set({});
    });
  }

  onFrequencyInput(value: string | number | null): void {
    this.frequencyMhz.set(value === null || value === undefined ? "" : String(value));
  }

  onNameInput(value: string): void {
    this.name.set(value);
  }

  cancel(): void {
    this.closed.emit();
  }

  submit(): void {
    const parsed = parseFrequencyMhz(this.frequencyMhz());
    if ("error" in parsed) {
      this.fieldErrors.set({ frequency: parsed.error });
      return;
    }

    const editingId =
      this.mode() === "edit" ? this.station()?.id : undefined;
    const errors = validateStation(
      parsed.frequencyKhz,
      this.stations(),
      editingId,
    );

    if (hasFieldErrors(errors)) {
      this.fieldErrors.set(errors);
      return;
    }

    const current = this.station();
    const saved: FmStation = {
      id: current?.id ?? newStationId(),
      name: this.name().trim(),
      frequencyKhz: parsed.frequencyKhz,
    };

    this.saved.emit(saved);
  }
}
