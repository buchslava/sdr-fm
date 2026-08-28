# Radio theory for beginners — and how SDR FM uses it

*Ukrainian version: **[Теорія радіо для початківців](RADIO_THEORY.uk.md)***

This is a first-principles tour of broadcast radio, written so you can follow it without a degree — and then look up the academic names when you want them. Everything here is physics that [SDR FM](../README.md) actually performs when you press **Start**.

You do not need mathematics to enjoy this. Every formula below is followed by a sentence in plain language, and you are allowed to skip the formula and keep the sentence. There is a [glossary](#15-glossary) at the end, and a [myth-busting section](#12-eight-things-beginners-usually-get-wrong) for the things everybody gets wrong at first (including the author).

For how the app *finds* stations by itself, see **[FM auto-detection](FM_AUTO_DETECTION.md)** — that document assumes this one.

**Contents**

1. [A radio wave is just a very fast wiggle](#1-a-radio-wave-is-just-a-very-fast-wiggle)
2. [Decibels: the volume knob of the universe](#2-decibels-the-volume-knob-of-the-universe)
3. [What "tuning" means](#3-what-tuning-means)
4. [IQ samples: the secret handshake of SDR](#4-iq-samples-the-secret-handshake-of-sdr)
5. [Amplitude vs frequency: AM and FM](#5-amplitude-vs-frequency-am-and-fm)
6. [Carson's rule, or "how fat is an FM station?"](#6-carsons-rule-or-how-fat-is-an-fm-station)
7. [Stereo, the 19 kHz pilot, and RDS](#7-stereo-the-19-khz-pilot-and-rds)
8. [Pre-emphasis and de-emphasis (the 75 µs trick)](#8-pre-emphasis-and-de-emphasis-the-75-µs-trick)
9. [From antenna to speaker in this app](#9-from-antenna-to-speaker-in-this-app)
10. [Sample rates you will see](#10-sample-rates-you-will-see)
11. [Noise, SNR, and why the antenna still matters](#11-noise-snr-and-why-the-antenna-still-matters)
12. [Eight things beginners usually get wrong](#12-eight-things-beginners-usually-get-wrong)
13. [Fine tuning: when the "right" frequency is slightly wrong](#13-fine-tuning-when-the-right-frequency-is-slightly-wrong)
14. [Experiments to try tonight](#14-experiments-to-try-tonight)
15. [Glossary](#15-glossary)
16. [Where to read in the code](#16-where-to-read-in-the-code)
17. [Where to go next](#17-where-to-go-next)

---

## 1. A radio wave is just a very fast wiggle

Imagine a cork bobbing on a pond. The number of bobs per second is the **frequency** \(f\), measured in **hertz** (Hz). Radio is the same idea, only the "pond" is the electromagnetic field, and the bobs are billions of times faster.

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

where \(c \approx 3 \times 10^8\) m/s (speed of light). An FM station at 100 MHz has \(\lambda \approx 3\) metres — about the length of a car. That is why an FM whip antenna is a fraction of a metre: it is a piece of metal cut to "feel" that wavelength.

> **Rule of thumb.** Divide 300 by the frequency in MHz and you get the wavelength in metres. 100 MHz → 3 m. A good simple antenna is a **quarter** of that: ~75 cm of wire. This is why the wire that came with your dongle is roughly that long, and why a 10 cm stub hears almost nothing.

### Why FM lives at 87.5–108 MHz

Nobody rolled dice. This is **VHF Band II**, and it is a compromise between three inconvenient facts:

- **Low frequencies bend around hills** and travel far, but they are crowded and there is no room there for a 200 kHz-wide signal per station.
- **High frequencies carry lots of bandwidth**, but they behave more like light: they want line-of-sight and get eaten by walls and trees.
- **Antennas must be a practical size.** A quarter wave at 1 MHz is 75 metres. On a car roof, that is a problem.

Around 100 MHz you get all three at a tolerable level: a stable service radius of tens of kilometres, room for high-fidelity stereo, and an antenna you can hide in a windscreen. The band is defined internationally by the **ITU** (International Telecommunication Union), which divides the world into three regions. Europe, Africa, and most of Asia are **Region 1**; the Americas are Region 2. This app is built for Region 1.

### Why every station ends in .1, .3, .5…

In Region 1 the legal centres sit on a **100 kHz grid** starting at 87.5 MHz: 87.5, 87.6, 87.7 … 107.9, 108.0. That grid is called a **raster**. Region 2 uses a 200 kHz raster and only odd tenths (88.1, 88.3 …), which is why American frequencies look subtly different in films.

The raster exists so regulators can plan who transmits where without stations chewing each other's edges. It is also a gift to software: instead of hunting for arbitrary peaks, the scanner can simply ask *"is there a station on this legal channel?"* 205 times. That single idea is most of [FM auto-detection](FM_AUTO_DETECTION.md).

In this app you never type Hz. You pick **megahertz** on the dial (`101.5`), and the Rust backend converts to hertz for the tuner (`101_500_000`).

---

## 2. Decibels: the volume knob of the universe

Skip this section if `dB` already feels natural. If it does not, five minutes here will save you a lot of confusion later, because everything about radio strength is measured in decibels.

Radio signals span an absurd range. The transmitter across town may deliver a *trillion* times more power to your antenna than a distant one. Writing that with zeros is unbearable, so engineers compress it with a logarithm:

\[
P_{\text{dB}} = 10 \log_{10}\!\left(\frac{P}{P_{\text{ref}}}\right)
\]

In plain language: **decibels count zeros instead of writing them.** What you actually need to memorise is four numbers:

| Change in dB | What happened to the power | Feels like |
|--------------|---------------------------|------------|
| **+3 dB** | doubled | a small but real improvement |
| **+10 dB** | ten times | obvious, "the hiss dropped away" |
| **+20 dB** | a hundred times | a different station entirely |
| **−3 dB** | halved | the edge of noticing |

Two consequences that matter when you use SDR FM:

- **dB are relative.** "40 dB gain" means the amplifier multiplies power by 10 000. It does not say how loud anything is on its own.
- **dB add where multiplication would happen.** Doubling twice (×4) is +3 dB +3 dB = +6 dB. This is why every plot of radio power is drawn in dB: the maths turns into arithmetic your eye can do.

When the scanner says a station is **4.5 dB above the noise**, it means "the energy in this channel is about three times the surrounding grass." That is a modest but statistically solid difference — enough to be sure, not enough to sound perfect.

---

## 3. What "tuning" means

The air is full of stations at once, like a room of people talking over each other. A **tuner** does two jobs:

1. **Select** one frequency (the **carrier**).
2. **Translate** that slice of spectrum down to something a computer can sample.

The second job is the clever one, and it has a name: **heterodyning**. You mix the incoming signal with a locally generated wave (the **local oscillator**, or **LO**) and out comes the *difference* between them. Tune the LO to 101.4 MHz, feed it a station at 101.5 MHz, and the difference is 0.1 MHz — a frequency slow enough to filter and process comfortably.

> **Historical aside.** Edwin Armstrong patented this **superheterodyne** receiver in 1918, and virtually every radio built since — your car, your phone, your microwave's leakage detector — is a descendant. Armstrong later invented wideband FM too, then spent his last years in patent litigation over it. Radio history is unreasonably dramatic.

A kitchen radio does this with analog filters and a physical oscillator. SDR FM does the same job in two layers:

| Layer | What it is | In this project |
|-------|------------|-----------------|
| **RF front-end** | Tiny TV tuner chip on the USB dongle | RTL-SDR (R820T2-class tuner + RTL2832 ADC) |
| **DSP** | Math on a stream of numbers | FutureSDR flowgraph in `src-tauri/src/dsp/flowgraph.rs` |

The dongle you are using was designed to watch European digital TV. The RTL2832 chip could be told to dump its raw samples instead of decoding TV, and around 2012 somebody noticed. That accident created the entire cheap-SDR hobby. Your €20 receiver is a repurposed television tuner, which is why it has quirks (see [§11](#11-noise-snr-and-why-the-antenna-still-matters)) that a purpose-built radio would not.

You still "tune to 101.5 MHz." The difference is that after the dongle, the signal is no longer a voltage on a speaker coil — it is a stream of **complex samples**.

---

## 4. IQ samples: the secret handshake of SDR

A real radio wave at the antenna is one real voltage versus time. Inside an SDR it is represented as two numbers per instant:

- **I** — in-phase (the "cosine" part)
- **Q** — quadrature (the "sine" part, 90° shifted)

Together they are a **complex baseband** sample \( I + jQ \). Think of a clock hand: **I** is east–west, **Q** is north–south. The *length* of the hand is amplitude. The *angle* of the hand is **phase**.

### Why two numbers and not one?

Because one number cannot tell you which way the hand is turning.

Watch a single shadow of a spinning wheel on a wall: you see it move left and right, but you cannot tell whether the wheel spins clockwise or anticlockwise. Add a second shadow from 90° around, and the ambiguity disappears — the two shadows go out of step in a way that reveals the direction.

That is exactly the job of Q. After heterodyning, a station **above** your LO and a station **below** it both produce the same difference frequency. With I alone they would be pasted on top of each other. With I and Q the receiver can tell "above" from "below" — the mathematical term is **negative frequency**, and it is a real, useful thing rather than a paradox. It is also why the waterfall in SDR++ has a left half and a right half around the centre.

### How fast must we sample?

**Nyquist's theorem**: to reconstruct a signal you must sample at least twice as fast as its highest frequency. Sample too slowly and you get **aliasing** — the same effect that makes wagon wheels appear to spin backwards in old films. The camera samples 24 times a second; a wheel turning slightly slower than one spoke per frame looks like it is creeping in reverse. Undersampled radio does the same thing: a signal appears at a frequency where it simply is not.

The RTL-SDR streams on the order of **1.024 million complex samples per second** by default (`SDR_FM_SAMPLE_RATE`). Because the samples are complex, that buys you roughly 1 MHz of spectrum to look at — about ±0.5 MHz around wherever the LO sits. That is the **IQ sample rate**, and it is *not* the 48 kHz your speakers use. Two different clocks, two different jobs.

---

## 5. Amplitude vs frequency: AM and FM

**Modulation** means "hide audio inside a radio wave."

Picture a lighthouse. You can send a message by making the lamp **brighter and dimmer** (that is AM), or by keeping the brightness constant and making the beam **sweep faster and slower** (that is FM). Fog dims a lamp; it does not change how fast the beam sweeps. Hold that image — it explains almost everything below.

### AM — Amplitude Modulation

The carrier stays at a fixed frequency. Loudness of the music changes the **height** of the wave. Static (lightning, fridge motors, a passing tram) also changes height, so AM sounds noisy. Academic name: the message sits in the **envelope**.

AM is simple, cheap, and its receivers can be a diode and an earpiece with no battery at all. That simplicity is why it dominated for decades, and why it is still used for aviation and long-distance shortwave.

### FM — Frequency Modulation

The carrier's *height* stays roughly constant. Loudness of the music changes **how far the frequency wanders** around the station's centre. That wander is the **frequency deviation** \(\Delta f\).

For broadcast FM in most of the world (ITU-R BS.412):

\[
\Delta f_{\text{peak}} = \pm 75\,\text{kHz}
\]

A quiet whisper barely nudges the carrier. A loud snare drum shoves it almost 75 kHz off-centre, then it springs back. Static still wiggles amplitude; the FM detector mostly ignores amplitude and watches **phase/frequency**. That is why FM won the music band.

> **The capture effect.** FM has a personality trait AM lacks: when two stations share a frequency, the stronger one does not blend with the weaker — it **erases** it. Roughly 6 dB of advantage is enough to win completely. This is why driving between two cities gives you one station, then a burst of mush, then the other station, rather than a permanent duet. It is also why "two stations on 101.5" is not something you will usually *hear*, even when both exist.

SDR FM is a **wideband FM (WBFM)** receiver: it expects that ±75 kHz swing, not the tiny deviation of amateur NBFM handhelds.

---

## 6. Carson's rule, or "how fat is an FM station?"

A naive person thinks "the station is a needle at 101.5 MHz." Open SDR++ and you see a **haystack** — a smear of energy maybe 150–200 kHz wide.

Why? Because a frequency that keeps moving is, by definition, not a single frequency. The louder the music, the further the carrier roams, and the more spectrum it paints. A station playing silence is nearly a needle; the same station playing a drum solo is a plateau.

**Carson's bandwidth rule** (a first-order occupancy estimate) says:

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

> Broadcast FM is *wide*. A spike a few kilohertz wide is a spur, a birdie, or the dongle's own DC blob — not a radio station.

Notice the tension the raster creates: channels are spaced **100 kHz** apart, but each station is **180 kHz** wide. They overlap on paper. In practice regulators never assign adjacent channels to the same area — your local stations are spaced 300 kHz or more — so the overlap is only a problem for software that must decide how many stations a wide blob contains. That is exactly the clustering problem solved in the auto-detection document.

Playback uses that width when it **channelizes**: after tuning, a FIR decimator drops the IQ rate toward **~256 kHz**, which is enough to hold one Carson haystack and throw away the neighbours.

---

## 7. Stereo, the 19 kHz pilot, and RDS

Here is a puzzle the engineers of 1961 had to solve: FM was already a success in mono, and millions of receivers existed. Add stereo how? Any solution that made old radios go silent was commercially dead.

Their answer was clever enough to still be in use. The transmitter does not send "left speaker" and "right speaker" as two RF carriers. It builds a single **multiplex (MPX)** baseband signal — a stack of information at different audio frequencies — and frequency-modulates *that*:

| MPX region | Content |
|------------|---------|
| 30 Hz – 15 kHz | L+R (everyone hears this, even mono radios) |
| **19 kHz** | **Pilot tone** — "I am stereo" |
| 23–53 kHz | L−R, amplitude-modulated onto 38 kHz (twice the pilot) |
| **57 kHz** | **RDS** (Radio Data System) — station name, and more |

A mono radio simply plays the bottom layer and never notices the rest — it cannot hear above 15 kHz anyway. A stereo radio locks onto the 19 kHz **pilot tone**, doubles it to regenerate the 38 kHz carrier, recovers L−R, and then does primary-school arithmetic:

\[
L = \frac{(L{+}R) + (L{-}R)}{2}, \qquad R = \frac{(L{+}R) - (L{-}R)}{2}
\]

Backwards compatibility by addition and subtraction. Stereo costs you signal quality, though: that upper layer is quieter and noisier, which is why a fringe station hisses in stereo and cleans up the moment your radio gives up and drops to mono.

SDR FM's *listening* path is currently **mono**: it demodulates FM to MPX, then plays the L+R part after de-emphasis. Stereo decode exists in the scan path only insofar as the RDS stack needs the pilot to recover the 57 kHz subcarrier.

**RDS Program Service (PS)** is the eight-character name (`HIT FM  `) your car radio displays. It is genuine digital data — about 1 187.5 bits per second, roughly a thousandth of dial-up — riding on that 57 kHz subcarrier. At that speed, a station name takes a couple of seconds to arrive, which is why your car display sometimes fills in letter by letter after you tune.

Scan dwells on strong candidates and fills names when the decoder gets a clean PS. Many stations send weak or no RDS. Empty name ≠ "not a station."

---

## 8. Pre-emphasis and de-emphasis (the 75 µs trick)

FM noise, after demodulation, is not white: it rises with frequency (the discriminator turns phase jitter into a triangular noise spectrum). In plain terms, **FM hiss lives in the treble**.

So broadcasters cheat, in a way both ends agree on in advance. Transmitters **pre-emphasize** treble — deliberately boosting the highs before transmission — and your receiver **de-emphasizes** by exactly the same amount, cutting the highs back down. The music comes out level, because the boost and the cut cancel. The hiss, which was added *in between*, only gets the cut. Free noise reduction, paid for entirely by agreeing on a number.

If that sounds familiar, it is: it is the same principle as Dolby noise reduction on cassette tapes.

The analog time constant in much of Europe (and in this app) is

\[
\tau = 75\,\mu\text{s}
\]

Implemented as a one-pole IIR at 48 kHz in `flowgraph.rs`:

\[
\alpha = e^{-1/(f_s \tau)}, \quad y[n] = (1-\alpha)\,x[n] + \alpha\, y[n-1]
\]

In plain language, that formula is a lazy averager: each output sample is mostly the previous output with a splash of the new input. Slow changes (bass) pass through; fast changes (treble, hiss) get smoothed away.

If this filter were omitted, FM would sound bright and hissy. If the wrong \(\tau\) were used (Europe standardised 50 µs, North America 75 µs, and this app uses 75 µs), the tonal balance would be slightly off — not silent, just "wrong EQ."

---

## 9. From antenna to speaker in this app

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

Read it as a funnel. Each stage throws away information the next stage does not need — 1 MHz of spectrum becomes 256 kHz becomes one audio channel at 48 kHz — and throwing things away early is what keeps the CPU cool enough to run on a Raspberry Pi.

### Phase discriminator (the FM detector)

If \(s[n]\) is the current IQ sample and \(s[n-1]\) the previous one, the instantaneous phase change is

\[
\Delta\phi = \arg\big(s[n]\, s[n-1]^*\big)
\]

That \(\Delta\phi\) *is* the audio (scaled so that 75 kHz deviation maps to a sensible amplitude). Back to the clock hand: measure how far it turned since the last sample, and you have measured its speed. Speed is frequency; frequency is the message. This is the discrete **polar discriminator**, cousin of the analog Foster–Seeley and quadrature detectors. No magic — the clock hand's speed *is* the music.

### Live retune

While **Start** is active, clicking another preset does not rebuild the graph. `SdrPlayer` posts `DspCommand::TuneFrequency` to the Seify source. The LO jumps; the rest of the chain keeps running. That is why switching stations feels instant after the first start, while the very first **Start** takes a moment — that is the flowgraph being constructed and the USB stream negotiated.

### What the UI is doing

The Angular dial is not a radio. It is a **front panel**: presets in `~/.sdr-fm/stations.json`, a selected frequency in kHz, Tauri `invoke("start_fm")` / `stop_fm`. All RF lives in Rust. The vintage look is a deliberate joke on this arrangement — a wooden Rigonda console whose scale pointer is a `<div>` and whose tuning capacitor is a JSON file.

---

## 10. Sample rates you will see

| Rate | Role |
|------|------|
| **1.024 MHz** (default IQ) | Dongle streaming. Override with `SDR_FM_SAMPLE_RATE`. RTL-SDR **rejects** 300–900 kHz (e.g. 768 kHz). |
| **~256 kHz** | Channel after decimation — one WBFM haystack |
| **48 kHz** | Sound card / `AudioSink` |

Two valid RTL2832 bands exist: ~225–300 kHz and ~0.9–3.2 MHz. The gap is a hardware fact about how the chip divides its master clock, not a software whim; the app snaps illegal rates to a neighbour and logs the fact.

Higher is not automatically better. A faster IQ rate shows you more spectrum at once and helps the scanner, but it also means more USB traffic, more CPU, and more chances for dropped samples on a small machine. On a Raspberry Pi, lowering the rate is the first thing to try when audio stutters:

```bash
export SDR_FM_SAMPLE_RATE=256000
```

Gain is fixed at **40 dB** with AGC off during scan (and the playback source uses the same 40 dB). Automatic gain control sounds desirable, but it would breathe the noise floor up and down, confusing both your ears and every threshold the detector relies on. A fixed gain gives measurements that mean the same thing from one second to the next.

---

## 11. Noise, SNR, and why the antenna still matters

**Thermal noise** is always there. Every object above absolute zero jiggles its electrons, and that jiggle appears at your receiver as a hiss that no engineering can remove — only out-shout. **SNR** (signal-to-noise ratio) is "how tall is the haystack compared to the grass," measured in the decibels from [§2](#2-decibels-the-volume-knob-of-the-universe).

Software-defined radio moves the receiver into a computer. It does not repeal physics:

- **A wet noodle antenna** → haystacks sink into the grass. No amount of gain helps, because gain amplifies the noise equally. This is the single biggest quality lever you have, and it costs almost nothing: get the wire vertical, get it near a window, get it away from the metal case of a laptop.
- **A dongle next to a laptop charger** → spurs (thin spikes) everywhere. Switching power supplies are radio transmitters that nobody licensed.
- **The RTL-SDR has a DC spike** at the tuned frequency (an artefact of ADC offset) and analog filters that droop near the edges of its range. Those are hardware facts, not software bugs. It is why the scanner deliberately blanks an 80 kHz hole around each LO, and why the RDS pass tunes 200 kHz *off* the station it is listening to.
- **Multipath.** Your signal arrives twice — once directly, once bounced off a building — and the copies partially cancel. In a moving car this is the fluttering "picket fencing" you hear on a fringe station. At a desk it means moving the antenna 30 cm can matter more than any setting in this app.

When listening, you judge SNR with your ears. When scanning, the app judges it with a **radiometer** on the spectrum — same quantity, different sense organ. That story is the next document.

---

## 12. Eight things beginners usually get wrong

| Belief | Reality |
|--------|---------|
| "A station is a single frequency." | It is a ~180 kHz **haystack**. The number on the dial is only its centre. |
| "More gain means better reception." | Gain lifts signal *and* noise together. Past the noise floor it only adds distortion. Antennas beat gain, always. |
| "Nothing found = nothing there." | Detection needs the signal above the grass. A poor antenna makes real stations statistically invisible. |
| "An empty name means it is not a real station." | Many broadcasters transmit no RDS at all, or a weak one. Frequency is truth; names are a bonus. |
| "Digital would be immune to noise." | RDS is digital and it still fails first at the fringe. Digital changes the failure *shape* (works, then abruptly does not), not the physics. |
| "The spike in the middle of the waterfall is a station." | That is the dongle's own DC offset following your LO around. If it moves when you retune, it is not out there. |
| "Stereo is strictly better." | Stereo uses a quieter, noisier layer of the multiplex. On a weak station, mono genuinely sounds better. |
| "48 kHz is the radio's sample rate." | 48 kHz is the *audio* rate. The radio side runs around 1 MHz. Two clocks, two jobs ([§10](#10-sample-rates-you-will-see)). |

---

## 13. Fine tuning: when the "right" frequency is slightly wrong

Everything above says FM stations live on an exact 100 kHz grid. So why does the app have **FINE −/+** buttons next to the frequency readout, nudging the station by 5 kHz a click?

Because the grid is where the transmitter *is*, and your receiver may disagree about what that number means.

**Your dongle's crystal is not perfect.** Every RTL-SDR contains a quartz crystal that sets its sense of frequency, and cheap crystals are off by some parts per million (**PPM**). An error of 30 PPM at 100 MHz is 3 kHz — enough to shift the station slightly off-centre in your receiver's channel filter. Crystals also drift as they warm up, so a dongle can be correct after ten minutes and wrong when cold.

**The detector can miss.** Auto-detection picks the raster channel with the strongest evidence. When two transmitters sit close together, or when the band edge clips a haystack, or when a spur leans on one shoulder of the blob, the winning channel can land 100 kHz away from where your ears would put it.

**Off-centre tuning has an audible cost.** The channel filter keeps ~256 kHz around where you told it to look. If the station is 30 kHz off that centre, one edge of the haystack is being trimmed while noise is let in on the other side. You hear it as distortion on loud passages, extra hiss, and stereo that will not lock.

So the fine-tune buttons are the software equivalent of nudging a vernier dial while listening:

- Each click moves the selected station by **5 kHz** — small enough to be a correction, not a channel change.
- The new frequency is saved to `~/.sdr-fm/stations.json` **immediately**, so tomorrow's session starts where you left off.
- If audio is playing, the tuner follows at once via the same live-retune path from [§9](#9-from-antenna-to-speaker-in-this-app). No gap, no restart.
- If the station's label is just its frequency, the label follows too. A real name like *Радіо П'ятниця* is left alone — it is yours, not a derived value.
- The readout shows three decimals once you leave the raster (`103.305 MHz`), so you can always see that a preset has been hand-corrected.

Tune by ear: nudge one way until it degrades, go back the other way, stop at the point where the hiss is lowest and the loud passages stay clean. That is peaking a signal, and it is the oldest skill in radio.

---

## 14. Experiments to try tonight

Theory sticks better when you have watched it happen.

**1. Prove that wavelength is real.** Tune a station that is slightly weak. Now change the antenna: extend it, make it vertical, move it near a window. Reception changes far more than any software setting will. The wire's *orientation* matters because FM broadcast is usually vertically or circularly polarised.

**2. Watch the DC spike follow you.** Tune to an empty frequency and look at the centre of the spectrum in SDR++ (or trust [§11](#11-noise-snr-and-why-the-antenna-still-matters)). There is a spike. Retune 200 kHz away. The spike moved with you. Real stations do not do this.

**3. Feel the capture effect.** Find two frequencies where a weak and a strong station are close. Notice that you never hear both at once — the stronger one owns the channel entirely, exactly as [§5](#5-amplitude-vs-frequency-am-and-fm) predicts.

**4. Test Carson's rule with your ears.** Tune 100 kHz off a strong local station — one raster click away. You will still hear it, distorted, because your 256 kHz channel filter is still catching half its 180 kHz haystack. Now go 300 kHz off: silence. You have just measured the width of a radio station without a single instrument.

**5. Hear pre-emphasis by breaking it.** Not something the UI exposes, but if you change the 75 µs constant in `flowgraph.rs` to something far too small and rebuild, everything turns bright and hissy. That is what [§8](#8-pre-emphasis-and-de-emphasis-the-75-µs-trick) is protecting you from.

**6. Correct a station by hand.** Take a scanned preset that sounds slightly rough and click **FINE −** and **FINE +** while listening ([§13](#13-fine-tuning-when-the-right-frequency-is-slightly-wrong)). If one direction is consistently cleaner, your dongle's crystal or the detector was a few kilohertz off, and you have just fixed it permanently.

---

## 15. Glossary

| Term | Plain language |
|------|----------------|
| **Carrier** | The station's centre frequency |
| **Baseband** | The audio/MPX after the carrier has been removed |
| **IQ** | Two-rail complex sample of baseband |
| **LO** | Local oscillator — the frequency the tuner "mixes against" |
| **Heterodyne** | Mixing two frequencies to shift a signal somewhere convenient |
| **Deviation** \(\Delta f\) | How far FM is allowed to wander |
| **Occupied bandwidth** | How much RF spectrum the signal actually paints |
| **WBFM** | Broadcast-style FM (±75 kHz), not handheld-radio FM |
| **Raster** | Legal channel grid (in Region 1: 100 kHz from 87.5 MHz) |
| **MPX** | The stacked baseband carrying mono, stereo, and RDS |
| **Pilot** | The 19 kHz tone that announces stereo |
| **RDS** | Slow digital data at 57 kHz — station names and more |
| **De-emphasis** | The treble cut that undoes the transmitter's boost and kills hiss |
| **Decibel (dB)** | A logarithmic ratio: +3 dB is double, +10 dB is ten times |
| **SNR** | How far the signal stands above the noise, in dB |
| **Noise floor** | The hiss that is always present — the "grass" under everything |
| **Spur / birdie** | A narrow fake signal generated by electronics, not broadcasting |
| **Aliasing** | Undersampling artefact — a signal appearing where it is not |
| **PPM** | Parts per million; how far the dongle's crystal is off |
| **Capture effect** | FM's habit of letting the stronger station erase the weaker |
| **Multipath** | Reflections arriving late and partially cancelling the signal |
| **PSD** | Power spectral density — "brightness vs frequency" |
| **CFAR** | Constant false alarm rate — a detector that tracks the local noise |

---

## 16. Where to read in the code

| Topic | File |
|-------|------|
| Open dongle, sample-rate policy | `src-tauri/src/dsp/mod.rs` |
| Listen pipeline | `src-tauri/src/dsp/flowgraph.rs` |
| Start / stop / retune | `src-tauri/src/sdr.rs` |
| Presets | `src-tauri/src/config/stations.rs` |
| Spectrum and detection | `src-tauri/src/dsp/spectrum.rs`, `src-tauri/src/dsp/detect.rs` |
| UI dial, Scan button, fine tune | `src/app/` |

---

## 17. Where to go next

You now have enough theory to understand why 101.5 MHz is a *place*, why the waterfall shows a *blob*, and why the app decimates before it demodulates.

The sequel — **[FM auto-detection](FM_AUTO_DETECTION.md)** — is how we teach the computer to click those blobs the way you would in SDR++: a Welch spectrum, a CFAR noise estimate, a radiometer, and a width test, which together turn "that looks like a station" into arithmetic.

If you want to go deeper into the classical material, the search terms that unlock the textbooks are: *superheterodyne receiver*, *Carson's bandwidth rule*, *quadrature sampling*, *polar discriminator*, *FM capture effect*, *ITU-R BS.450* (the raster), and *ITU-R BS.412* (deviation and MPX).
