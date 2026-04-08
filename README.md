<div align="center">

<h1><img src="public/logo.png" width="80" align="absmiddle"> ReLightHost </h1>

**A modern, real-time audio plugin host built with Rust, React, and Tauri**

*Designed for musicians and audio engineers who need low-latency, multi-format plugin processing, fast plugin browsing, and a clean native-feeling UI.*

[![Version](https://img.shields.io/badge/version-1.8.1-9b72cf?style=for-the-badge)](https://github.com/hiimgyn/ReLightHost)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-0d7adf?style=for-the-badge)](https://github.com)
[![Rust](https://img.shields.io/badge/rust-1.77%2B-orange?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Tauri](https://img.shields.io/badge/tauri-2.x-24c8db?style=for-the-badge&logo=tauri&logoColor=white)](https://tauri.app)

</div>

---

## Overview

ReLightHost is a lightweight, cross-platform audio plugin host. Load **VST2**, **VST3**, and **CLAP** plugins into a linear processing chain, route audio from any input device through that chain, and output to any output device — all in real time with sub-millisecond latency when using ASIO.

Three built-in processors — **Compressor**, **RNNoise Noise Suppressor**, and **Voice EQ** — are included, so the host is useful out of the box even without an external plugin library.

---

## Features

| | Feature | Description |
|:---:|---|---|
| 🔌 | **Multi-format hosting** | VST2 `.dll`, VST3 `.vst3`, CLAP `.clap`, and built-in processors |
| 🔎 | **Plugin browser** | Search, filter, and group plugins by manufacturer before adding them to the chain |
| 🔗 | **Linear plugin chain** | Drag-and-drop reordering; per-plugin bypass, rename, and GUI controls |
| 🖥 | **Native GUI support** | VST2 / VST3 / CLAP editors open in native plugin windows when supported |
| ⚡ | **ASIO / WASAPI / CoreAudio / ALSA / JACK** | Full audio API support via CPAL |
| 🔊 | **Audio routing** | Route processed audio to a second output device; loopback toggle for hardware monitoring |
| 📊 | **Real-time VU meter** | Live L/R peak and RMS level monitoring |
| 📈 | **System stats** | Per-process CPU and RAM usage in real time |
| 💾 | **Preset management** | Save/load named chains with parameters and preserved VST3 binary state |
| 🔄 | **Auto-save session** | Plugin chain and audio config are restored automatically on launch |
| 🖱 | **System tray** | Minimize to tray, mute toggle, hardware out toggle, quick-access menu |
| 🚀 | **Run on startup** | Optional Windows startup registry entry with show-window or start-hidden mode |
| 🌗 | **Dark / Light theme** | Persistent theme toggle |
| 🛡 | **Crash isolation + cooldown recovery** | Crash is isolated, audio stays alive, then auto-recovery is attempted safely |
| 📦 | **Frontend code-splitting** | Heavy modals and plugin UIs are lazy-loaded to reduce initial bundle cost |

---

## Screenshots

![Main window](Screenshot.png)

---

## Tech Stack

<details>
<summary><strong>Frontend</strong></summary>
<br>

| Technology | Version | Role |
|---|---|---|
| React | 18.3 | UI framework |
| TypeScript | 5.7 | Type safety |
| Vite | 6.0 | Build tool / dev server |
| Ant Design | 6.x | UI component library |
| Zustand | 5.0 | State management |
| TailwindCSS | 3.4 | Utility CSS |

</details>

<details>
<summary><strong>Backend (Rust)</strong></summary>
<br>

| Crate | Role |
|---|---|
| `tauri 2.x` | Desktop shell + IPC bridge |
| `tauri-plugin-updater` | Auto-update support |
| `tauri-plugin-single-instance` | Single-instance enforcement |
| `cpal 0.15` | Cross-platform audio I/O (ASIO, WASAPI, CoreAudio, ALSA, JACK) |
| `vst3-rs 0.3` | VST3 plugin hosting |
| `vst-rs 0.3` | VST2 plugin hosting |
| `ringbuf 0.4` | Lock-free SPSC ring buffer (audio thread safety) |
| `parking_lot 0.12` | High-performance RwLock / Mutex |
| `nnnoiseless 0.5` | Built-in RNNoise noise suppression |
| `serde_json` | Preset & config serialization |
| `sysinfo` | Per-process CPU and RAM monitoring |

</details>

---

## Getting Started

### Prerequisites

- [Node.js](https://nodejs.org/) 20+
- [pnpm](https://pnpm.io/) 9+
- [Rust](https://rustup.rs/) 1.77+
- [Tauri CLI v2](https://tauri.app/start/prerequisites/)
- **Windows only:** Visual Studio Build Tools with the C++ workload

### Development

```powershell
pnpm install
pnpm tauri dev
```

Optional quality gates:

```powershell
pnpm build
cd src-tauri
cargo check
cargo clippy --all-targets --all-features -- -D warnings
```

### Production Build

```powershell
pnpm tauri build
```

### Updater Signing Key (Required for Signed Update Artifacts)

Set `TAURI_SIGNING_PRIVATE_KEY` in your environment before running a release build.

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = @"
PASTE_YOUR_PRIVATE_KEY_HERE
"@
pnpm tauri build
```

If your private key is password-protected, also set:

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "your-password"
pnpm tauri build
```

Tip: copy `.env.example` to `.env.local` for local reference, but do not commit it.

> Output binaries → `src-tauri/target/release/`  
> Installers (NSIS `.exe` / MSI) → `src-tauri/target/release/bundle/`

---

## ASIO Setup (Windows)

> ASIO provides the lowest possible audio latency on Windows. Without it, ReLightHost falls back to WASAPI, which still works but with higher latency.

1. Download the **ASIO SDK** from [Steinberg's developer portal](https://www.steinberg.net/developers/)
2. Extract it, e.g. to `C:\ASIO_SDK`
3. Set the environment variable before building:
   ```powershell
   $env:CPAL_ASIO_DIR = "C:\ASIO_SDK"
   ```
4. Run `pnpm tauri dev` or `pnpm tauri build` as normal

---

## Plugin Support

| Format | Extension | GUI | State persistence |
|---|---|---|---|
| VST3 | `.vst3` | Native Win32 (`IPlugView`) | Binary blob (`IComponent::getState`) |
| VST2 | `.dll` | Plugin-provided | Parameters |
| CLAP | `.clap` | Custom | Plugin state |
| Built-in | — | React (Ant Design) | Parameters in preset JSON |

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

macOS:

```
/Library/Audio/Plug-Ins/VST3
/Library/Audio/Plug-Ins/VST
/Library/Audio/Plug-Ins/CLAP
~/Library/Audio/Plug-Ins/VST3
~/Library/Audio/Plug-Ins/VST
~/Library/Audio/Plug-Ins/CLAP
```

Linux:

```
/usr/lib/vst3
/usr/lib/vst
/usr/lib/clap
~/.vst3
~/.vst
~/.clap
```

> Custom directories can be added via **Plugin Settings → ＋ Add Path**.

### Crash Protection

All external plugins are wrapped with `catch_unwind`. If a plugin panics:
- The crash is logged with full details
- That instance switches to **temporary silence/pass-through protection mode**
- The host attempts **cooldown-based auto recovery** to avoid crash loops
- The rest of the chain continues processing normally

---

## Audio Pipeline

```
┌───────────────────────────────────────────┐
│           Input Device (CPAL)             │
└───────────────────┬───────────────────────┘
                    │
          Lock-free ring buffer
                    │
      ┌─────────────▼─────────────┐
      │   Real-time audio thread  │
      │  ┌───────────────────┐    │
      │  │  Plugin 1  (L/R)  │    │
      │  │  Plugin 2  (L/R)  │    │
      │  │  Plugin N  (L/R)  │    │
      │  └─────────┬─────────┘    │
      │            │              │
      │     VU meter sample       │
      └─────────────┬─────────────┘
                    │
        ┌───────────┴───────────┐
        │                       │
   Primary output          Hardware Out
  (always active)      (loopback button ON)
```

**Latency formula:**

$$\text{latency (ms)} = \frac{\text{buffer\\_size}}{\text{sample\\_rate}} \times 1000$$

| Buffer / Rate | Latency |
|---|---|
| 1024 samples @ 48 kHz (WASAPI) | **21.3 ms** |
| 512 samples @ 48 kHz | **10.7 ms** |
| 128 samples @ 48 kHz (ASIO) | **2.7 ms** |

---

## Built-in Processors

> All three processors are compiled directly into the host — no external plugin files required.

### 🗜 Compressor

Feed-forward RMS compressor with soft-knee and parallel mix.

| Parameter | Range | Default |
|---|---|---|
| Threshold | −60 → 0 dB | −18 dB |
| Ratio | 1:1 → 20:1 | 4:1 |
| Attack | 0.1 → 200 ms | 10 ms |
| Release | 10 → 2000 ms | 100 ms |
| Makeup Gain | 0 → +30 dB | 0 dB |
| Knee | 0 → 12 dB | 3 dB |
| Parallel Mix | 0 → 100% | 100% |

### 🔇 Noise Suppressor

Powered by [RNNoise](https://jmvalin.ca/demo/rnnoise/) — a recurrent neural network trained on speech to remove background noise without affecting voice quality.

| Parameter | Range | Default |
|---|---|---|
| Mix | 0 → 100% | 100% |
| VAD Gate Threshold | 0.0 → 1.0 | 0.0 (off) |
| Gate Attenuation | 0.0 → 1.0 | 0.0 |
| Output Gain | −24 → +12 dB | 0 dB |

### 🎙 Voice EQ

3-band EQ with harmonic drive, stereo width, and output ceiling — optimised for voice clarity and presence.

| Parameter | Range | Default |
|---|---|---|
| Low | −12 → +12 dB | 0 dB |
| Mid | −12 → +12 dB | 0 dB |
| High | −12 → +12 dB | 0 dB |
| Drive | 0 → 100% | 0% |
| Width | 0 → 100% | 0% |
| Ceiling | −12 → 0 dB | 0 dB |

---

## Project Structure

```
ReLightHost/
├── src/                            # Frontend (React + TypeScript)
│   ├── App.tsx                     # Session restore + window lifecycle
│   ├── main.tsx                    # Entry point
│   ├── index.css                   # Global styles and theme overrides
│   ├── theme.ts                    # Shared Ant Design theme tokens
│   ├── components/
│   │   ├── layout/
│   │   │   ├── Header.tsx          # Brand, engine state, mute, loopback, theme, settings
│   │   │   ├── Layout.tsx          # Shell + footer meters + system stats
│   │   │   └── VUMeter.tsx         # Real-time L/R meter
│   │   ├── chain/
│   │   │   ├── CurvedArrow.tsx     # Chain connectors
│   │   │   ├── PluginCard.tsx      # Per-plugin controls and GUIs
│   │   │   └── PluginChain.tsx     # Drag-and-drop plugin chain
│   │   ├── plugin/
│   │   │   ├── PluginInfoModal.tsx  # Plugin metadata details
│   │   │   ├── PluginLibrary.tsx    # Search/filter/add plugins
│   │   │   └── PluginSettings.tsx   # Scan paths + rescan
│   │   ├── audio/
│   │   │   └── AudioSettings.tsx    # Device, sample rate, buffer, hardware out
│   │   ├── settings/
│   │   │   └── AppSettings.tsx      # Startup, tray, about
│   │   └── plugin-gui/
│   │       ├── CompressorGui.tsx
│   │       ├── NoiseSuppressorGui.tsx
│   │       └── VoiceGui.tsx
│   ├── stores/
│   │   ├── audioStore.ts            # Audio engine state
│   │   ├── pluginStore.ts           # Plugin library and chain state
│   │   ├── presetStore.ts           # Preset list / restore metadata
│   │   └── themeStore.ts            # Dark / light theme
│   └── lib/
│       ├── tauri.ts                 # Tauri IPC wrappers
│       ├── types.ts                 # Shared TypeScript types
│       └── index.ts                 # Re-exports
│
└── src-tauri/                      # Backend (Rust)
    └── src/
        ├── lib.rs                  # App state, commands, tray setup
        ├── main.rs                 # Native entry point
        ├── bootstrap/
        │   ├── tray.rs             # Tray menu and events
        │   └── window.rs           # Window setup
        ├── commands/
        │   ├── audio.rs
        │   ├── config.rs
        │   ├── plugin.rs
        │   ├── session.rs
        │   ├── startup.rs
        │   └── system.rs
        ├── core/
        │   ├── app_events.rs
        │   ├── autosave.rs
        │   ├── error.rs
        │   ├── session.rs
        │   ├── snapshot.rs
        │   ├── threading.rs
        │   └── timing.rs
        ├── domain/
        │   ├── config.rs
        │   └── preset.rs
        └── plugins/
            ├── mod.rs
            ├── builtin/
            ├── core/
            ├── gui/
            └── processor/
```

---

## Preset & Session Management

### Preset Files

Stored as JSON in the platform app-data directory:

| OS | Location |
|---|---|
| Windows | `%LOCALAPPDATA%\ReLightHost\presets\` |
| macOS | `~/Library/Application Support/ReLightHost/presets/` |
| Linux | `~/.local/share/ReLightHost/presets/` |

<details>
<summary>Example preset JSON</summary>
<br>

```json
{
  "name": "My Chain",
  "description": "Vocal chain",
  "created_at": "2026-03-12T10:00:00Z",
  "plugin_chain": [
    {
      "plugin_id": "vst3_fabfilter_pro-c2",
      "plugin_name": "Pro-C 2",
      "plugin_vendor": "FabFilter",
      "plugin_format": "vst3",
      "bypassed": false,
      "parameters": [{ "id": 0, "name": "Threshold", "value": -18.0 }],
      "vst3_state": "<base64 binary blob>"
    }
  ]
}
```

</details>

### Auto-save

Every structural change to the plugin chain (add, remove, reorder, bypass toggle, rename) writes to an `autosave` preset. The chain is restored silently on the next launch.

### Session Restore Sequence

```
1. restore_session() is called from the frontend on mount
2. Audio config + mute + loopback are restored from session.json
3. Plugin chain is restored from the autosave preset
4. Audio stream is started
   └─ Voicemeeter ASIO waits briefly so the driver can finish startup
   └─ VST3 restore can defer the start a little longer during its safety window
5. Frontend stores sync with the restored backend state
```

### Startup Visibility

In **Application Settings**, you can choose:

- **Run on startup**: register or unregister the Windows Run key
- **Show app window on startup**:
  - ON: launch normally and show the main window
  - OFF: launch with `--start-hidden` and stay in the tray until opened

### VB-Cable / Voicemeeter

- **[VB-Audio Cable / HIFI-CABLE & ASIO-Bridge](https://vb-audio.com/Cable/index.htm)** — select as Primary Output to route to Discord, OBS, Teams, etc.
- **[Voicemeeter](https://vb-audio.com/Voicemeeter/)** — use ASIO Insert device for near-zero-latency routing

---

## System Tray

Enable **Minimize to Tray** in *Application Settings* to hide the window to the tray instead of quitting.

| Menu Item | Action |
|---|---|
| Show ReLightHost | Restore and focus the window |
| Mute Audio / Unmute Audio | Toggle output mute |
| Enable Hardware Out / Disable Hardware Out | Toggle Loopback |
| Audio Settings… | Open Audio Settings dialog |
| Application Settings… | Open App Settings dialog |
| Exit | Quit the application |

> The tray tooltip changes to **"ReLightHost (Muted)"** when audio is muted.

---

## Contributing

Contributions are welcome. Please open an issue first to discuss the change before submitting a pull request.

```bash
# TypeScript type check
pnpm tsc --noEmit

# Rust static check
cd src-tauri && cargo check
```

---

<div align="center">

*Made with ❤️ by Gyn*

</div>
