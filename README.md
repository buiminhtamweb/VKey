# Vkey-rs

Vkey-rs is a native Vietnamese input method written in Rust. It does not use
Fcitx5, IBus, Electron, an external keyboard command, or the clipboard as its
primary input path.

The repository currently completes:

- **Phase 1:** platform-independent Vietnamese core and test CLI.
- **Phase 2:** native X11 keyboard observation and debug integration.

Text injection, daemon IPC, egui settings, packaging, and Wayland support are
not implemented yet.

## Architecture

```text
                    X11 Server
                        |
                        | XI_RawKeyPress / XI_RawKeyRelease
                        v
              +-----------------------+
              | X11KeyboardBackend    |
              +-----------+-----------+
                          |
                          v
                     KeyEvent
                          |
                          v
              +-----------------------+
              | Vietnamese Core       |
              +-----------+-----------+
                          |
                          v
                    EngineAction
                          |
                          v
                 Phase 3 Injector
                    (not present)
```

`vietnamese-core` remains independent of X11, Linux, devices, and GUI types.
`keyboard-linux` converts X11 events into the shared `KeyEvent`; it contains no
Vietnamese spelling rules.

## X11 architecture decision

Phase 2 selects `XI_RawKeyPress` and `XI_RawKeyRelease` from XInput2 on the
screen's root window. It does not use `XGrabKeyboard`:

- Raw events are global and independent of the focused client.
- Selection is observational and never freezes, consumes, or reroutes input.
- The blocking `wait_for_event()` path has no busy loop and needs no event
  thread.
- Closing the X11 connection automatically removes the subscription. `stop()`
  also clears the event mask while the connection is healthy.

XRecord was not selected because XInput2 provides the required device events
directly without a second recording connection. Commands such as `xinput`,
`xev`, and `xdotool` are never executed.

Raw XInput2 events contain keycodes, not final text. `libxkbcommon-x11` loads
the X server's active XKB keymap and state, resolves layout/group/Shift/Caps
Lock, then produces the keysym used by the platform-neutral decoder. Keycodes
are never hard-coded as ASCII.

Important: raw events are observation-only. The Phase 2
`KeyboardDecision::{PassThrough, Consume}` mapping describes the decision
boundary, but is not applied to the X server. Phase 3 will require a separate,
carefully scoped interception/injection mechanism; this backend does not
pretend raw event selection can consume keys.

References: [XInput2 selection](https://xorg.freedesktop.org/archive/X11R7.5/doc/man/man3/XISelectEvents.3.html),
[XInput protocol](https://www.x.org/docs/Xi/proto.pdf),
[libxkbcommon X11 support](https://xkbcommon.org/doc/current/group__x11.html),
[libxkbcommon keyboard state](https://xkbcommon.org/doc/current/group__state.html).

## Crates and licenses

Runtime dependencies added for Phase 2:

- `x11rb` 0.14 — X11/XInput2 protocol and XCB connection; MIT/Apache-2.0.
- `xkbcommon` 0.9 — safe wrappers around system `libxkbcommon`; MIT.
- `xkeysym` 0.2 — keysym constants and Unicode conversion;
  MIT/Apache-2.0/Zlib.
- `thiserror` — typed backend errors; MIT/Apache-2.0.
- `tracing` — structured logging; MIT.
- `tracing-subscriber` — debug binary logging; MIT.

The `x11rb` and `xkbcommon` dependencies are Linux-target-only. The repository
contains no project-authored `unsafe`; the audited FFI crates encapsulate the
native XCB/XKB calls.

## Requirements

General:

- Rust stable 1.85 or newer
- Cargo

Ubuntu/Debian packages for the Linux X11 binaries:

```bash
sudo apt install build-essential pkg-config libxcb1-dev \
  libxkbcommon-dev libxkbcommon-x11-dev
```

At runtime, `$DISPLAY` must point to a reachable X server that supports
XInput2 2.0 or newer and XKB. Root access is neither needed nor supported.

## Build

```bash
cargo build --workspace
cargo build --workspace --release
```

Phase 2 release binaries are:

```text
openkey-core-test
keyboard-debug
keyboard-core-debug
```

## Run

Core-only Telex and VNI:

```bash
cargo run -p openkey-core-test -- "tieengs Vieejt"
# tiếng Việt

cargo run -p openkey-core-test -- --vni "tie6ng1 Vie65t"
# tiếng Việt
```

Raw X11 decoder:

```bash
cargo run -p keyboard-debug
```

Example output:

```text
X11 keyboard backend started.

Press keys (Escape exits)...
KeyPress Character('a') modifiers=[]
KeyRelease Character('a') modifiers=[]
KeyPress Character('A') modifiers=[SHIFT]
KeyRelease Character('A') modifiers=[SHIFT]
```

X11 to Vietnamese engine, with actions printed but never injected:

```bash
cargo run -p keyboard-core-debug
```

Press Escape to stop either debug binary cleanly. Use `RUST_LOG=debug` to see
per-key tracing; key content is never logged at `info`.

## Key events and modifiers

The shared event model distinguishes `Press` and `Release` and supports:

- Unicode `Character(char)` without assuming a US keycode layout.
- Backspace, Enter, Escape, Tab, Space, Delete, and Insert.
- arrows, Home, End, PageUp, and PageDown.
- Shift, Control, Alt, Super, CapsLock, NumLock, and F1–F12.
- Shift, Ctrl, Alt, Super, Caps Lock, and Num Lock modifier state.

`Ctrl+A` remains `Character('a')` plus `ctrl = true`; it is not converted into
a control character.

## Auto-repeat

The backend relies entirely on X11 repeat. It creates no repeat timer. On
XInput 2.2, `KeyRepeat` raw press events are emitted as additional `Press`
events and are not applied twice to the local XKB state. With an older XInput2
server, its native release/press sequence is forwarded as received. In either
case the event loop remains blocking and does not poll.

## Configuration

The future daemon will load `$XDG_CONFIG_HOME/openkey-rs/config.toml`, falling
back to `~/.config/openkey-rs/config.toml`. Defaults are in
`config/default.toml`. Phase 2 performs no config file writes.

## Telex

Implemented shapes: `dd`, `aa`, `aw`, `ee`, `oo`, `ow`, `uw`, including
uppercase. Tone keys are `s`, `f`, `r`, `x`, and `j`.

## VNI

Implemented tones are `1`–`5`; shape keys are `6` (â/ê/ô), `7` (ơ/ư), `8`
(ă), and `9` (đ).

## Tests and validation

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --release
```

The keysym decoder is unit-tested independently of a display. A real X11
server is still required for the manual capture tests below.

## Manual X11 checklist

Run `cargo run -p keyboard-debug`, then verify:

- Basic: `a b c 1 2 3`
- Shift: `A B C ! @ #`
- Ctrl: Ctrl+A/C/V/X/Z
- Alt: Alt+A and Alt+Tab
- Super: Super+A
- Special: Backspace, Enter, Tab, Space, Delete, Insert, arrows, Home, End,
  PageUp, PageDown, F1–F12
- Locks: Caps Lock + `a`, Num Lock + keypad keys
- Repeat: hold `a`; events continue without increased idle CPU

Run `cargo run -p keyboard-core-debug` and type `tieengs`. The final printed
core state must be `tiếng`; the focused application will still receive the
original keys because Phase 2 does not consume or inject.

## Error handling and troubleshooting

- `DISPLAY is not set`: run inside an X11 session and check `$DISPLAY`.
- `failed to connect to X11 display`: verify X authority and that the display
  is reachable.
- `XInput2 is unavailable`: use an X server with XInput2 2.0 or newer.
- On Wayland, an XWayland `$DISPLAY` only observes the X11/XWayland scope and
  is not a global Wayland backend.

Runtime paths return typed errors instead of panicking. Losing the X11
connection marks the backend stopped and exits the debug program cleanly.

## Wayland limitations

Wayland is not implemented in Phase 2. A normal Wayland client cannot globally
observe and rewrite other clients' keyboard input. `libinput` is intended for
compositors rather than ordinary applications, and compositor-controlled
input-method/virtual-keyboard protocols are not universally available.

References: [Wayland protocol model](https://wayland.freedesktop.org/docs/book/Protocol.html),
[libinput documentation](https://wayland.freedesktop.org/libinput/doc/latest/).

## Install, autostart, and uninstall

Packaging and autostart are Phase 6 work. Phase 2 installs no system files.
Use Cargo to run the binaries; remove copied binaries and use `cargo clean` to
delete build artifacts.

## Roadmap

1. Vietnamese core and CLI — complete.
2. X11 keyboard backend and debug integration — complete.
3. X11 text injection/interception.
4. Daemon and Unix-domain-socket IPC.
5. Native egui settings application.
6. systemd user autostart and Debian-first packaging.
7. Wayland capability research/backend with explicit limitations.
