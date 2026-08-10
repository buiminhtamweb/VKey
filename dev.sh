#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
cd "$SCRIPT_DIR"

info() { printf '\033[0;34m[dev]\033[0m %s\n' "$*"; }
warn() { printf '\033[0;33m[warn]\033[0m %s\n' "$*"; }
die() { printf '\033[0;31m[error]\033[0m %s\n' "$*" >&2; exit 1; }

usage() {
    cat <<'EOF'
Usage: bash dev.sh [COMMAND] [ARGS...]

Commands:
  run [args]       Run the GUI + keyboard service (default: --debug-input)
  headless [args]  Run only the keyboard service
  core [args]      Run VKey-core-test, e.g. bash dev.sh core "tieengs Vieejt"
  kbd-debug        Run the platform keyboard-event diagnostic
  core-debug       Run keyboard -> Vietnamese core diagnostics
  test [args]      Run workspace tests
  check            Run fmt, check, and Clippy
  all              Run the complete local validation suite and debug build
  package [args]   Delegate to build.sh, e.g. bash dev.sh package --quick
  help             Show this help
EOF
}

command -v cargo >/dev/null 2>&1 || die "Cargo is not available in PATH"

COMMAND="${1:-run}"
if (($#)); then shift; fi

case "$COMMAND" in
    run)
        args=("$@")
        ((${#args[@]})) || args=(--debug-input)
        export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"
        export RUST_LOG="${RUST_LOG:-debug}"
        info "Starting VKey-rs ${args[*]}"
        exec cargo run -p VKey-rs -- "${args[@]}"
        ;;
    headless)
        export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"
        export RUST_LOG="${RUST_LOG:-debug}"
        info "Starting VKey-rs in headless mode"
        exec cargo run -p VKey-rs -- --headless "$@"
        ;;
    core|test-cli)
        info "Running the platform-independent Vietnamese core CLI"
        exec cargo run -p VKey-core-test -- "$@"
        ;;
    kbd-debug)
        export RUST_LOG="${RUST_LOG:-debug}"
        exec cargo run -p keyboard-debug -- "$@"
        ;;
    core-debug)
        export RUST_LOG="${RUST_LOG:-debug}"
        exec cargo run -p keyboard-core-debug -- "$@"
        ;;
    test)
        exec cargo test --workspace --all-features --locked "$@"
        ;;
    check)
        cargo fmt --all -- --check
        cargo check --workspace --all-targets --all-features --locked
        cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
        ;;
    all)
        bash "$0" check
        cargo test --workspace --all-features --locked
        cargo build --workspace --locked
        ;;
    package)
        exec bash "$SCRIPT_DIR/build.sh" "$@"
        ;;
    help|-h|--help)
        usage
        ;;
    *)
        usage >&2
        die "Unknown command: $COMMAND"
        ;;
esac
