<div align="center">

<h1><img src="public/logo.png" width="80" align="absmiddle"> ReLightHost</h1>

**A real-time audio plugin host built with Rust, React, and Tauri**

ReLightHost is a desktop audio host for loading external plugins into a linear chain, routing live audio through them, and managing the whole session from a native Tauri app.

[![Version](https://img.shields.io/badge/version-2.4.1-9b72cf?style=for-the-badge)](https://github.com/hiimgyn/ReLightHost)
[![Platform](https://img.shields.io/badge/platform-Windows-0d7adf?style=for-the-badge)](https://github.com)
[![Rust](https://img.shields.io/badge/rust-1.77%2B-orange?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Tauri](https://img.shields.io/badge/tauri-2.x-24c8db?style=for-the-badge&logo=tauri&logoColor=white)](https://tauri.app)

</div>

---

## Overview

ReLightHost is a lightweight plugin host with a focus on live audio routing, session restore, and a clean workflow for managing plugins. It supports VST3, VST2, CLAP, and built-in processors, and it keeps the audio chain, device configuration, mute state, and loopback state in sync with the backend.

The app is centered around a drag-and-drop signal chain, a searchable plugin library, native plugin GUI support where available, and a footer that surfaces latency, plugin count, CPU, RAM, and VU metering in real time.

VST3, VST2, and CLAP hosting are all implemented as raw FFI against each format's native ABI — no host-side SDK dependency for any of the three. Plugin GUI windows embed natively (Win32 `HWND`, cross-process where relevant) instead of any web-based or remote-rendered approach.

> **Platform note:** the audio/plugin-hosting backend (ASIO/WASAPI via CPAL, VST3/VST2/CLAP hosting, native GUI embedding) is Windows-only today. The project builds on other platforms but plugin hosting is stubbed out there.

---

## Features

| Feature | Description |
|---|---|
| Plugin chain | Add, remove, reorder, swap, bypass, and rename plugins in a linear processing chain |
| Plugin library | Search, filter, and group plugins by manufacturer before adding them to the chain |
| Multi-format support | VST3, VST2 (.dll), CLAP, and built-in processors — all hosted via raw FFI, no per-format SDK dependency |
| Native GUI support | Open plugin editors in native windows when the plugin exposes one |
| VST3 crash isolation | A VST3 plugin that repeatedly crashes the host gets automatically reloaded in an isolated child process on subsequent launches, so a fragile plugin can no longer take the whole app down |
| Audio routing | Select input/output devices, monitor live audio, and toggle hardware loopback |
| Real-time meters | Live VU meter plus CPU and RAM monitoring in the app footer |
| Session restore | Restore audio settings and plugin state on launch |
| Auto-save | Structural chain changes are persisted automatically |
| System tray | Minimize to tray, restore from the tray, and a full iconized tray menu (mute/monitor-output toggles reflect live state) |
| Startup options | Windows startup registration and show-hidden behavior on launch |
| Theme support | Persistent dark and light theme toggle |
| Built-in processors | Compressor, noise suppressor, and a 4-stage voice processor are bundled with the host |

---

## Screenshot

![Main window](Screenshot.png)

---

## Tech Stack

<details>
<summary><strong>Frontend</strong></summary>
<br>

| Technology | Role |
|---|---|
| React 19 | UI framework |
| TypeScript | Type safety |
| Vite 8 | Build tool and dev server |
| Ant Design 6 | Component library |
| Zustand | State management |
| Tailwind CSS 4 | Utility styling |

</details>

<details>
<summary><strong>Backend</strong></summary>
<br>

| Technology | Role |
|---|---|
| Tauri 2 | Desktop shell and IPC bridge |
| CPAL | Cross-platform audio I/O (ASIO/WASAPI on Windows) |
| vst3-rs | VST3 hosting bindings |
| Raw FFI (no crate) | VST2 hosting — direct `AEffect`/dispatcher ABI, no SDK dependency |
| Raw FFI (no crate) | CLAP hosting — direct C ABI, no SDK dependency |
| windows-sys | Native Win32 interop — window embedding, tray, DPI, process/job management |
| ringbuf | Lock-free audio buffers |
| parking_lot | Fast synchronization primitives |
| nnnoiseless | Built-in noise suppression |
| serde / serde_json | Session, preset, and IPC serialization |
| sysinfo | CPU and RAM monitoring |

</details>

---

## Getting Started

### Prerequisites

- Node.js 20+
- pnpm 9+
- Rust stable 1.77+
- Tauri CLI v2
- Windows only: Visual Studio Build Tools with the C++ workload

### Install and Run

```powershell
pnpm install
pnpm tauri dev
```

### Build

```powershell
pnpm tauri build
```

Optional checks:

```powershell
pnpm build
cd src-tauri
cargo check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

### Release Signing

If you build signed updater artifacts, set `TAURI_SIGNING_PRIVATE_KEY` before running a release build. If the key is password-protected, also set `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = @"
PASTE_YOUR_PRIVATE_KEY_HERE
"@
pnpm tauri build
```

---

## Audio Notes

ReLightHost uses CPAL for device I/O. On Windows, ASIO is the lowest-latency path when available, while WASAPI is the fallback.

If you want ASIO on Windows, set `CPAL_ASIO_DIR` to the location of the Steinberg ASIO SDK before building.

```powershell
$env:CPAL_ASIO_DIR = "C:\ASIO_SDK"
```

---

## Plugin Support

| Format | Extension | Hosting | GUI | Notes |
|---|---|---|---|---|
| VST3 | `.vst3` | `vst3-rs` (COM/vtable bindings) | Native when supported | Binary state persistence; repeatedly-crashing plugins are auto-sandboxed in a child process on later loads |
| VST2 | `.dll` | Raw FFI, no crate | Plugin-provided, native window | Chunk-based state persistence (`effGetChunk`/`effSetChunk`) |
| CLAP | `.clap` | Raw FFI, no crate | Plugin-provided | Native plugin format support |
| Built-in | - | Native Rust DSP | React UI | Bundled processors shipped with the app |

### Default Scan Paths

Windows:

```
C:\Program Files\Common Files\VST3
C:\Program Files\VSTPlugins
C:\Program Files\Steinberg\VSTPlugins
C:\Program Files\Common Files\CLAP
%LOCALAPPDATA%\Programs\Common\CLAP
%LOCALAPPDATA%\Programs\Common\VST2
```

Custom directories can be added from Plugin Settings.

---

## Crash Resilience

A plugin crashing the whole app is the worst failure mode for a live audio host, so ReLightHost layers a few defenses:

- **Panic protection** — Rust-side panics inside a plugin call are caught and turned into a bypass instead of taking the process down.
- **Per-block safety** — the plugin chain never blocks the real-time audio callback waiting on a plugin; a busy or slow plugin is skipped for that block rather than causing an underrun.
- **VST3 sandboxing** — a native crash (access violation, heap corruption) in a plugin's own compiled code can't be caught in-process at all. ReLightHost tracks crash history per VST3 plugin across app restarts, and once a plugin crosses the crash threshold it's loaded inside a dedicated `vst3_sandbox_host.exe` child process instead — audio and GUI both keep working exactly as before, but a crash there only takes down that child, not the app.

---

## Built-in Processors

The bundled processors are compiled into the host, so you can use them without installing external plugins.

### Compressor

Feed-forward RMS compressor with soft-knee and parallel mix.

### Noise Suppressor

RNNoise-based speech noise suppression with mix, gate, and output gain controls.

### Voice Designer

Four-stage voice processor: 3-band EQ → saturation (drive) → doubler (stereo width) → limiter (output ceiling).

---

## Project Structure

```
ReLightHost/
├── src/                        # Frontend (React + TypeScript)
│   ├── App.tsx                 # Session restore and window lifecycle
│   ├── components/             # Layout, chain, audio, plugin, settings, GUI panels
│   ├── stores/                 # Zustand state stores
│   └── lib/                    # Tauri wrappers and shared types
└── src-tauri/                  # Backend (Rust)
    ├── src/
    │   ├── lib.rs               # App state, commands, tray setup
    │   ├── bin/                 # vst3_sandbox_host.exe — sandboxed VST3 child process
    │   ├── bootstrap/           # Window and tray bootstrapping
    │   ├── commands/            # IPC commands
    │   ├── core/                # Session, autosave, timing, threading
    │   ├── domain/               # Config and preset models
    │   └── plugins/
    │       ├── core/             # Scanner, instance manager, crash protection
    │       ├── processor/        # VST3 / VST2 / CLAP hosting (raw FFI)
    │       │   └── vst3_sandbox/ # Out-of-process VST3 host: protocol + registry
    │       ├── gui/               # Native plugin editor window embedding
    │       └── builtin/          # Compressor, noise suppressor, Voice Designer
    └── icons/tray/               # Tray menu icons
```

---

## Contributing

Contributions are welcome. Open an issue first if you want to discuss a larger change.

```bash
pnpm build
cd src-tauri && cargo check
```

---

<div align="center">

Made with 💖 by Gyn

</div>
