# VKey — Cấu trúc mã nguồn

> Tài liệu mô tả kiến trúc toàn bộ dự án VKey — bộ gõ tiếng Việt cho Linux X11.
>
> **Nền tảng duy nhất**: Linux X11 (xkbcommon + XInput2 + XTEST)

---

## Tổng quan

```
VKey
├── crates/
│   ├── vietnamese-core/      # Engine xử lý tiếng Việt (platform-agnostic)
│   ├── keyboard-linux/       # X11 keyboard capture + text injection
│   ├── VKey-rs/              # GUI (egui), tray icon (GTK), daemon loop
│   ├── keyboard-debug/       # Binary debug: in ra KeyEvent từ X11
│   ├── keyboard-core-debug/  # Binary debug: in ra EngineAction từ engine
│   └── core-test/            # Binary test: chạy pipeline đầy đủ từ stdin
├── config/                   # File cấu hình mặc định
├── scripts/                  # Shell scripts tiện ích
├── dev.sh / dev-linux.sh     # Chạy môi trường dev
├── build.sh / build-linux.sh # Build script release
└── Cargo.toml                # Workspace root
```

---

## Crates

### `vietnamese-core`

**Mục đích**: Engine thuần logic xử lý tiếng Việt, không phụ thuộc platform.

| File | Nội dung |
|------|----------|
| `lib.rs` | Re-export tất cả public API |
| `engine.rs` | `InputEngine` — nhận `KeyEvent`, trả về `EngineAction` |
| `composition.rs` | `Composition` — buffer âm tiết hiện tại |
| `telex.rs` | `TelexProcessor` — xử lý input theo phương thức Telex |
| `vni.rs` | `VniProcessor` — xử lý input theo phương thức VNI |
| `tone.rs` | Logic đặt dấu thanh điệu (Smart Tone / Classic Tone) |
| `word.rs` | `Word` — cấu trúc dữ liệu âm tiết tiếng Việt |
| `key.rs` | `Key`, `KeyEvent`, `KeyState`, `Modifiers` — mô hình hóa phím |
| `config.rs` | `EngineConfig` — cấu hình engine |
| `charset.rs` | Chuyển đổi Unicode ↔ TCVN3 ↔ VNI Windows |
| `unicode.rs` | Tiện ích xử lý Unicode cho tiếng Việt |

**Luồng dữ liệu chính**:
```
KeyEvent → InputEngine::process_key() → EngineAction
```

**`EngineAction`** có thể là:
- `PassThrough` — để phím đi qua bình thường
- `Consume` — nuốt phím
- `Commit(text)` — commit text khi gõ xong âm tiết
- `Replace { delete_graphemes, text }` — xóa và thay thế text

---

### `keyboard-linux`

**Mục đích**: Bắt sự kiện bàn phím từ X11 (XInput2), inject text qua XTEST.

| File | Nội dung |
|------|----------|
| `lib.rs` | Re-export: `KeyboardBackend`, `TextInjector`, `X11KeyboardBackend`, `X11TextInjector` |
| `backend.rs` | Trait `KeyboardBackend` + `KeyboardDecision` + `KeyboardError` |
| `injector.rs` | Trait `TextInjector` + `execute_engine_action()`, `decision_for()` |
| `x11.rs` | Toàn bộ implementation X11 (~1700 dòng): capture, decode, inject |
| `x11_injector.rs` | `X11TextInjector<'a>` — adapter mượn `X11KeyboardBackend` để inject |

#### Trait `KeyboardBackend`

```rust
pub trait KeyboardBackend {
    fn start(&mut self) -> Result<()>;
    fn stop(&mut self) -> Result<()>;
    fn next_event(&mut self) -> Result<KeyEvent>;
    fn decide(&mut self, decision: KeyboardDecision) -> Result<()>;
    fn is_running(&self) -> bool;
}
```

#### `X11KeyboardBackend` — cơ chế hoạt động

1. **Khởi tạo** (`new()`):
   - Kết nối XCB/X11
   - Kiểm tra XInput2 ≥ 2.0, XTEST ≥ 2.1, XKB
   - Tải keymap/state từ thiết bị bàn phím chính
   - Tìm keycode trống để injection Unicode
   - Enumerate thiết bị keyboard (lọc bỏ XTEST virtual device)

2. **Bắt phím** (`start()` → `next_event()`):
   - Passive grab tất cả keycodes interceptable
   - Lắng nghe `XinputRawKeyPress/Release` (theo dõi modifiers)
   - Lắng nghe `XinputKeyPress/Release` (intercept và quyết định)
   - Lắng nghe `XinputRawButtonPress` (clear composition khi click chuột)
   - Lắng nghe `XinputHierarchy` (hot-plug bàn phím mới)

3. **Quyết định** (`decide()`):
   - `PassThrough` → `XiAllowEvents(AsyncDevice)` + XTEST fake press gốc
   - `Consume` → `XiAllowEvents(AsyncDevice)` (không replay)

4. **Inject text** (qua `X11TextInjector`):
   - Tìm keycode trực tiếp cho ký tự (nếu có trong layout)
   - Nếu không: temporarily remap `injection_keycode` → XTEST fake
   - Backspace qua XTEST fake
   - `sync()` sau batch để đảm bảo thứ tự

5. **Xử lý Shift phức tạp**:
   - `RawModifierState` track modifier vật lý (tránh XKB latching)
   - Phát hiện "hidden shift press" bị che bởi active grab
   - `normalize_stale_shift_state()` clear latched XKB shift

---

### `VKey-rs`

**Mục đích**: Binary chính — GUI settings (egui), tray icon (GTK), daemon loop.

| File | Nội dung |
|------|----------|
| `main.rs` | Entry point, argument parsing, daemon loop, shortcut handling |
| `gui.rs` | `AppGui` (egui App), Linux GTK tray icon, UI rendering |
| `config_store.rs` | Load/save `EngineConfig` từ TOML file (XDG config dir) |

#### Kiến trúc đa luồng

```
Main Thread (egui)                Background Thread (daemon)
─────────────────                 ──────────────────────────
eframe::run_native()              run_daemon()
  AppGui::update()                  X11KeyboardBackend::next_event()
  GTK tray event loop               engine.process_key()
                                    execute_engine_action()
                                    backend.decide()

AppMessage channel  (GUI → Daemon): UpdateConfig, Exit
GuiMessage channel  (Daemon → GUI): StateChanged, ShowSettingsWindow
tray_channel        (AppGui → GTK tray): sync EngineConfig
```

#### Daemon loop

```
loop {
    1. Poll AppMessage (non-blocking): UpdateConfig? Exit?
    2. next_event() — chờ tối đa 50ms
    3. Poll AppMessage lại
    4. shortcut_state.update() — kiểm tra Ctrl+Shift / Alt+Z
    5. engine.process_key(event) → action
    6. Nếu Consume:
       - execute_engine_action() — inject trong khi device còn frozen
       - backend.decide(Consume) — unfreeze, nuốt phím
    7. Nếu PassThrough:
       - backend.decide(PassThrough) — replay phím gốc
}
```

#### GTK Tray Icon

- `libappindicator` qua GTK bindings
- `LinuxTrayControls` chứa menu items GTK
- Config sync qua `tray_channel: Sender<EngineConfig>`
- Icon vẽ dynamically thành Pixbuf (hình tròn V/E)

#### `ShortcutState`

Track modifiers vật lý từ raw events:
- `Ctrl+Shift`: trigger khi cả hai được giữ đồng thời
- `Alt+Z`: trigger khi Alt+Z/z được nhấn

---

### `keyboard-debug`

Binary debug: khởi động backend, in ra mọi `KeyEvent` nhận được.

### `keyboard-core-debug`

Binary debug: khởi động backend + engine, in ra `KeyEvent` và `EngineAction`.

### `core-test`

Binary test: pipeline đầy đủ từ stdin. Dùng để test engine không cần X11.

---

## Luồng dữ liệu tổng thể

```
Người dùng nhấn phím
        │
        ▼
X11 Server (XInput2 passive grab)
        │
        ▼
X11KeyboardBackend::next_event()
  ├── RawKeyPress  → decode_raw()   → track modifiers
  ├── KeyPress     → decode()       → KeyEvent
  ├── RawButtonPress → boundary event
  └── Hierarchy   → refresh devices
        │
        ▼ KeyEvent
InputEngine::process_key() → EngineAction
        │
        ├── [Consume] X11TextInjector::execute_engine_action()
        │     ├── delete_previous_graphemes() → XTEST Backspace×N
        │     └── insert_text()              → XTEST Unicode injection
        │
        └── backend.decide()
              ├── [PassThrough] → XiAllowEvents + XTEST replay
              └── [Consume]    → XiAllowEvents (nuốt phím)
```

---

## Cấu hình (`EngineConfig`)

| Trường | Kiểu | Ý nghĩa |
|--------|------|---------|
| `enabled` | `bool` | Bật/tắt bộ gõ |
| `input_method` | `InputMethod` | `Telex` \| `Vni` |
| `charset` | `Charset` | `Unicode` \| `Tcvn3` \| `Vni` |
| `smart_tone` | `bool` | Smart Tone (vần chuẩn hiện đại) |
| `shortcut_key` | `ShortcutKey` | `CtrlShift` \| `AltZ` |
| `startup_with_system` | `bool` | XDG autostart (~/.config/autostart/vkey.desktop) |

Config file: `~/.config/vkey/config.toml`

---

## Dependencies chính

| Crate | Mục đích |
|-------|---------|
| `x11rb` | XCB/X11, XInput2, XTEST, XKB protocol |
| `xkbcommon` | Keymap loading, key state, keysym → char |
| `xkeysym` | Keysym constants |
| `libc` | `poll()` cho non-blocking X11 event wait |
| `eframe` / `egui` | GUI framework (OpenGL/glow, X11 backend) |
| `gtk` | Tray icon (libappindicator) |
| `tracing` | Structured logging |
| `serde` / `toml` | Config serialization |
| `thiserror` | Error type derivation |

---

## Build & Development

```bash
# Chạy dev (tự động rebuild)
./dev.sh

# Debug log đầy đủ
RUST_LOG=debug cargo run -p VKey-rs

# Build release
./build.sh

# Chạy tất cả tests
cargo test --workspace

# Debug keyboard events
cargo run -p keyboard-debug

# Debug engine actions  
cargo run -p keyboard-core-debug
```
