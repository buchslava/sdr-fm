import {
  AfterViewInit,
  Component,
  computed,
  DestroyRef,
  ElementRef,
  inject,
  input,
  output,
  signal,
  viewChild,
} from "@angular/core";

import { FmStation } from "../../models/fm-station";
import {
  buildDialNumerals,
  buildDialTicks,
  buildPresetMarkers,
  buildScanMarkers,
  khzToPercent,
  LABEL_LANE_COUNT,
  neighborStation,
  percentToNearestStation,
} from "../../utils/fm-dial";

@Component({
  selector: "app-rigonda-dial",
  templateUrl: "./rigonda-dial.component.html",
  styleUrl: "./rigonda-dial.component.css",
})
export class RigondaDialComponent implements AfterViewInit {
  private readonly destroyRef = inject(DestroyRef);

  readonly stations = input<FmStation[]>([]);
  readonly selectedId = input<string | null>(null);
  readonly scanHits = input<FmStation[]>([]);
  readonly disabled = input(false);

  readonly stationSelected = output<string>();
  readonly noPresetNearby = output<void>();

  readonly trackRef = viewChild<ElementRef<HTMLElement>>("track");

  readonly ticks = buildDialTicks();
  readonly numerals = buildDialNumerals();
  readonly laneCount = LABEL_LANE_COUNT;
  readonly dragging = signal(false);
  readonly trackWidthPx = signal(0);

  readonly presetMarkers = computed(() =>
    buildPresetMarkers(this.stations(), this.trackWidthPx()),
  );

  readonly scanMarkers = computed(() =>
    buildScanMarkers(this.scanHits(), this.stations()),
  );

  readonly pointerPercent = computed(() => {
    const selected = this.stations().find((s) => s.id === this.selectedId());
    if (!selected) {
      return 50;
    }
    return khzToPercent(selected.frequencyKhz);
  });

  private resizeObserver: ResizeObserver | null = null;

  ngAfterViewInit(): void {
    const track = this.trackRef()?.nativeElement;
    if (!track) {
      return;
    }

    this.trackWidthPx.set(track.clientWidth);

    this.resizeObserver = new ResizeObserver((entries) => {
      const width = entries[0]?.contentRect.width ?? 0;
      if (width > 0) {
        this.trackWidthPx.set(width);
      }
    });
    this.resizeObserver.observe(track);

    this.destroyRef.onDestroy(() => {
      this.resizeObserver?.disconnect();
      this.resizeObserver = null;
    });
  }

  onTrackClick(event: MouseEvent): void {
    if (this.disabled() || this.dragging()) {
      return;
    }
    this.selectAtClientX(event.clientX);
  }

  onTrackKeydown(event: KeyboardEvent): void {
    if (this.disabled()) {
      return;
    }

    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") {
      return;
    }

    event.preventDefault();
    const direction = event.key === "ArrowLeft" ? -1 : 1;
    const next = neighborStation(
      this.stations(),
      this.selectedId(),
      direction,
    );
    if (next) {
      this.stationSelected.emit(next.id);
    }
  }

  onPointerDown(event: PointerEvent): void {
    if (this.disabled()) {
      return;
    }

    event.preventDefault();
    event.stopPropagation();
    this.dragging.set(true);

    const target = event.currentTarget as HTMLElement;
    target.setPointerCapture(event.pointerId);
  }

  onPointerMove(event: PointerEvent): void {
    if (!this.dragging() || this.disabled()) {
      return;
    }

    event.preventDefault();
    this.selectAtClientX(event.clientX, false);
  }

  onPointerUp(event: PointerEvent): void {
    if (!this.dragging()) {
      return;
    }

    event.preventDefault();
    this.dragging.set(false);
    this.selectAtClientX(event.clientX);

    const target = event.currentTarget as HTMLElement;
    if (target.hasPointerCapture(event.pointerId)) {
      target.releasePointerCapture(event.pointerId);
    }
  }

  leaderOffset(marker: { xPercent: number; labelXPercent: number }): number {
    return marker.xPercent - marker.labelXPercent;
  }

  selectStationById(id: string, event: Event): void {
    if (this.disabled()) {
      return;
    }

    event.stopPropagation();
    this.stationSelected.emit(id);
  }

  private selectAtClientX(clientX: number, emitMiss = true): void {
    const track = this.trackRef()?.nativeElement;
    if (!track) {
      return;
    }

    const rect = track.getBoundingClientRect();
    const percent = ((clientX - rect.left) / rect.width) * 100;
    const station = percentToNearestStation(percent, this.stations());

    if (station) {
      this.stationSelected.emit(station.id);
      return;
    }

    if (emitMiss) {
      this.noPresetNearby.emit();
    }
  }

  formatNumeral(mhz: number): string {
    return Number.isInteger(mhz) ? String(mhz) : mhz.toFixed(1);
  }
}
