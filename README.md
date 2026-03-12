# ReLightHost

> A modern, real-time audio plugin host built with Rust and React — designed for musicians and audio engineers who need low-latency, multi-format plugin processing with a clean, native-feeling UI.

![Version](https://img.shields.io/badge/version-1.3.0-9b72cf?style=flat-square)
![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-blue?style=flat-square)
![Rust](https://img.shields.io/badge/rust-1.77%2B-orange?style=flat-square)
![Tauri](https://img.shields.io/badge/tauri-2.x-24c8db?style=flat-square)

---

## Table of Contents

- [Overview](#overview)
- [Features](#features)
- [Screenshots](#screenshots)
- [Tech Stack](#tech-stack)
- [Getting Started](#getting-started)
  - [Prerequisites](#prerequisites)
  - [Development](#development)
  - [Production Build](#production-build)
- [ASIO Setup (Windows)](#asio-setup-windows)
- [Plugin Support](#plugin-support)
- [Audio Pipeline](#audio-pipeline)
- [Built-in Processors](#built-in-processors)
- [Project Structure](#project-structure)
- [Preset & Session Management](#preset--session-management)
- [System Tray](#system-tray)
- [Contributing](#contributing)

---

## Overview

ReLightHost is a lightweight, cross-platform audio plugin host. It lets you load VST2, VST3, and CLAP plugins into a linear processing chain, route audio from any input device through that chain, and output to any output device — all in real time with sub-millisecond latency when using ASIO.

It ships three built-in processors (Compressor, RNNoise Noise Suppressor, and Pitch Shifter) that require no external plugins, making it useful out of the box even without a plugin library.

---

## Features

- **Multi-format plugin hosting** — VST2 (`.dll`), VST3 (`.vst3`), CLAP (`.clap`), and built-in processors
- **Linear plugin chain** — Drag-and-drop to reorder plugins; per-plugin bypass toggle
- **Native GUI support** — VST3 and VST2 plugins open their original UI in a native Win32 window
- **ASIO / WASAPI / CoreAudio / ALSA / JACK** — Full audio API support via CPAL
- **Hardware Out monitoring** — Route processed audio to a second output device (headphones/speakers); toggle with the loopback button
- **Real-time VU meter** — Live L/R output level monitoring in the footer
- **System stats** — Per-process CPU and RAM usage displayed in real time
- **Preset management** — Save and load named plugin chains (including parameter states and VST3 binary state)
- **Auto-save session** — Plugin chain and audio config restored automatically on next launch
- **System tray** — Minimize to tray, mute toggle, quick-access menu
- **Run on startup** — Optional Windows startup entry
- **Built-in processors** — Compressor · Noise Suppressor (RNNoise) · Pitch Shifter
- **Dark / Light theme** — Persistent theme toggle
- **Plugin crash isolation** — A panicking plugin is caught and bypassed; the host keeps running

---

## Screenshots

![Main window](Screenshot.png)

---

## Tech Stack

### Frontend
| Technology | Version | Role |
|---|---|---|
| React | 18.3 | UI framework |
| TypeScript | 5.7 | Type safety |
| Vite | 6.0 | Build tool / dev server |
| Ant Design | 6.x | UI component library |
| Zustand | 5.0 | State management |
| TailwindCSS | 3.4 | Utility CSS |
| lucide-react | 0.462 | Icon library |

### Backend (Rust)
| Crate | Role |
|---|---|
| `tauri 2.x` | Desktop shell + IPC bridge |
| `tauri-plugin-updater` | Auto-update support |
| `tauri-plugin-single-instance` | Single-instance enforcement |
| `cpal 0.15` | Cross-platform audio I/O (ASIO, WASAPI, CoreAudio, ALSA, JACK) |
| `vst3-rs 0.3` | VST3 plugin hosting |
| `vst-rs 0.3` | VST2 plugin hosting |
| `midir 0.10` | MIDI I/O |
| `ringbuf 0.4` | Lock-free SPSC ring buffer (audio thread safety) |
| `parking_lot 0.12` | High-performance RwLock / Mutex |
| `nnnoiseless 0.5` | Built-in RNNoise noise suppression |
| `dasp 0.11` | Audio DSP primitives (stereo frame processing) |
| `serde_json` | Preset & config serialization |
| `sysinfo` | Per-process CPU and RAM monitoring |

---

## Getting Started

### Prerequisites

- **Rust** 1.77+ — [rustup.rs](https://rustup.rs)
- **Node.js** 18+ and **pnpm** — `npm install -g pnpm`
- **Tauri CLI** — installed automatically via pnpm
- **Windows SDK** (Windows builds) — required for native Win32 GUI hosting
- **ASIO SDK** (optional, Windows) — see [ASIO Setup](#asio-setup-windows)

### Development

```bash
# Install frontend dependencies
pnpm install

# Start the app in development mode (Vite HMR + Tauri dev window)
pnpm tauri dev
```

### Production Build

```bash
# Build frontend + compile Rust in release mode
pnpm tauri build
```

Output binaries will be in `src-tauri/target/release/`. Installers (NSIS `.exe` / MSI) are placed in `src-tauri/target/release/bundle/`.

---

## ASIO Setup (Windows)

ASIO provides the lowest possible audio latency on Windows. To enable it:

1. Download the **ASIO SDK** from [Steinberg's developer portal](https://www.steinberg.net/developers/).
2. Extract it somewhere, e.g. `C:\ASIO_SDK`.
3. Set the environment variable before building:
   ```powershell
   $env:CPAL_ASIO_DIR = "C:\ASIO_SDK"
   ```
4. Run `pnpm tauri dev` or `pnpm tauri build` as normal.

Without ASIO, ReLightHost falls back to WASAPI (Windows), which still works but has higher latency.

---

## Plugin Support

| Format | Extension | GUI | State |
|---|---|---|---|
| VST3 | `.vst3` | Native Win32 (`IPlugView`) | Binary blob (`IComponent::getState`) |
| VST2 | `.dll` | Plugin-provided | Parameters |
| CLAP | `.clap` | Custom | Plugin state |
| Built-in | — | React (Ant Design) | Parameters in preset JSON |

### Default Scan Paths (Windows)

```
C:\Program Files\Common Files\VST3
C:\Program Files\Common Files\CLAP
%LOCALAPPDATA%\Programs\Common\VST3
%LOCALAPPDATA%\Programs\Common\CLAP
```

Custom scan directories can be added in the **Plugin Settings** dialog (⚙ icon).

### Plugin Crash Protection

Plugins are wrapped with `catch_unwind`. If a plugin panics:
- The crash is logged
- That plugin instance switches to pass-through mode
- The rest of the chain continues processing normally

---

## Audio Pipeline

```
Input Device (CPAL stream)
        │
        ▼
Lock-free ring buffer (SPSC)
        │
        ▼
Audio callback (realtime thread)
  ┌─────────────────────────────────┐
  │  Plugin chain processing        │
  │   ┌──────────────────────────┐  │
  │   │ Plugin 1 (L/R)           │  │
  │   │ Plugin 2 (L/R)           │  │
  │   │ ...                      │  │
  │   └──────────────────────────┘  │
  │  (bypassed plugins pass through) │
  │           │                     │
  │   VU meter sampling             │
  └─────────────────────────────────┘
        │
        ├──► Primary output device  (always active while monitoring)
        └──► Hardware Out device    (only when loopback button is ON)
```

**Latency** is determined by buffer size and sample rate:

$$\text{latency (ms)} = \frac{\text{buffer\_size}}{\text{sample\_rate}} \times 1000$$

Example: 1024 samples @ 48 kHz = **21.3 ms**

ASIO with a 128-sample buffer @ 48 kHz = **2.7 ms**

---

## Built-in Processors

All three built-in processors ship inside the host — no external plugin files needed.

### Compressor
A feed-forward RMS compressor with full control over dynamics.

| Parameter | Range | Default |
|---|---|---|
| Threshold | −60 dB → 0 dB | −18 dB |
| Ratio | 1:1 → 20:1 | 4:1 |
| Attack | 0.1 ms → 200 ms | 10 ms |
| Release | 10 ms → 2000 ms | 100 ms |
| Makeup Gain | 0 dB → +30 dB | 0 dB |
| Knee | 0 dB → 12 dB | 3 dB |
| Parallel Mix | 0% → 100% | 100% |

### Noise Suppressor
Powered by [RNNoise](https://jmvalin.ca/demo/rnnoise/) (nnnoiseless). Uses a recurrent neural network trained on speech to remove background noise without affecting voice.

| Parameter | Range | Default |
|---|---|---|
| Mix | 0% → 100% | 100% |
| VAD Gate Threshold | 0.0 → 1.0 | 0.0 (off) |
| Gate Attenuation | 0.0 → 1.0 | 0.0 |
| Output Gain | −24 dB → +12 dB | 0 dB |

### Pitch Shifter
Dual-buffer overlap-add pitch shifter with a **Voice Color Panel** — a 2D XY pad inspired by VoiceMeeter's Intellipan, where:
- **X axis** = Fine tune (±100 cents)
- **Y axis** = Semitones (±24 st)

Seven voice color presets are plotted on the pad as named markers (DEEP, LOW, WARM, NATURAL, BRIGHT, HIGH, ULTRA). Click a marker or drag anywhere to blend freely.

| Parameter | Range | Default |
|---|---|---|
| Semitones | −24 st → +24 st | 0 |
| Fine | −100 ¢ → +100 ¢ | 0 |
| Wet Mix | 0% → 100% | 100% |

---

## Project Structure

```
ReLightHost/
├── src/                        # Frontend (React + TypeScript)
│   ├── App.tsx                 # Root component; session restore logic
│   ├── main.tsx                # Entry point; context menu block
│   ├── index.css               # Global styles
│   ├── components/
│   │   ├── Layout.tsx          # Shell: header + footer (VU meter, stats, latency)
│   │   ├── Header.tsx          # App bar: logo, mute, loopback, theme, settings
│   │   ├── PluginChain.tsx     # IN → [Plugin1] → [Plugin2] → OUT (drag & drop)
│   │   ├── PluginCard.tsx      # Per-plugin card: bypass, info, open GUI
│   │   ├── PluginLibrary.tsx   # Browse & search available plugins; add to chain
│   │   ├── PluginSettings.tsx  # Custom scan path management + plugin rescan
│   │   ├── PluginInfoModal.tsx # Plugin metadata viewer
│   │   ├── AudioSettings.tsx   # Device, sample rate, buffer size, Hardware Out
│   │   ├── AppSettings.tsx     # Startup & tray options; about info
│   │   ├── PresetManager.tsx   # Save / load / delete named presets
│   │   ├── VUMeter.tsx         # Real-time L/R dB bar meter
│   │   ├── CompressorGui.tsx   # Built-in compressor UI
│   │   ├── NoiseSuppressorGui.tsx  # Built-in noise suppressor UI
│   │   └── VoiceGui.tsx        # Built-in pitch shifter UI (Voice Color Panel XY pad)
│   ├── stores/
│   │   ├── audioStore.ts       # Audio device, SR, buffer, monitoring, loopback state
│   │   ├── pluginStore.ts      # Plugin library, active chain, scan state
│   │   ├── presetStore.ts      # Preset list
│   │   └── themeStore.ts       # Dark/light theme persistence
│   └── lib/
│       ├── tauri.ts            # All Tauri IPC command wrappers
│       ├── types.ts            # Shared TypeScript type definitions
│       └── index.ts            # Re-exports
│
├── src-tauri/                  # Backend (Rust)
│   ├── src/
│   │   ├── lib.rs              # AppState, Tauri command handlers, tray setup
│   │   ├── main.rs             # Entry point
│   │   ├── config.rs           # JSON config persistence (audio settings, app options)
│   │   ├── preset.rs           # Preset serialization/deserialization
│   │   ├── audio/
│   │   │   ├── manager.rs      # CPAL stream lifecycle; monitoring & loopback toggle
│   │   │   ├── device.rs       # Device enumeration
│   │   │   ├── types.rs        # AudioStatus, AudioConfig, AudioDeviceInfo
│   │   │   └── vu_meter.rs     # Peak / RMS level tracking
│   │   └── plugins/
│   │       ├── scanner.rs      # VST/CLAP directory scanner + built-in registration
│   │       ├── instance.rs     # Per-plugin instance wrapper
│   │       ├── types.rs        # PluginInfo, PluginInstanceInfo, PluginFormat
│   │       ├── vst3_processor.rs
│   │       ├── vst3_gui.rs
│   │       ├── vst2_processor.rs
│   │       ├── vst2_gui.rs
│   │       ├── clap_processor.rs
│   │       ├── clap_gui.rs
│   │       ├── crash_protection.rs
│   │       └── builtin/
│   │           ├── mod.rs          # Factory + default params registry
│   │           ├── compressor.rs
│   │           ├── noise_suppressor.rs
│   │           └── pitch_shifter.rs
│   ├── Cargo.toml
│   └── tauri.conf.json
│
├── package.json
├── vite.config.ts
└── tailwind.config.js
```

---

## Preset & Session Management

### Presets

Presets are JSON files stored in the app data directory:

- **Windows:** `%APPDATA%\ReLightHost\presets\`
- **macOS/Linux:** `~/.config/ReLightHost/presets/`

Each preset captures the full plugin chain state:

```json
{
  "name": "My Chain",
  "created_at": "2026-03-12T10:00:00Z",
  "plugin_chain": [
    {
      "plugin_id": "vst3_fabfilter_pro-c2",
      "plugin_name": "Pro-C 2",
      "plugin_vendor": "FabFilter",
      "plugin_format": "vst3",
      "bypassed": false,
      "parameters": [
        { "id": 0, "name": "Threshold", "value": -18.0 }
      ],
      "vst3_state": "<base64 binary blob>"
    }
  ]
}
```

### VB-Cable

If you want to route processed output to Discord, Teams, OBS, etc. without VoiceMeeter, install VB-Audio Cable and select it as your **Primary Output** in Audio Settings.

[Download VB-Audio Cable](https://vb-audio.com/Cable/index.htm)

### Hardware Out

Set a **Hardware Out** device in Audio Settings to route processed audio to a second physical output (e.g. headphones). Press the **loopback button** (⇄) in the header to toggle monitoring on and off. This uses a separate CPAL stream gated by an atomic flag — toggling it is instantaneous with no audio glitch.

### Auto-save

Every change to the plugin chain (add, remove, reorder, bypass toggle) triggers an auto-save to the `__autosave__` preset. On the next app launch, this chain is automatically restored.

### Session Restore Sequence

1. `restore_session()` Tauri command is called on startup
2. Audio config (device, SR, buffer) is restored from `config.json`
3. Plugin chain is restored from `__autosave__` preset
4. Audio stream is started (with a 2-second delay for Voicemeeter ASIO devices)
5. Frontend syncs its stores from the restored backend state

---

## System Tray

When **Minimize to Tray** is enabled (Application Settings → Minimize to Tray), closing the window hides the app to the system tray instead of quitting.

### Tray Context Menu

| Item | Action |
|---|---|
| Show ReLightHost | Restore and focus the main window |
| Mute Audio / Unmute Audio | Toggle output mute |
| Audio Settings… | Open Audio Settings dialog |
| Application Settings… | Open App Settings dialog |
| Exit | Quit the application |

The tray icon tooltip changes to **"ReLightHost (Muted)"** when muted.

---

## Contributing

Contributions are welcome! Please open an issue first to discuss the change before submitting a pull request.

```bash
# Run type checks
pnpm tsc --noEmit

# Run Rust checks
cd src-tauri && cargo check
```

---

*Made with ❤️ by Gyn*
