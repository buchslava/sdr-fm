# FM auto-detection — how the computer clicks the haystack

This is the companion to **[Radio theory for beginners](RADIO_THEORY.md)**. If “IQ,” “Carson’s 180 kHz,” “raster,” and “DC spike” are still foggy, read that first. Here we only answer one question:

> You can find stations in SDR++ by *looking*. How does **Scan** do the same job without a pair of eyes?

SDR++ has no auto-scan like this app. You are the detector: you see a haystack, you click the middle. Auto-detection is that gesture, written as a **hypothesis test** on a **power spectral density**.

---

## 1. The picture you already trust

On a waterfall, a living FM station is not a needle. It is a **haystack**: a ridge of energy about 150–200 kHz wide, sitting on a noisy floor. Spurs are sewing needles. The dongle’s LO leakage is a pimple at the centre of whatever you tuned.

Your brain does four things in a second:

1. Estimate the grass (noise floor).
2. Notice blobs that are *both loud and wide*.
3. Click the **middle** of each blob.
4. Ignore two clicks that are obviously the same blob.

Scan is those four steps, with names you can look up in a radar textbook.

---

## 2. Why the naive algorithm fails

An obvious idea: hop the tuner every 100 kHz, measure “how much power is in the IF,” keep the peaks.

That is a **radiometer** (energy detector) with the wrong camera:

| Problem | What you hear / see |
|---------|---------------------|
| Each hop **retunes** the LO | Settling transients, a moving DC spike, a slightly different noise floor |
| One number per hop | No shape — a 5 kHz birdie and a 180 kHz station look alike |
| 240 kHz IF, 100 kHz step | One station lights up three bins; they get promoted as three “stations” |
| “Kurtosis of the IQ” extras | Empirical seasoning, not a model of WBFM |
| “Keep everything within 18 dB of the loudest” | The weak station you *can* see in SDR++ is discarded |

So the old scan could *work* and still annoy you. The new scan never asks the tuner “how strong is this one channel?” It asks the same question SDR++ answers: **what does the spectrum look like?**

---

## 3. Stage A — take the spectrum (Welch, not a guess)

The RTL-SDR cannot swallow 87.5–108 MHz in one gulp (~20.5 MHz). At 2.048 million samples/s you only get about ±1 MHz of Nyquist, and the analog filters are honest only in the inner ~80 %. So the app **hops**.

### Each hop

1. Tune the LO to a centre that covers a slice of the FM band.
2. Dwell ~150 ms (a handful of hundred FFT windows).
3. Compute a **Hann-windowed Welch periodogram** — average of overlapping FFTs (size 4096 → 500 Hz bins).

Welch’s method is the academically boring, practically correct PSD estimator: window to kill leakage, overlap 50 %, average to kill variance. After ~100 averages the grass in the plot stops boiling.

### Stitching

Hops overlap on purpose. Around each LO there is an **80 kHz DC hole** (ADC offset + 1/f) and droopy edges. A frequency is accepted only when it is *away* from that hole. Neighbours fill the gaps. The result is one composite PSD on a **5 kHz grid** from 87.5 to 108 MHz — a still frame of what SDR++ would show if it could see the whole band at once.

Code: `src-tauri/src/dsp/spectrum.rs`  
Orchestration: `src-tauri/src/dsp/scan.rs` (progress phase `"spectrum"`).

AGC is **off**, gain **40 dB**. If the noise floor were breathing, every later threshold would lie.

---

## 4. Stage B — legal frequencies only (the ITU raster)

You do not click `101.537 MHz` in a car radio. In **ITU Region 1** (Europe, Ukraine, most of Africa — ITU-R BS.450) broadcast FM lives on

\[
f_n = 87.5\,\text{MHz} + n \times 100\,\text{kHz}
\]

So the detector does not hunt an arbitrary peak and then “snap.” It **tests each legal channel** as a hypothesis:

> \(H_0\): this 100 kHz slot is empty grass.  
> \(H_1\): a WBFM haystack is centred here.

That is a **composite hypothesis test** on a known grid. The grid is part of the physics-plus-regulation model, not a cosmetic round-off.

(Region 2, the Americas, uses a 200 kHz raster. This app is built for Region 1.)

---

## 5. Stage C — how loud is the grass? (OS-CFAR)

A global “noise = −80 dB” is a lie. The tuner’s gain ripples across 20 MHz; your antenna has VSWR holes; a strong neighbour heats the floor.

**CFAR** (Constant False Alarm Rate) is the radar name for “estimate noise *locally* so the false-alarm rate stays put.” This app uses an **order-statistic CFAR (OS-CFAR)**:

- Around channel \(f_c\), ignore a **guard** of ±120 kHz (the station under test plus Bessel skirts — do not count the target as noise).
- Collect **training cells** ~800 kHz left and right.
- Take the **25th percentile** of those cells.

Why percentile, not the mean? Mean is wrecked by any other station in the training window. The 25th percentile still sees grass when a couple of haystacks contaminate 10–20 % of the cells. That is the whole point of OS-CFAR versus cell-averaging CFAR.

Subtract that noise (in dB) from the PSD and you have a local **excess** — the plot your eye calls “above the grass.”

---

## 6. Stage D — the radiometer (Neyman–Pearson in one sentence)

If \(H_1\) is “unknown amplitude, known occupancy ~180 kHz, additive noise,” the classic **UMP** (uniformly most powerful) test for Gaussian frequency-domain noise is: **add up the linear power in the test cell**.

That integrator is a **radiometer**. Carson says the test cell is

\[
[f_c - 90\,\text{kHz},\; f_c + 90\,\text{kHz}]
\]

The app converts the mean linear excess to decibels and asks: is it at least **4.5 dB**? After Welch averaging, estimator variance is tiny; 4.5 dB is not a vibe, it is “many \(\sigma\) above \(H_0\).”

Linear mean (not dB-mean) is the theoretically honest energy detector. A loud needle can still fool it — which is why shape comes next.

---

## 7. Stage E — haystack vs needle (occupancy and \(B_{\mathrm{eq}}\))

Two extra statistics, both measuring **width**:

**Occupancy.** Fraction of 5 kHz bins in the test cell that sit ≥ 3 dB above CFAR noise. A haystack fills a third or more of the 180 kHz. A spur fills two bins and goes home.

**Equivalent rectangular bandwidth**

\[
B_{\mathrm{eq}} = \Delta f \cdot \frac{(\sum p_i)^2}{\sum p_i^2}
\]

where \(p_i\) is *excess* linear power and \(\Delta f = 5\) kHz. A rectangle of width \(W\) yields \(B_{\mathrm{eq}} = W\). A single bin yields \(B_{\mathrm{eq}} \approx 5\) kHz. The app requires \(B_{\mathrm{eq}} \gtrsim 55\) kHz — below Carson, above a birdie, kind to lightly modulated classical stations.

Together: **loud and wide**. That is the SDR++ click, in two numbers.

A channel that passes SNR + occupancy + \(B_{\mathrm{eq}}\) is **eligible**.

---

## 8. Stage F — one blob, one station

A 180 kHz haystack makes *several* neighbouring raster channels eligible (the 100 kHz neighbours still overlap the blob). If you promote every **local maximum**, a stitch dip or a Bessel shoulder becomes a second “station” 200 kHz away — same music, worse tune. You asked to keep the **best** one.

So eligible channels that touch on the raster are one **connected component** (one occupancy blob). Inside the blob, keep the channel with the highest radiometer SNR (occupancy as a tie-break). That is the centre you would have clicked.

Two real transmitters 400 kHz apart usually leave a **valley** of grass between them. If that valley is at least **6 dB** below the weaker peak, the blob is split — 102.0 and 102.4 both survive. A 1–2 dB wrinkle from hop stitching does not split. Hits still closer than **250 kHz** collapse to the stronger (one WBFM occupancy cannot be two independent locals).

Code: `src-tauri/src/dsp/detect.rs`.

---

## 9. Stage G — names, not detection (RDS)

Finding the frequency is a spectrum problem. The **name** is a coding problem.

On the strongest ~15 hits the app retunes with a **200 kHz LO offset** (so the station is not sitting on the DC pimple), channelizes, FM-demodulates, and runs the RDS stack (`fmradio`) for ~2.5 s. If Program Service comes through, you get a label. If not, you still have a valid MHz.

RDS is slow and optional. It must never be the reason a station is kept or dropped: plenty of real broadcasts are mute on PS.

The UI phase is `"rds"`. Confirming the dialog **replaces** the preset list (`~/.sdr-kitchen/stations.json`). Existing names at nearby frequencies can be copied onto unnamed hits (frontend merge).

---

## 10. The whole Scan, as a block diagram

```
Stop playback (release the dongle)
        │
        ▼
  Hop LO across 87.5–108 MHz @ 2.048 Msps
        │  150 ms IQ  →  Welch PSD  →  stitch, blank DC
        ▼
  Composite spectrum (5 kHz grid)
        │
        ▼
  For each 100 kHz ITU channel:
        OS-CFAR noise
        → radiometer in ±90 kHz
        → occupancy + Beq
        │
        ▼
  Cluster eligible blobs → one best raster per blob
        │
        ▼
  RDS dwell on the loudest few  →  Station { MHz, name? }
        │
        ▼
  Angular: show hits, ask to replace presets
```

Time budget: ~150 s ceiling. The spectrum pass is a few seconds of hops; RDS is what makes Scan feel long.

---

## 11. Mapping to code (who owns what)

| Responsibility | Module |
|----------------|--------|
| FFT, hops, stitch | `dsp/spectrum.rs` |
| CFAR, radiometer, blobs | `dsp/detect.rs` |
| USB, dwell, RDS, progress events | `dsp/scan.rs` |
| Tauri command `scan_fm_band` | `src-tauri/src/lib.rs` |
| Scan button / confirm replace | `src/app/app.component.ts` |

That split is deliberate: detection is a pure function of a PSD. You can (and the tests do) paint synthetic haystacks and spurs **without a dongle**. Hardware only supplies the PSD.

---

## 12. What can still go wrong (honest limits)

| Symptom | Likely physics |
|---------|----------------|
| Weak station missing | Antenna / SNR below ~4.5 dB mean excess in 180 kHz |
| Extra ghost near a strong one | Image, intermod, or a valley deeper than 6 dB — rare after clustering |
| Two cities on one frequency | Capture effect: you hear the stronger; scan reports one centre |
| Empty names | No RDS, or 2.5 s was not enough; frequency can still be right |
| Off by 100 kHz | Huge PPM error on a cheap crystal (unusual); or a lopsided haystack |

Scan cannot invent a better antenna. It can only click as well as the spectrum you fed it.

---

## 13. Pocket glossary for this document

| Term | In one line |
|------|-------------|
| **Welch PSD** | Averaged, windowed FFT — a stable brightness-vs-frequency curve |
| **Radiometer** | “Add the power in a known band and compare to noise” |
| **Neyman–Pearson** | Best detection power for a given false-alarm rate |
| **OS-CFAR** | Noise = a robust percentile of nearby cells, not the global min |
| **Occupancy / \(B_{\mathrm{eq}}\)** | Is the energy *spread* like FM, or *spiked* like a spur? |
| **ITU raster** | The only centre frequencies we are allowed to call “a channel” |
| **Connected blob** | Adjacent eligible channels = one haystack = one click |

---

## 14. If you remember only three sentences

1. FM stations are **haystacks ~180 kHz wide** on a **100 kHz legal grid** — not needles, not arbitrary MHz.
2. Scan **photographs the band** (Welch), then runs a **CFAR radiometer with a width test** — the same decision you make in SDR++.
3. **One blob, one best channel**; RDS is a nametag, not the detector.

That is the whole trick: borrow your eyes’ algorithm, give it the names it always had in detection theory, and stop hopping the tuner like a Geiger counter.
