# Radio theory for beginners — and how SDR FM uses it

This note is a first-principles tour of broadcast radio, written so you can follow it without a degree, then look up the academic names when you want them. Everything here is the physics that [SDR FM](../README.md) actually implements when you press **Start**.

For how the app *finds* stations by itself, see **[FM auto-detection](FM_AUTO_DETECTION.md)** — that document assumes this one.

---

## 1. A radio wave is just a very fast wiggle

Imagine a cork bobbing on a pond. The number of bobs per second is the **frequency** \(f\), measured in **hertz** (Hz). Radio is the same idea, only the “pond” is the electromagnetic field, and the bobs are billions of times faster.

| Name | Meaning | Everyday scale |
|------|---------|----------------|
| **Hz** | cycles per second | a piano A is 440 Hz |
| **kHz** | thousand Hz | AM talk radio |
| **MHz** | million Hz | FM broadcast, `101.5 MHz` |
| **GHz** | billion Hz | Wi-Fi, satellite |

**Wavelength** \(\lambda\) is the distance between two crests:

\[
\lambda = \frac{c}{f}
\]

where \(c \approx 3 \times 10^8\) m/s (speed of light). An FM station at 100 MHz has \(\lambda \approx 3\) metres — about the length of a car. That is why an FM whip antenna is a fraction of a metre: it is a piece of metal cut to “feel” that wavelength.

In this app you never type Hz. You pick **megahertz** on the dial (`101.5`), and the Rust backend converts to hertz for the tuner (`101_500_000`).

---

## 2. What “tuning” means

The air is full of stations at once, like a room of people talking over each other. A **tuner** does two jobs:

1. **Select** one frequency (the **carrier**).
2. **Translate** that slice of spectrum down to something a computer can sample.

A kitchen radio does this with analog filters and a local oscillator (the classic **superheterodyne** receiver). SDR FM does the same job in two layers:

| Layer | What it is | In this project |
|-------|------------|-----------------|
| **RF front-end** | Tiny TV tuner chip on the USB dongle | RTL-SDR (R820T2-class tuner + RTL2832 ADC) |
| **DSP** | Math on a stream of numbers | FutureSDR flowgraph in `src-tauri/src/dsp/flowgraph.rs` |

You still “tune to 101.5 MHz.” The difference is that after the dongle, the signal is no longer a voltage on a speaker coil — it is a stream of **complex samples**.

---

## 3. IQ samples: the secret handshake of SDR

A real radio wave at the antenna is one real voltage versus time. Inside an SDR it is represented as two numbers per instant:

- **I** — in-phase (the “cosine” part)
- **Q** — quadrature (the “sine” part, 90° shifted)

Together they are a **complex baseband** sample \( I + jQ \). Think of a clock hand: **I** is east–west, **Q** is north–south. The *length* of the hand is amplitude. The *angle* of the hand is **phase**.

That representation is not a gimmick. Any narrow slice of RF can be shifted to “almost DC” and described by a slowly moving clock hand. Frequency modulation is then literally: **the hand speeds up and slows down**.

Nyquist’s theorem says: to reconstruct a signal you must sample at least twice as fast as its highest frequency. The RTL-SDR streams on the order of **1.024 million complex samples per second** by default (`SDR_FM_SAMPLE_RATE`). That is the **IQ sample rate**. It is *not* the 48 kHz your speakers use. Two different clocks, two different jobs.

---

## 4. Amplitude vs frequency: AM and FM

**Modulation** means “hide audio inside a radio wave.”

### AM — Amplitude Modulation

The carrier stays at a fixed frequency. Loudness of the music changes the **height** of the wave. Static (lightning, fridge motors) also changes height, so AM sounds noisy. Academic name: the message sits in the **envelope**.

### FM — Frequency Modulation

The carrier’s *height* stays roughly constant. Loudness of the music changes **how far the frequency wanders** around the station’s centre. That wander is the **frequency deviation** \(\Delta f\).

For broadcast FM in most of the world (ITU-R BS.412):

\[
\Delta f_{\text{peak}} = \pm 75\,\text{kHz}
\]

A quiet whisper barely nudges the carrier. A loud snare drum shoves it almost 75 kHz off-centre, then it springs back. Static still wiggles amplitude; the FM detector mostly ignores amplitude and watches **phase/frequency**. That is why FM won the music band.

SDR FM is a **wideband FM (WBFM)** receiver: it expects that ±75 kHz swing, not the tiny deviation of amateur NBFM.

---

## 5. Carson’s rule, or “how fat is an FM station?”

A naive person thinks “the station is a needle at 101.5 MHz.” Open SDR++ and you see a **haystack** — a smear of energy maybe 150–200 kHz wide.

**Carson’s bandwidth rule** (a first-order occupancy estimate) says:

\[
B \approx 2(\Delta f + f_m)
\]

- \(\Delta f = 75\) kHz (peak deviation)
- \(f_m \approx 15\) kHz (highest audio for mono)

so

\[
B \approx 2(75 + 15) = 180\,\text{kHz}
\]

Stereo is a bit hungrier (the multiplex goes up to ~53 kHz, RDS sits at 57 kHz), but on a coarse spectrum plot it is still one haystack, not two needles.

**Bessel functions** describe the exact sideband recipe. You do not need them to use the app. You only need this slogan:

> Broadcast FM is *wide*. A spike a few kilohertz wide is a spur, a birdie, or the dongle’s own DC blob — not a radio station.

Playback uses that width when it **channelizes**: after tuning, a FIR decimator drops the IQ rate toward **~256 kHz**, which is enough to hold one Carson haystack and throw away the neighbours.

---

## 6. Stereo, the 19 kHz pilot, and RDS

The transmitter does not send “left speaker” and “right speaker” as two RF carriers. It builds a **multiplex (MPX)** baseband and frequency-modulates *that*:

| MPX region | Content |
|------------|---------|
| 30 Hz – 15 kHz | L+R (everyone hears this, even mono radios) |
| **19 kHz** | **Pilot tone** — “I am stereo” |
| 23–53 kHz | L−R, amplitude-modulated onto 38 kHz (twice the pilot) |
| **57 kHz** | **RDS** (Radio Data System) — station name, and more |

A kitchen radio locks to the 19 kHz pilot with a PLL and recovers L and R. SDR FM’s *listening* path is currently **mono**: it demodulates FM to MPX, then plays the L+R part after de-emphasis. Stereo decode exists in the scan path only insofar as the RDS stack needs the pilot to recover the 57 kHz subcarrier.

**RDS Program Service (PS)** is the eight-character name (`HIT FM  `). Scan dwells on strong candidates and fills names when the decoder gets a clean PS. Many stations send weak or no RDS. Empty name ≠ “not a station.”

---

## 7. Pre-emphasis and de-emphasis (the 75 µs trick)

FM noise, after demodulation, is not white: it rises with frequency (the discriminator turns phase jitter into a triangular noise spectrum). Transmitters **pre-emphasize** treble (boost highs) so that at the receiver you can **de-emphasize** (cut highs) and take the noise down with them. Music is restored; hiss is not.

The analog time constant in much of Europe (and in this app) is

\[
\tau = 75\,\mu\text{s}
\]

Implemented as a one-pole IIR at 48 kHz in `flowgraph.rs`:

\[
\alpha = e^{-1/(f_s \tau)}, \quad y[n] = (1-\alpha)\,x[n] + \alpha\, y[n-1]
\]

If this filter were omitted, FM would sound bright and hissy. If the wrong \(\tau\) were used (the US often uses 75 µs as well; some regions use 50 µs), the tonal balance would be slightly off — not silent, just “wrong EQ.”

---

## 8. From antenna to speaker in this app

Here is the listening pipeline, in the order the samples actually move.

```
Antenna
  → RTL-SDR tuner + ADC          (RF → IQ at ~1.024 Msps)
  → SoapySDR / FutureSDR source  (USB into the process)
  → FIR decimator                (keep ~256 kHz around the station)
  → Phase discriminator          (FM demod: angle of I+jQ)
  → Resampler                    (down to 48 kHz)
  → De-emphasis                  (75 µs)
  → Gain
  → AudioSink (cpal)             (your speakers)
```

### Phase discriminator (the FM detector)

If \(s[n]\) is the current IQ sample and \(s[n-1]\) the previous one, the instantaneous phase change is

\[
\Delta\phi = \arg\big(s[n]\, s[n-1]^*\big)
\]

That \(\Delta\phi\) *is* the audio (scaled so that 75 kHz deviation maps to a sensible amplitude). This is the discrete **polar discriminator**, cousin of the analog Foster–Seeley and quadrature detectors. No magic — the clock hand’s speed *is* the music.

### Live retune

While **Start** is active, clicking another preset does not rebuild the graph. `SdrPlayer` posts `DspCommand::TuneFrequency` to the Seify source. The LO jumps; the rest of the chain keeps running. That is why switching stations feels instant after the first start.

### What the UI is doing

The Angular dial is not a radio. It is a **front panel**: presets in `~/.sdr-fm/stations.json`, a selected frequency in kHz, Tauri `invoke("start_fm")` / `stop_fm`. All RF lives in Rust.

---

## 9. Sample rates you will see

| Rate | Role |
|------|------|
| **1.024 MHz** (default IQ) | Dongle streaming. Override with `SDR_FM_SAMPLE_RATE`. RTL-SDR **rejects** 300–900 kHz (e.g. 768 kHz). |
| **~256 kHz** | Channel after decimation — one WBFM haystack |
| **48 kHz** | Sound card / `AudioSink` |

Two valid RTL2832 bands exist: ~225–300 kHz and ~0.9–3.2 MHz. The app snaps illegal rates to a neighbour and logs the fact.

Gain is fixed at **40 dB** with AGC off during scan (and the playback source uses the same 40 dB). AGC would breathe the noise floor and confuse both ears and detectors.

---

## 10. Noise, SNR, and why the antenna still matters

**Thermal noise** is always there. **SNR** (signal-to-noise ratio) is “how tall is the haystack compared to the grass.”

SDR does not repeal physics:

- A wet noodle antenna → haystacks sink into the grass.
- A dongle next to a laptop charger → spurs (thin spikes) everywhere.
- The RTL-SDR has a **DC spike** at the tuned frequency (ADC offset) and analog filters that droop near ±Nyquist. Those are hardware facts, not software bugs.

When listening, you judge SNR with your ears. When scanning, the app judges it with a **radiometer** on the spectrum — same quantity, different sense organ. That story is the next document.

---

## 11. Glossary

| Term | Plain language |
|------|----------------|
| **Carrier** | The station’s centre frequency |
| **Baseband** | The audio/MPX after the carrier has been removed |
| **IQ** | Two-rail complex sample of baseband |
| **LO** | Local oscillator — the frequency the tuner “mixes against” |
| **Deviation** \(\Delta f\) | How far FM is allowed to wander |
| **Occupied bandwidth** | How much RF spectrum the signal actually paints |
| **WBFM** | Broadcast-style FM (±75 kHz), not handheld-radio FM |
| **Raster** | Legal channel grid (in Region 1: 100 kHz from 87.5 MHz) |
| **PSD** | Power spectral density — “brightness vs frequency” |
| **CFAR** | Constant false alarm rate — a detector that tracks the local noise |

---

## 12. Where to read in the code

| Topic | File |
|-------|------|
| Open dongle, sample-rate policy | `src-tauri/src/dsp/mod.rs` |
| Listen pipeline | `src-tauri/src/dsp/flowgraph.rs` |
| Start / stop / retune | `src-tauri/src/sdr.rs` |
| Presets | `src-tauri/src/config/stations.rs` |
| UI dial and Scan button | `src/app/` |

You now have enough theory to understand why 101.5 MHz is a *place*, why the waterfall shows a *blob*, and why the app decimates before it demodulates. The sequel — [FM auto-detection](FM_AUTO_DETECTION.md) — is how we teach the computer to click those blobs the way you would in SDR++.
