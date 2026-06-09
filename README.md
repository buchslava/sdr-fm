# SDR FM

Desktop FM radio receiver built with **Tauri**, **Angular**, and **RTL-SDR**.

Enter a WFM frequency in kHz and listen to FM broadcast stations through your RTL-SDR dongle.

## Prerequisites

### Build tools

- [Node.js](https://nodejs.org/) (v18+)
- [Rust](https://www.rust-lang.org/tools/install)

### RTL-SDR system libraries

The app uses **FutureSDR + SoapySDR** to talk to the dongle. Three packages are required:

| Package | Purpose |
|---------|---------|
| `soapysdr` | SDR hardware abstraction layer |
| `librtlsdr` | Low-level RTL-SDR USB driver |
| `soapyrtlsdr` | SoapySDR plugin for RTL-SDR (**required** — without it you get `Device::make() no match`) |

**macOS (Homebrew):**

```bash
brew install soapysdr librtlsdr soapyrtlsdr
```

**Linux (Debian/Ubuntu):**

```bash
sudo apt install libsoapysdr-dev soapysdr-module-rtlsdr librtlsdr-dev
```

### Verify the dongle

Plug in your RTL-SDR, then:

```bash
SoapySDRUtil --find
SoapySDRUtil --probe="driver=rtlsdr"
```

You should see your device listed (e.g. `RTL-SDR Blog V4`, `driver = rtlsdr`). If `--find` returns *No devices found*, install `soapyrtlsdr` / `soapysdr-module-rtlsdr` and replug the dongle.

Optional low-level check with librtlsdr tools:

```bash
rtl_test
```

### Hardware

- RTL-SDR v4 (or compatible) dongle connected via USB

## Development

```bash
npm install
npm run tauri dev
```

## Usage

1. Enter a frequency in **kHz** (e.g. `101500` for 101.5 MHz).
2. Click **Listen** to start WFM demodulation and audio playback.
3. Click **Stop** to release the RTL-SDR device.

FM broadcast band: **64,000 – 1,080,000 kHz** (64 – 108 MHz).

## How it works

- The Rust backend opens the RTL-SDR via **SoapySDR** (`driver=rtlsdr`).
- A **FutureSDR** flowgraph demodulates WBFM in-process: decimate → phase discriminator → resample → de-emphasis.
- Audio plays through your default output via **cpal** (`AudioSink` at 48 kHz).
- Tauri bridges the Angular UI to the Rust SDR controller.

## Build

```bash
npm run tauri build
```

The packaged app will be in `src-tauri/target/release/bundle/`.
