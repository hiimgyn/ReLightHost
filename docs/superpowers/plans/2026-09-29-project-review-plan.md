# Rà soát toàn dự án ReLightHost v2.7.0 — Lỗi, tối ưu & kế hoạch sửa

> **For agentic workers:** Dùng superpowers:executing-plans hoặc superpowers:subagent-driven-development để làm từng task. Mỗi task độc lập, commit riêng. Checkbox `- [ ]` để theo dõi.

**Phạm vi đã rà:** toàn bộ `src-tauri/src` (audio engine, backend WASAPI/ASIO, plugin hosting VST3/VST2/CLAP/built-in, session/autosave/preset, commands, bootstrap) và `src` (stores, App, polling, settings). Không rà `vendor/DeepFilterNet`.

**Hiện trạng build:** `cargo clippy --all-targets` sạch, `cargo test --lib` 40/40 pass, `tsc --noEmit` sạch. Mọi lỗi dưới đây là lỗi **logic/runtime** mà compiler và test hiện tại không bắt được.

**Độ tin cậy:** ✅ = xác nhận bằng cách đọc code, lần theo đường chạy đầy đủ. ⚠️ = suy luận có cơ sở, cần kiểm chứng trên máy thật trước khi sửa.

---

## Tóm tắt theo mức độ

| # | Vấn đề | Mức | Tin cậy | Công |
|---|---|---|---|---|
| 1 | Autosave mất state VST3 khi GUI đang mở / processor bận | P0 mất dữ liệu | ✅ | S |
| 2 | Autosave chạy giữa lúc restore → ghi đè chain rỗng / state mặc định | P0 mất dữ liệu | ✅ | S |
| 3 | Thoát app không flush autosave → mất thay đổi cuối | P0 mất dữ liệu | ✅ | S |
| 4 | `PluginInstance` có thể bị drop trên audio thread | P0 crash/glitch | ✅ | S |
| 5 | Ring buffer đẩy/lấy từng sample → L/R có thể đảo kênh vĩnh viễn | P1 audio | ✅ | S |
| 6 | Chế độ bridged không kiểm soát drift → độ trễ tăng tới ~680 ms | P1 audio | ✅ | S |
| 7 | Sample rate: ASIO không bao giờ được set; plugin không re-prepare khi đổi SR/buffer; built-in im lặng pass-through ở ≠48k | P1 audio | ✅ | M |
| 8 | VST3 thiếu `IComponentHandler` + `inputParameterChanges` → chỉnh GUI không tới DSP (plugin kiểu SDK) | P1 audio | ⚠️ | L |
| 9 | VST2: lock miss phát lại buffer cũ; host callback trả 0 cho sample rate/block size | P1 audio | ✅ | S |
| 10 | WASAPI luôn thử Exclusive trước → chặn âm thanh app khác | P1 UX | ✅ | S |
| 11 | Mất thiết bị (rút USB) → stream chết im lặng, UI vẫn báo "monitoring" | P1 UX | ✅ | M |
| 12 | `safe_start_deadline` ghi nhưng không bao giờ đọc → delay Voicemeeter không có hiệu lực | P1 | ✅ | S |
| 13 | Command đồng bộ chạy trên main thread → UI đơ khi scan/restore/load | P2 perf | ⚠️ | M |
| 14 | `vst3_state` lưu dạng mảng số JSON pretty-print → file autosave phình ~8× | P2 perf | ✅ | S |
| 15 | Scanner load DLL VST2/CLAP song song, in-process | P2 ổn định | ✅ | M |
| 16 | Bản release không có file log | P2 hỗ trợ | ✅ | S |
| 17 | `minWidth 1430 × minHeight 880` → không vừa màn hình laptop | P2 UX | ✅ | S |
| 18 | Dọn dẹp: dead code, dep thừa, lock thừa, ghi file không atomic, CI | P3 | ✅ | S |

S = < 1 giờ, M = nửa ngày, L = 1–2 ngày.

**Thứ tự đề xuất:** Phase 1 (1→4) → Phase 2 (5, 6, 9, 12) → Phase 3 (7, 10, 11) → Phase 4 (13–17) → Phase 5 (18) → Phase 6 (8, cần plugin thật để test).

---

## Phase 1 — Mất dữ liệu & crash (làm ngay)

### Task 1: Autosave không bao giờ được làm mất state plugin ✅

**Vấn đề.** `PluginInstance::get_state_binary` (`plugins/core/instance.rs:272-289`) dùng `try_lock()` trên mutex processor — mutex mà audio thread giữ mỗi block. Lock miss → rơi xuống nhánh format khác → trả `Vec::new()`. Với VST3 còn tệ hơn: `Vst3Processor::get_state` (`plugins/processor/vst3.rs:579-598`) `try_lock` trên `com_access_lock`, mà GUI thread **giữ lock này suốt thời gian GUI mở** (`plugins/gui/vst3.rs:227`). `build_chain_preset_from_manager` (`core/snapshot.rs:20-25`) thấy blob rỗng → `vst3_state = None` → autosave ghi đè, **xóa state cũ**.

**Tái hiện chắc chắn:** mở GUI một VST3 → `launch_plugin` emit `"gui_open"` → 500 ms sau autosave chạy → `com_access_lock` đang bị giữ → autosave.json mất state plugin đó. Nếu app thoát/crash trước khi đóng GUI (xem Task 3) thì mất vĩnh viễn.

**Cách sửa (tối thiểu):**
- [ ] Thêm `last_state: RwLock<Option<Vec<u8>>>` vào `PluginInstance`. Cập nhật khi `get_state_binary` đọc thành công và khi `set_state_binary` được gọi.
- [ ] `get_state_binary`: dispatch theo `self.plugin_info.format` (như `process_stereo`), không rơi qua format khác. Dùng `try_lock_for(Duration::from_millis(50))` thay `try_lock()` (autosave chạy trên worker thread, chờ 1 block là chấp nhận được). Đọc thất bại → trả `last_state` đã cache.
- [ ] `set_state_binary` và `set_parameter`: cũng `try_lock_for` thay `try_lock`, để restore/đổi tham số không bị bỏ qua im lặng khi đụng audio thread.

**Kiểm chứng:** unit test cho `PluginInstance` built-in: giữ mutex processor từ thread khác trong lúc gọi `get_state_binary` → phải trả state cache, không rỗng. Thủ công: mở GUI VST3, chờ 1 s, kiểm tra `%LOCALAPPDATA%/ReLightHost/presets/autosave.json` còn `vst3_state`.

### Task 2: Chặn autosave trong lúc restore session ✅

**Vấn đề.** `emit_plugin_chain_changed` (`core/app_events.rs:17-30`) **luôn** gọi `request_plugin_chain_autosave()`, kể cả với reason `"restore_total"`/`"restore_progress"`. Trong `restore_session_impl` (`core/session.rs`), chain bị `clear()` trước, plugin chỉ được commit vào manager **sau khi cả batch load xong** (`instance.rs:748-752`). Nếu một plugin load > 500 ms (Supertone Clear ~12 s), debounce hết hạn → autosave chụp **chain rỗng** và ghi đè autosave.json. Sau khi commit, lại có một lần autosave chụp state VST3 **mặc định** (trước khi thread replay chạy — `VST3_STATE_REPLAY_DELAY` 1 s). Crash trong cửa sổ này (đúng lúc plugin mong manh nhất — lý do `crash_marker` tồn tại) = mất cả chain. Comment ở `session.rs` ("so autosave cannot capture a partially restored state") nói ý định đúng nhưng code không thực hiện.

**Cách sửa:**
- [ ] Thêm `static RESTORE_IN_PROGRESS: AtomicBool` trong `core/autosave.rs` (hoặc truyền `startup.vst3_restore_ready` vào worker). `save_autosave_snapshot` return sớm khi đang restore.
- [ ] Set `true` đầu `restore_session_impl` (sau guard StrictMode), set `false` khi: không có VST3 → cuối hàm; có VST3 → cuối thread `vst3-replay` (cả nhánh spawn thất bại).
- [ ] Sau khi hạ cờ, gọi `request_plugin_chain_autosave()` một lần để lưu state đã restore đầy đủ.

**Kiểm chứng:** test cho worker: bật cờ, gửi `ChainChanged`, đợi > debounce → file không đổi. Thủ công: restore chain có plugin chậm, theo dõi mtime autosave.json trong lúc loading.

### Task 3: Flush autosave khi thoát ✅

**Vấn đề.** `shutdown_for_exit` (`commands/system.rs:4-14`) gửi `Shutdown` cho worker; nếu worker đang trong vòng debounce thì `return` **không lưu** (`core/autosave.rs`, nhánh `Ok(AutosaveRequest::Shutdown) => return`). Luồng quit từ UI (`App.tsx:230-234`) gọi `close_plugins` → GUI đóng → emit `"gui_close"` → ngay sau đó `quit_app` → worker nhận Shutdown giữa debounce → mất. Thoát từ tray (`bootstrap/tray.rs:199`) còn không đóng GUI. Kết hợp Task 1: chỉnh plugin trong GUI rồi thoát = mất chỉnh sửa, **mọi đường thoát**.

**Cách sửa:**
- [ ] `shutdown_for_exit`: (1) đóng mọi GUI (`request_close_gui(GUI_CLOSE_TIMEOUT)` cho mỗi instance), (2) gọi `save_autosave_snapshot` **đồng bộ** (bỏ qua nếu `RESTORE_IN_PROGRESS`), (3) shutdown worker, (4) stop audio, (5) clear.
- [ ] Worker: nhận `Shutdown` giữa debounce thì `break` (không cần save — bước (2) đã lưu), giữ nguyên.
- [ ] `install_update` đã gọi `shutdown_for_exit` → hưởng luôn.

**Kiểm chứng:** thủ công: đổi tham số built-in, thoát trong < 500 ms, mở lại → giá trị còn.

### Task 4: Không bao giờ drop plugin trên audio thread ✅

**Vấn đề.** Chain dùng `ArcSwap` RCU. `remove_instance` (`instance.rs:757-790`) và `clear()` (`instance.rs:840-843`) `store` list mới rồi drop Arc của mình. Nhưng nếu audio thread đang giữ guard của list cũ (rất hay xảy ra — nó giữ suốt thời gian xử lý block), khi arc-swap trả nợ tham chiếu, **audio thread cầm tham chiếu cuối** → `PluginInstance::drop` chạy trên audio thread: ghi file `crash_marker`, `request_close_gui` (vòng sleep), `Vst3Processor::drop` (`setProcessing(0)`/`setActive(0)`) — tất cả trên thread realtime. Hậu quả: dropout, hoặc crash với plugin mong manh. Cùng file: `instance.rs:245-253` gọi `std::thread::spawn` từ audio thread khi crash_count == 3.

**Cách sửa:**
- [ ] Helper `fn wait_until_unshared(inst: &Arc<PluginInstance>)`: vòng `while Arc::strong_count(inst) > 1 && elapsed < 1s { sleep(1ms) }` (arc-swap trả hết nợ trong `store`, nên strong_count phản ánh đúng reader). Gọi trong `remove_instance` trước `drop(instance)`.
- [ ] `clear()`: `let old = self.instances.swap(Arc::new(Vec::new()))`, rồi `wait_until_unshared` cho từng phần tử của `old` trước khi drop (ngoài `modify_lock`).
- [ ] Crash-limit event: thay `thread::spawn` bằng `AtomicBool` "pending_crash_notice"; `get_crash_statuses` (được frontend poll 10 s) hoặc autosave worker phát event.

**Kiểm chứng:** test: giữ `instances.load()` guard trên thread A, gọi `remove_instance` trên thread B, thả guard sau 20 ms → assert `Drop` (dùng built-in instance + `thread::current().id()` ghi lại trong một test hook) chạy trên thread B.

---

## Phase 2 — Đúng âm thanh (sửa nhỏ, lợi lớn)

### Task 5: Ring buffer theo frame, không theo sample ✅

**Vấn đề.** Mọi leg đẩy/lấy L và R bằng hai lệnh `try_push`/`try_pop` riêng (`wasapi.rs:531-539`, `wasapi.rs:731-740`, `asio.rs:636-637`, `asio.rs:746-753`, mirror ảo `asio.rs:519-522`, `wasapi.rs:746-751`). Producer/consumer chạy song song: consumer có thể lấy được L trong khi R chưa được đẩy → R = 0 (tính underrun) và **sample R ấy thành L của frame sau → đảo kênh, kéo dài cho tới lần lệch tiếp theo**. Tương tự khi buffer đầy. Mic mono nhân đôi thì không nghe ra; nguồn stereo thì có. Phụ: `underrun_count` tăng theo sample (×2), không theo frame.

**Cách sửa:** đổi `HeapRb<f32>` → `HeapRb<[f32; 2]>` ở `manager.rs` (capacity chia 2), đổi chữ ký `HeapProd<f32>`/`HeapCons<f32>` trong `asio.rs`/`wasapi.rs`, push/pop `[l, r]`. Underrun đếm 1 lần/frame.
- [ ] `manager.rs` + 2 backend + mirror ảo.
- [ ] Test: producer/consumer trên 2 thread, 1e6 frame có L = +i, R = −i → consumer không bao giờ thấy `l != -r` (bỏ qua frame 0).

### Task 6: Giới hạn độ trễ ở chế độ bridged ✅

**Vấn đề.** Capacity bridged = `max(buffer,4096) × 8 × 2` sample (`manager.rs:390-394`) ≈ 32k frame ≈ 680 ms @48k. Không có cơ chế giữ mức đầy: capture bắt đầu trước render → backlog ban đầu thành độ trễ vĩnh viễn; clock drift giữa hai thiết bị (hoặc SR khác nhau — Task 7) làm độ trễ bò lên dần tới trần rồi rớt frame. Người dùng thấy "càng chạy lâu càng trễ".

**Cách sửa (ponytail: bỏ frame, không resample):** trong consumer (`wasapi::start_render`, `asio::start_output_only`), trước khi pop: nếu `occupied_len() > target` với `target = 3 × block`, `skip(occupied - target)` frame. Không resampling — chấp nhận một tiếng click nhỏ khi trim; resampler thích nghi chỉ khi người dùng than phiền.
- [ ] Hàm thuần `frames_to_drop(occupied, block) -> usize` + unit test.
- [ ] Gọi ở 2 consumer.

### Task 9: VST2 — pass-through khi lock miss, trả lời host callback ✅

- [ ] `processor/vst2.rs:297-315`: khi `try_lock` thất bại thì `return` sớm (để nguyên `left/right` = dry), thay vì copy `out_l/out_r` cũ (phát lại block trước = tiếng rè lặp).
- [ ] `host_callback` (`vst2.rs:85-88`): trả `audioMasterGetSampleRate` (16) và `audioMasterGetBlockSize` (17) từ hai `AtomicU32` toàn cục do `Vst2Processor::load` set (ponytail: toàn cục, đủ vì mọi plugin cùng SR/block; chuyển sang map theo `AEffect*` nếu cần). Một số plugin hỏi SR qua callback thay vì chờ `effSetSampleRate`.

### Task 12: Thực thi hoặc xóa `safe_start_deadline` ✅

**Vấn đề.** `session.rs:79/105` ghi `startup.safe_start_deadline`, không chỗ nào đọc. Frontend gọi `toggleMonitoring(true)` ngay (`App.tsx:74-80`, comment "backend will wait for its anti-crash deadline") → delay Voicemeeter 2 s/VST3 4 s không có hiệu lực; chỉ có vòng retry ASIO ở frontend (1.2/3.2/5.2 s) che lấp.

- [ ] Quyết định: nếu delay còn cần → trong `commands::audio::toggle_monitoring(true)`, nếu `now < deadline` thì `sleep(deadline - now)` rồi mới start (command này chạy trên thread đã init COM — đúng yêu cầu ASIO). Nếu không cần → xóa field + comment sai.

---

## Phase 3 — Sample rate, exclusive mode, mất thiết bị

### Task 7: Một nguồn sự thật cho sample rate / block size ✅

**Vấn đề (3 lỗi liên quan):**
1. ASIO không bao giờ được `set_sample_rate` (`asio.rs:464-467`, `722-725` chỉ **đọc** `driver.sample_rate()`), dù `asio-sys` có `can_sample_rate`/`set_sample_rate`. Config mặc định 48 kHz; nhiều interface mặc định 44.1 kHz → plugin được prepare ở 48k nhưng nhận audio 44.1k (EQ lệch ~9%, DeepFilter sai mô hình); ở chế độ ASIO+WASAPI thì WASAPI chạy 48k, ASIO 44.1k → tràn/cạn ring buffer liên tục.
2. `set_sample_rate`/`set_buffer_size` (`manager.rs:787-816`) chỉ restart stream; plugin đã load giữ nguyên SR/`maxSamplesPerBlock` lúc load (`instance.rs:61`, `commands/plugin.rs` `load_plugin`).
3. `NoiseSuppressor`/`DeepFilterProcessor` trả `None` ở ≠ 48k (`builtin/mod.rs` `create_builtin`) → instance "đã load" nhưng không làm gì, UI không báo.

**Cách sửa:**
- [ ] ASIO: sau `load_driver`, nếu `can_sample_rate(config)` thì `set_sample_rate(config)`, không thì log + dùng rate của driver. Trả rate thực tế ra `AudioManager` (thêm vào `AudioStatus.sample_rate`).
- [ ] Sau khi `toggle_monitoring(true)` thành công, nếu SR thực tế hoặc block size khác lúc load plugin → **reload chain**: snapshot (`build_chain_preset_from_manager`) → clear → load lại với SR mới → set state. Tái dùng đúng code path restore (tách phần load+replay của `restore_session_impl` ra hàm riêng). Ponytail: reload thay vì gọi `setupProcessing` lại cho từng format — một đường code, đúng cho cả 4 format.
- [ ] Built-in ≠ 48k: `PluginInstance::new` trả `Err` rõ ràng ("cần 48 kHz") thay vì instance rỗng, để frontend hiện lỗi.

### Task 10: WASAPI Exclusive thành tùy chọn, mặc định Shared ✅ (cần chủ dự án quyết định mặc định)

**Vấn đề.** `initialize_client` (`wasapi.rs:239-291`) luôn thử `AUDCLNT_SHAREMODE_EXCLUSIVE` trước. Thành công trên loa/tai nghe = **mọi app khác (Discord, trình duyệt, game) mất tiếng**; trên mic = app khác không mở được mic. Với app route mic → virtual cable, đây gần như luôn là hành vi không mong muốn.

- [ ] Thêm `wasapi_exclusive: bool` (mặc định `false`) vào `AppConfig` theo đúng pattern `parallel_vst3_loading` (config field + 2 command + toggle trong AppSettings + i18n en/vi).
- [ ] `initialize_client(.., exclusive)`: `false` → đi thẳng `fall_back_to_shared` với `fallback_reason: None`.

### Task 11: Phát hiện mất thiết bị ✅

**Vấn đề.** Capture thread thoát im lặng khi lỗi (`wasapi.rs:548-551`); render thread log một lần rồi quay vòng vô ích (`wasapi.rs:713-721`). `AudioStatus.is_monitoring` vẫn `true`; không event nào tới frontend; không tự khôi phục.

- [ ] Thêm `stream_failed: Arc<AtomicBool>` vào `MixerState`/struct stream; thread set cờ khi gặp lỗi thiết bị rồi thoát.
- [ ] `get_status` (frontend poll 4 s khi đang monitoring) thấy cờ → set `is_monitoring=false`, gắn `wasapi_fallback_reason`/trường lỗi mới; frontend hiện thông báo + nút "Khởi động lại audio". Ponytail: không làm `IMMNotificationClient` hot-plug tự động; thêm khi người dùng cần auto-reconnect.

---

## Phase 4 — Hiệu năng & UX

### Task 13: Không chặn main thread ⚠️

**Vấn đề.** Mọi `#[tauri::command]` trừ 2 command update đều là `fn` đồng bộ; Tauri 2 chạy command đồng bộ trên **main thread** (chính comment ở `instance.rs:631-636` xác nhận giả định này). Các command dài: `scan_plugins` (quét đĩa + load DLL), `restore_session` (có thể hàng chục giây), `load_plugin` (VST3 `initialize()`), `launch_plugins` (chờ tới 10 s ở `wait_for_vst3_restore_ready`), `close_plugins` (tới 3 s/plugin). Trong lúc đó cửa sổ không vẽ/không nhận input, và event `restore_progress` có thể không tới webview được cho tới khi restore xong (làm vô nghĩa thanh tiến độ).

**Kiểm chứng trước:** chạy restore với plugin chậm, xem thanh "Preparing plugins N/total" có tăng dần không, và cửa sổ có kéo được không.

**Cách sửa (chọn lọc, vì có ràng buộc COM/JUCE):**
- [ ] `scan_plugins`, `close_plugins`, `get_system_stats`, `list_audio_devices` → `#[tauri::command(async)]` (không đụng thread-affinity của plugin đang chạy). Lưu ý `list_audio_devices` load driver ASIO → giữ trên thread đã init COM: bọc bằng `ensure_com_initialized()` ở đầu.
- [ ] `restore_session`, `load_plugin`, `launch_plugins`: giữ đồng bộ **cho tới khi** có dedicated "plugin host thread" (COM STA + message pump) — việc lớn, ghi thành issue riêng, không làm trong plan này.

### Task 14: Lưu state nhị phân gọn ✅

**Vấn đề.** `PresetPlugin.vst3_state: Option<Vec<u8>>` (`domain/preset.rs`) serialize thành mảng số JSON, lại qua `to_string_pretty` → mỗi byte một dòng, ~6–8 byte text/byte. State 1 MB → autosave.json ~8 MB, bị serialize **hai lần** mỗi lần autosave (`preset_hash_bytes` + `save_to_file`).

- [ ] Serialize `vst3_state` dạng base64 (hàm `serialize_with` nhỏ, tự viết ~20 dòng hoặc crate `base64` nếu chấp nhận thêm dep). `deserialize_with` chấp nhận **cả** chuỗi base64 lẫn mảng số cũ (tương thích ngược file hiện có).
- [ ] `save_to_file`: `to_string` thay `to_string_pretty`. Hash trên chính bytes đã serialize (serialize một lần, dùng cho cả hash và ghi file).
- [ ] Test round-trip: đọc JSON dạng mảng cũ → ghi base64 → đọc lại bằng nhau.

### Task 15: Scanner an toàn hơn ✅

**Vấn đề.** `scan_directory` (`scanner.rs:293`) dùng `into_par_iter`, và với VST2 (`read_vst2_metadata` — chạy `VSTPluginMain`), CLAP (`read_clap_metadata`), VST3 fallback (`read_vst3_dll_info_win`) thì **thực thi code plugin song song trong process chính**. `catch_unwind` (`scanner.rs:458`) chỉ bắt panic Rust, không bắt access violation → một DLL hỏng làm sập app lúc quét.

- [ ] Bước rẻ: chỉ song song phần đọc đĩa/metadata tĩnh (moduleinfo.json, VERSIONINFO); các bước load DLL chạy **tuần tự** (gom path cần load → loop).
- [ ] Bước đúng (ghi issue, làm sau): quét out-of-process — spawn chính exe với `--scan-one <path>`, đọc JSON từ stdout, timeout 10 s; crash của plugin chỉ giết process con.

### Task 16: Log file ở bản release ✅

**Vấn đề.** `bootstrap/mod.rs:7` chỉ cài `tauri_plugin_log` khi `debug_assertions`. Bản phát hành không có log → người dùng báo crash plugin không có gì để gửi, trong khi đây là app hay crash do plugin bên thứ ba.

- [ ] Cài plugin log ở mọi build: release = `LevelFilter::Info`, target `LogDir` (file) + giới hạn kích thước (`max_file_size`, `RotationStrategy::KeepOne`). Debug giữ như cũ.

### Task 17: Kích thước cửa sổ tối thiểu ✅

**Vấn đề.** `tauri.conf.json` `minWidth 1430 / minHeight 880` + `window.rs:8-9` ép `max(MIN_W)`. Màn 1366×768, hoặc 1920×1080 ở scale 150% (logic 1280×720), cửa sổ tràn màn hình, không thu nhỏ được.

- [ ] Hạ min xuống mức layout thực sự chịu được (đo bằng devtools, ví dụ ~1100×700), và trong `window.rs` kẹp kích thước theo `min(monitor * 0.9)` trước khi `max(MIN)`.

### (Tùy chọn) Task 17b: Một command áp dụng cấu hình audio

`AudioSettings.handleApply` gọi tới 7 setter nối tiếp (mỗi cái ghi `session.json`). Chỉ làm nếu thấy chậm: thêm `apply_audio_config(config: AudioConfig)` ghi một lần. Ponytail: hiện chấp nhận được vì monitoring đã được tắt trước.

---

## Phase 5 — Dọn dẹp (P3)

- [ ] Xóa `core/error.rs` (`AppError` không được dùng ở đâu) + dep `thiserror`.
- [ ] Xóa `esbuild` khỏi `dependencies` trong `package.json` (không import ở đâu; Vite tự mang).
- [ ] `domain/preset.rs`: xóa module `chrono` giả (trả epoch-seconds dưới tên `to_rfc3339`), dùng crate `chrono` thật đã có trong `Cargo.toml` (`chrono::Local::now().to_rfc3339()` — cần bật feature `clock`, đã bật).
- [ ] `instance.rs:894-907`: module `uuid` giả → đổi tên thành `next_instance_id()` cho khỏi gây hiểu nhầm (ID theo counter là đủ).
- [ ] `lib.rs`: `audio_manager`, `plugin_scanner`, `preset_manager`, `config_manager` bọc `RwLock` nhưng **không nơi nào** lấy `.write()` — bỏ lớp `RwLock` (các type đã có interior mutability). Diff lớn nhưng cơ học; làm cuối.
- [ ] `domain/config.rs` `save_config`/`save_session`: dùng cùng kiểu ghi tmp + rename như `Preset::save_to_file`.
- [ ] `commands/system.rs` `open_external_url`: chỉ chấp nhận `https://`.
- [ ] `App.tsx`: chuỗi cứng "Install & Restart", "Session restored — …" → i18n; bỏ `localStorage.setItem("audioConfigured")` còn sót ở `AudioSettings.tsx:188`.
- [ ] `crash_marker`: set theo path nên 2 instance cùng plugin, xóa 1 là mất dấu cái còn lại → đổi sang đếm (`HashMap<String, u32>`).
- [ ] Gỡ `gsap`: chỉ dùng ở `PluginCard.tsx:196-214` cho 2 tween (fade-in khi mount, mờ khi bypass) — thay bằng `@keyframes` + `transition` trong `index.css` (class `rh-card-enter`/`rh-card-wrap`, `data-bypassed`), thêm `@media (prefers-reduced-motion: reduce)`, xóa `gsap` khỏi `package.json`.
- [ ] CI: thêm workflow `check.yml` chạy trên PR/push: `cargo clippy -- -D warnings`, `cargo test --lib`, `pnpm tsc --noEmit` (runner Windows). Hiện chỉ có workflow release.

---

## Phase 6 — VST3 parameter flow ⚠️ (cần plugin thật)

### Task 8: `IComponentHandler` + `inputParameterChanges`

**Vấn đề.** Host không gọi `controller.setComponentHandler(...)` (không có trong `gui/vst3.rs`) và luôn truyền `inputParameterChanges: null` (`processor/vst3.rs:553`). Theo spec VST3, plugin tách controller/processor (kiểu Steinberg SDK, iPlug2) chỉ báo thay đổi từ GUI qua `IComponentHandler::performEdit`, và processor chỉ nhận qua `IParameterChanges` trong `process()`. Hậu quả: xoay knob trong GUI của những plugin này **không đổi âm thanh**, và `getState` của component không phản ánh thay đổi. JUCE thường không bị (chia sẻ `AudioProcessor` qua `IConnectionPoint`), nên lỗi có thể chưa ai gặp.

**Kiểm chứng trước:** thử với plugin mẫu Steinberg (`again.vst3` từ VST3 SDK) — xoay Gain trong GUI, nghe có đổi không.

**Cách sửa:**
- [ ] `HostComponentHandler` (`ComWrapper`, như `HostPlugFrame` sẵn có): `performEdit(id, value)` đẩy `(id, value)` vào SPSC queue lock-free của processor; `beginEdit/endEdit` no-op; `restartComponent` log. Set qua `setComponentHandler` khi load (không phải khi mở GUI).
- [ ] `process_chunk`: drain queue → dựng `IParameterChanges`/`IParamValueQueue` tối thiểu (1 điểm/param/block, offset 0) → gán `inputParameterChanges`.
- [ ] `performEdit` cũng gọi `emit_plugin_chain_changed("parameter_update")` (qua cờ atomic, không phát từ thread plugin) để autosave bắt được chỉnh sửa GUI.

---

## Những gì đã kiểm và **ổn** (không cần sửa)

- Vòng đời ASIO: `ASIO_LIFECYCLE_LOCK`, `AsioGuard` chống panic, thứ tự stop/remove/dispose — cẩn thận, đúng.
- COM setup WASAPI trên chính thread realtime; retry `AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED` đúng protocol Microsoft.
- Chunking theo `maxSamplesPerBlock` ở cả VST3/VST2/CLAP — callback lớn hơn block đã prepare được xử lý đúng.
- `process_chain_stereo` lock-free (ArcSwap), `try_lock` ở mọi đường audio — không block audio thread (ngoại trừ Task 4).
- Preset ghi atomic (tmp + rename). Polling frontend chỉ chạy khi cửa sổ hiển thị (`useVisibleInterval`), có chống request chồng (`inFlight`).
- Clippy/tsc sạch, test pass.

---

## Vòng 2 (2026-09-29) — rà các phần chưa đọc kỹ ở vòng 1

### Phase 7 — Lỗi còn lại

- [ ] **7.1 CLAP `init`/`deinit` không cân bằng ✅ (S).** `ClapProcessor::load` gọi `clap_entry.init` mỗi instance và `Drop` gọi `deinit` mỗi instance (`processor/clap.rs`); `read_clap_metadata` (scanner) cũng init/deinit. Hai instance cùng một plugin CLAP, hoặc quét plugin khi plugin đó đang trong chain → `deinit` gỡ trạng thái toàn cục của instance còn sống → crash với plugin không tự đếm tham chiếu (CLAP < 1.2). Sửa: đếm theo path phía host, chỉ `init` lần đầu và `deinit` lần cuối (scanner dùng chung bộ đếm).
- [ ] **7.2 CLAP xử lý in-place không kiểm tra ✅ (S).** `process_chunk` trỏ input và output vào cùng buffer; CLAP chỉ cho phép khi plugin khai báo `in_place_pair`. Sửa: copy input sang scratch như VST2/VST3.
- [ ] **7.3 CLAP `start_processing` gọi sai thread ✅ (S).** Spec: `[audio-thread]`; đang gọi trong `load`. Sửa: gọi lần đầu trong `process_stereo`.
- [ ] **7.4 CLAP `request_callback` bị bỏ qua ✅ (M).** `on_main_thread` không bao giờ được gọi → plugin dựa vào cơ chế này (nih-plug, clap-wrapper) không đồng bộ tham số/GUI. Sửa: cờ trong host → `AppHandle::run_on_main_thread(on_main_thread)`.
- [ ] **7.5 DeepFilter: độ trễ tăng vĩnh viễn sau mỗi lần worker chậm ✅ (S–M).** Khi worker trễ, audio thread phát im lặng nhưng không bỏ phần đầu ra đến muộn → mỗi lần như vậy cộng thêm độ trễ, tối đa 1 s (`builtin/deep_filter.rs`). Sửa: gộp 4 ring thành một ring frame `[f32; 4]` (clean L/R + dry L/R) và dùng `BacklogTrimmer` (tổng quát hóa kiểu phần tử).
- [ ] **7.6 ASIO bỏ qua reset request ✅ (S–M).** Đổi buffer size / sample rate trong control panel của driver → driver gửi `kAsioResetRequest` / `SampleRateChanged`; app không đăng ký `Driver::add_event_callback` nên âm thanh hỏng tới khi restart tay. Sửa: callback set cờ; `get_status` (UI poll) restart monitoring như Task 11.
- [ ] **7.7 Ba minor còn lại từ vòng 1:** scan async chồng với load plugin; cờ restore kẹt nếu thread replay panic (drop guard); reload chain thừa khi đổi thiết bị lúc không monitoring.
- [ ] **7.8 Dọn:** `src/stores/presetStore.ts` là file rỗng; canvas gọi `getBoundingClientRect` mỗi frame (dùng `ResizeObserver`); worker DeepFilter poll `sleep(1ms)`.

### Phase 8 — Nâng cấp (tính năng)

- [ ] **8.1 Preset có tên (M):** lưu / tải / xóa chain theo tên. Backend đã có `PresetManager` + đường load của restore; thiếu command + UI.
- [ ] **8.2 Waveform thật trước/sau xử lý (M):** `WaveformDualCanvas` đang vẽ sóng sin giả lập. Thêm ring peak pre/post mỗi block cho từng instance, UI poll như VAD.
- [ ] **8.3 Hiển thị tổng latency của chain (S–M):** VST3 `getLatencySamples`, CLAP ext `latency`, VST2 `initial_delay`, DeepFilter hop — cộng vào `AudioStatus`.
- [ ] **8.4 Bundle nhiều plugin (M):** CLAP/VST3 chỉ lấy plugin đầu tiên trong file; liệt kê và nạp theo index.
- [ ] **8.5 Tự nối lại thiết bị WASAPI khi cắm lại (M):** `IMMNotificationClient`, dựa trên cờ lỗi của Task 11.
- [ ] **8.6 Quét plugin out-of-process (L)** và **8.7 thread host plugin riêng (COM STA + message pump) để restore/load không chặn main thread (L).**

---

## Vòng 3 (2026-09-30) — rà lại sau Phase 7–8

Đã làm xong toàn bộ Phase 7–8 (kể cả 8.7 thread host plugin). Rà lại chính các thay đổi đó và phần code chưa đọc kỹ; đã sửa (mỗi mục một commit, có test khi kiểm chứng được):

- [x] Thread host: vòng chờ (`request_close_gui`, chờ replay VST3) vẫn bơm message — tránh treo khi đóng editor JUCE; reorder/swap chạy trên thread host.
- [x] VST2 GUI: gửi `effEditIdle` định kỳ; `effEditClose` trước khi hủy cửa sổ host.
- [x] CLAP GUI: chạy trên thread host (`gui.*` là `[main-thread]`), `hide`/`destroy` trước khi hủy cửa sổ, `set_scale` theo DPI; mở editor không giữ mutex processor (tránh audio dry).
- [x] i18n các chuỗi còn viết cứng; tiêu đề canvas mô tả đúng dữ liệu thật.
- [x] Tray: trạng thái Monitor Output đồng bộ và được lưu session ở mọi đường.
- [x] UI giữ trạng thái "đang chuẩn bị" tới khi restore (giờ async) xong; gộp các lần gọi `get_audio_status` chồng nhau.
- [x] VU meter không phụ thuộc buffer size.
- [x] NoiseSuppressor: trễ cố định đúng 1 khung (480 mẫu) với mọi buffer size — trước đây đan xen tín hiệu thô/đã xử lý khi buffer không chia hết 480.
- [x] BacklogTrimmer: thêm biên jitter + trễ cắt (hysteresis).
- [x] Scanner: không đi theo symlink/junction trỏ ngược lên thư mục cha (trùng plugin / tràn stack).

Vòng 4: kiểm chứng tổng (clippy `-D warnings`, 100/100 test, tsc, `pnpm build`, `cargo build`, `--scan-one`) — sạch.

Còn cần thử trên máy thật (không tự động hoá được): JUCE/iPlug2 VST3, CLAP (nih-plug), VST2 editor; ASIO + Voicemeeter trên thread host; rút/cắm lại USB; đổi buffer trong control panel ASIO; preset; waveform; restore với plugin chậm.
