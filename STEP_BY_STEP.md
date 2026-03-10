# 📋 ReLightHost - Step by Step Implementation Guide

## ✅ Phase 1: Foundation (COMPLETED)

- [x] Setup package.json và dependencies
- [x] Cấu hình TypeScript + Vite
- [x] Setup Tailwind CSS
- [x] Tạo cấu trúc frontend (src/, components/)
- [x] Tạo UI components cơ bản:
  - Header (với audio status, CPU, latency)
  - Sidebar (plugin library)
  - PluginChain (signal chain area)
  - Layout wrapper

**Kết quả**: Giao diện cơ bản đã sẵn sàng, có thể chạy `pnpm tauri dev` để xem.

---

## 🎯 Phase 2: Audio Engine Setup (NEXT)

### Step 1: Add Audio Dependencies

Cập nhật `src-tauri/Cargo.toml`:

```toml
[dependencies]
# ... existing dependencies ...
cpal = "0.15"                    # Cross-platform audio I/O
```

### Step 2: Create Audio Manager

Tạo `src-tauri/src/audio/mod.rs`:
- Initialize audio devices
- Start/stop audio stream
- Handle buffer processing

### Step 3: Test Audio Passthrough

Implement simple input → output passthrough để test audio engine.

**Expected Time**: 2-3 hours

---

## 🔌 Phase 3: Plugin Hosting

### Step 1: Add Plugin Host Dependencies

```toml
clack-host = "0.11"      # CLAP plugin hosting
vst3 = { version = "0.2" }  # VST3 support
```

### Step 2: Plugin Scanner

Tạo `src-tauri/src/plugins/scanner.rs`:
- Scan folders for .vst3 / .clap files
- Parse plugin metadata
- Cache plugin list

### Step 3: Plugin Instance Manager

Tạo `src-tauri/src/plugins/instance.rs`:
- Load plugin libraries
- Create plugin instances
- Manage plugin lifecycle

### Step 4: Plugin Chain

Tạo `src-tauri/src/audio/chain.rs`:
- Link plugins in series
- Process audio through chain
- Handle bypass/enable states

**Expected Time**: 1 week

---

## 🔄 Phase 4: IPC Commands (Frontend ↔ Rust)

### Tauri Commands cần implement:

```rust
// src-tauri/src/lib.rs

#[tauri::command]
async fn scan_plugins(paths: Vec<String>) -> Result<Vec<PluginInfo>, String>

#[tauri::command]
async fn load_plugin(plugin_id: String) -> Result<(), String>

#[tauri::command]
async fn remove_plugin(instance_id: String) -> Result<(), String>

#[tauri::command]
async fn start_audio() -> Result<(), String>

#[tauri::command]
async fn stop_audio() -> Result<(), String>

#[tauri::command]
async fn get_audio_status() -> Result<AudioStatus, String>

#[tauri::command]
async fn save_preset(name: String, data: PresetData) -> Result<(), String>

#[tauri::command]
async fn load_preset(name: String) -> Result<PresetData, String>
```

**Expected Time**: 2-3 days

---

## 🎨 Phase 5: UI Enhancement

### Step 1: Plugin Card Component

Tạo `src/components/PluginCard.tsx`:
- Hiển thị plugin trong chain
- Drag & drop để reorder
- Bypass button
- Settings button

### Step 2: Plugin Editor Modal

Tạo `src/components/PluginEditor.tsx`:
- Show plugin parameters
- Knobs/sliders for value control
- Preset management

### Step 3: Audio Settings Dialog

Tạo `src/components/AudioSettings.tsx`:
- Select audio device
- Buffer size configuration
- Sample rate selection

**Expected Time**: 1 week

---

## 💾 Phase 6: State Management

### Setup Zustand Stores

Tạo `src/stores/`:

```typescript
// src/stores/pluginStore.ts
interface PluginStore {
  plugins: Plugin[]
  chain: PluginInstance[]
  addToChain: (pluginId: string) => void
  removeFromChain: (instanceId: string) => void
  reorderChain: (from: number, to: number) => void
}

// src/stores/audioStore.ts
interface AudioStore {
  isPlaying: boolean
  cpuUsage: number
  latency: number
  devices: AudioDevice[]
  selectedDevice: string | null
  start: () => Promise<void>
  stop: () => Promise<void>
}
```

**Expected Time**: 1-2 days

---

## 🔊 Phase 7: Real-time Monitoring

### Step 1: Audio Meter

- Implement RMS/Peak metering trong audio thread
- Send data qua IPC (throttled)
- Animate meter bars trong UI

### Step 2: CPU/Latency Monitoring

- Track audio callback execution time
- Calculate CPU percentage
- Display trong Header

**Expected Time**: 2-3 days

---

## 💿 Phase 8: Preset System

### Step 1: Preset Serialization

```rust
// src-tauri/src/preset.rs
#[derive(Serialize, Deserialize)]
struct Preset {
    name: String,
    plugins: Vec<PluginState>,
}
```

### Step 2: Save/Load UI

- Preset browser
- Quick save/load
- Export/import từ file

**Expected Time**: 2 days

---

## 🚀 Phase 9: System Tray Integration

```rust
// src-tauri/src/tray.rs
use tauri::SystemTray;

fn create_tray() -> SystemTray {
    // Add tray icon
    // Add context menu
    // Handle tray events
}
```

**Expected Time**: 1 day

---

## 🧪 Phase 10: Testing & Polish

- [ ] Error handling everywhere
- [ ] Loading states
- [ ] Crash recovery
- [ ] Performance optimization
- [ ] Documentation
- [ ] Package for distribution

**Expected Time**: 1 week

---

## 📊 Total Estimated Time

| Phase | Time |
|-------|------|
| Phase 1: Foundation | ✅ Done |
| Phase 2: Audio Engine | 2-3 hours |
| Phase 3: Plugin Hosting | 1 week |
| Phase 4: IPC | 2-3 days |
| Phase 5: UI Enhancement | 1 week |
| Phase 6: State Management | 1-2 days |
| Phase 7: Monitoring | 2-3 days |
| Phase 8: Presets | 2 days |
| Phase 9: System Tray | 1 day |
| Phase 10: Testing | 1 week |
| **TOTAL** | **4-6 weeks** |

---

## 🎯 Immediate Next Action

```powershell
# 1. Install dependencies
pnpm install

# 2. Test chạy app
pnpm tauri dev

# 3. Kiểm tra UI hoạt động
# 4. Tiếp tục với Phase 2: Audio Engine
```

---

## 📚 Tài liệu tham khảo

- [Tauri Docs](https://tauri.app/v1/guides/)
- [CPAL Examples](https://github.com/RustAudio/cpal/tree/master/examples)
- [CLAP Plugin Hosting](https://github.com/free-audio/clap)
- [VST3 SDK](https://github.com/steinbergmedia/vst3sdk)

---

## 💡 Tips

1. **Test audio engine riêng** trước khi thêm plugin hosting
2. **Mock plugin data** cho UI development
3. **Implement error handling** ngay từ đầu
4. **Profile performance** thường xuyên
5. **Git commit** sau mỗi phase

Good luck building! 🎵
