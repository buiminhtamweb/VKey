# Vkey-rs

Vkey-rs is being built as a small, native Vietnamese input method in Rust. It
does not use Fcitx5, IBus, Electron, or a clipboard-based primary input path.

This repository currently completes **Phase 1 only**: the platform-independent
Vietnamese engine and the `openkey-core-test` CLI. It does not intercept or
inject keyboard input yet.

## Architecture

Phase 1 keeps all platform concerns outside `vietnamese-core`:

```text
KeyEvent -> composition buffer -> Telex/VNI parser
                              -> tone placement -> Unicode NFC
                              -> EngineAction diff
```

The public engine returns `PassThrough`, `Consume`, `Commit`, or a grapheme-safe
`Replace { backspaces, text }`. Linux backends can therefore be added without
putting X11, Wayland, device, or GUI types into the core.

Current crates:

- `vietnamese-core`: engine, Telex/VNI, smart tone, NFC, restore, backspace.
- `openkey-core-test`: minimal stdin/argument CLI for exercising the engine.

Dependencies in Phase 1 are deliberately small: `serde` for stable config
types, `unicode-normalization` for NFC, `unicode-segmentation` for safe
backspace/diffs, and the dev-only `criterion` benchmark harness.

For Phase 2, the intended X11 implementation uses Rust `x11rb` bindings with
the X Input and XTEST extensions. It will only be introduced in the Linux
backend/injector crates.

## Requirements

- Rust stable 1.85 or newer
- Cargo

The core builds on any platform supported by Rust. A Linux graphical session is
not required for Phase 1.

## Build

```bash
cargo build --workspace
cargo build --release --workspace
```

The Phase 1 release binary is `openkey-core-test`. The final
`openkey-rs` and `openkey-rs-settings` binaries belong to later phases.

## Run

Telex from an argument:

```bash
cargo run -p openkey-core-test -- "tieengs Vieejt"
# tiếng Việt
```

VNI:

```bash
cargo run -p openkey-core-test -- --vni "tie6ng1 Vie65t"
# tiếng Việt
```

With no argument, the CLI reads standard input.

## Tests and benchmark

```bash
cargo test --workspace
cargo bench -p vietnamese-core --bench process_key
```

Tests cover the required Telex/VNI forms, uppercase handling, smart tone,
runtime method switching, restore typing, grapheme-safe backspace, and NFC.

## Configuration

The future daemon will load `$XDG_CONFIG_HOME/openkey-rs/config.toml` (falling
back to `~/.config/openkey-rs/config.toml`). The planned defaults are in
`config/default.toml`. Phase 1 consumes `EngineConfig` directly and performs no
filesystem access.

`restore_key = "z"` restores the current transformed composition to its exact
raw keystrokes. For example, `tieengsz` displays `tieengs`.

## Telex

Implemented shapes: `dd`, `aa`, `aw`, `ee`, `oo`, `ow`, `uw` and their
uppercase variants. Tones are `s`, `f`, `r`, `x`, `j`. Tone keys may arrive
before the final consonant, as in `Tieesng -> Tiếng`.

## VNI

Implemented tones are `1` through `5`; shapes are `6` (â/ê/ô), `7` (ơ/ư),
`8` (ă), and `9` (đ).

## X11 support

**Status: not implemented until Phase 2/3.** X11 can support the intended
design: X Input provides event selection/grabbing and XTEST can synthesize core
key events. The backend must explicitly preserve Ctrl/Alt/Super shortcuts and
tag or otherwise suppress self-injected events.

References: [X Input extension](https://www.x.org/releases/current/doc/libXi/inputlib.pdf),
[XTEST protocol](https://www.x.org/releases/current/doc/xextproto/xtest.pdf).

## Wayland limitations

**Status: unsupported in Phase 1 and not claimed as globally functional.** A
normal Wayland client receives keyboard events for its focused surface; it
cannot globally observe and rewrite other clients' input. `libinput` is an input
stack for compositors, not a normal application API. The unstable input-method
and virtual-keyboard protocols are compositor-controlled and are not guaranteed
to be advertised or authorized.

Consequently, Phase 7 must expose capability detection and an explicit
limited/unsupported result when the compositor provides no suitable trusted
protocol. Reading `/dev/input` directly is not treated as a transparent
solution because it changes device-permission and security requirements.

References: [Wayland protocol model](https://wayland.freedesktop.org/docs/book/Protocol.html),
[libinput documentation](https://wayland.freedesktop.org/libinput/doc/latest/),
[input-method protocol](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/unstable/input-method/input-method-unstable-v2.xml),
[virtual-keyboard protocol](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/unstable/virtual-keyboard/virtual-keyboard-unstable-v1.xml).

## Install

Packaging and installation are Phase 6 work. During Phase 1, run through Cargo
or copy the release `openkey-core-test` binary for local testing.

## Autostart

Not applicable to the Phase 1 CLI. A systemd user service is planned for the
daemon; the settings GUI will not need to autostart.

## Troubleshooting

- If output is unchanged, confirm the desired method (`--vni` is required for
  VNI in the CLI).
- Run `cargo test --workspace` before reporting parser regressions.
- The CLI is a core demonstration, not a global keyboard hook.

## Uninstall

No system files are installed in Phase 1. Remove any copied test binary and run
`cargo clean` if build artifacts are no longer needed.

## Roadmap

1. Vietnamese core and CLI — complete.
2. X11 keyboard backend.
3. X11 text injection.
4. Daemon and Unix-domain-socket IPC.
5. Native egui settings application.
6. systemd user autostart and Debian-first packaging.
7. Wayland capability research/backend with limitations reported honestly.
