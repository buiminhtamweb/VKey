#!/usr/bin/env bash

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m'

info() { echo -e "${BLUE}[INFO]${NC} $*"; }
success() { echo -e "${GREEN}[SUCCESS]${NC} $*"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*"; }

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT_DIR"

show_help() {
    echo "Usage: ./dev-linux.sh [COMMAND] [ARGS...]"
    echo ""
    echo "Linux Mint/Ubuntu development helper for VKey-rs."
    echo ""
    echo "Commands:"
    echo "  run [args]       Run VKey-rs in debug mode (default)"
    echo "  kbd-debug        Run X11 raw keyboard decoder"
    echo "  core-debug       Run X11 to Vietnamese engine observer"
    echo "  test-cli [args]  Run Vietnamese core CLI"
    echo "  check            Run fmt, clippy, and cargo check"
    echo "  test             Run workspace tests"
    echo "  deps             Check native Linux dependencies"
    echo "  install-deps     Install native Linux dependencies with apt"
    echo "  help             Show this help message"
    echo ""
    echo "Starting an X11 keyboard command stops known competing Viet+ processes"
    echo "for the current session because two global input methods cannot coexist."
}

require_linux() {
    local os_type
    os_type="$(uname -s)"
    if [ "$os_type" != "Linux" ]; then
        error "This script is for Linux only. Detected: $os_type"
        exit 1
    fi
}

ensure_deps() {
    ./build-linux.sh --check-deps-only
}

require_display() {
    if [ -z "${DISPLAY:-}" ]; then
        warn "DISPLAY is not set. X11 keyboard capture/injection will not work without an X server."
    fi
}

stop_conflicting_input_methods() {
    local process_name
    local stopped=false

    for process_name in vietc-tray vietc-daemon; do
        if pgrep -x "$process_name" >/dev/null 2>&1; then
            warn "Stopping competing input method process: $process_name"
            pkill -TERM -x "$process_name"
            stopped=true
        fi
    done

    if [ "$stopped" = false ]; then
        return
    fi

    # Give the competing daemon a brief chance to release its virtual X11
    # keyboard before VKey opens its own global observer.
    for _attempt in {1..20}; do
        if ! pgrep -x vietc-tray >/dev/null 2>&1 \
            && ! pgrep -x vietc-daemon >/dev/null 2>&1; then
            success "Competing Viet+ input method stopped for this session."
            return
        fi
        sleep 0.05
    done

    error "Could not stop Viet+. Stop vietc-tray/vietc-daemon before running VKey."
    exit 1
}

require_linux

CMD="${1:-run}"
if [ $# -gt 0 ]; then
    shift
fi

case "$CMD" in
    run)
        ensure_deps
        require_display
        stop_conflicting_input_methods
        info "Starting VKey-rs on Linux..."
        export RUST_BACKTRACE=1
        export RUST_LOG="${RUST_LOG:-debug}"
        exec cargo run -p VKey-rs -- "$@"
        ;;
    kbd-debug)
        ensure_deps
        require_display
        stop_conflicting_input_methods
        info "Starting keyboard-debug..."
        export RUST_BACKTRACE=1
        export RUST_LOG="${RUST_LOG:-debug}"
        exec cargo run -p keyboard-debug -- "$@"
        ;;
    core-debug)
        ensure_deps
        require_display
        stop_conflicting_input_methods
        info "Starting keyboard-core-debug..."
        export RUST_BACKTRACE=1
        export RUST_LOG="${RUST_LOG:-debug}"
        exec cargo run -p keyboard-core-debug -- "$@"
        ;;
    test-cli)
        info "Starting VKey-core-test..."
        exec cargo run -p VKey-core-test -- "$@"
        ;;
    check)
        ensure_deps
        info "Checking formatting..."
        cargo fmt --all -- --check
        info "Running clippy..."
        cargo clippy --workspace --all-targets --all-features -- -D warnings
        info "Type checking workspace..."
        cargo check --workspace
        success "Linux dev checks finished."
        ;;
    test)
        ensure_deps
        info "Running workspace tests..."
        exec cargo test --workspace "$@"
        ;;
    deps)
        ensure_deps
        ;;
    install-deps)
        ./build-linux.sh --install-deps --check-deps-only
        ;;
    help|-h|--help)
        show_help
        ;;
    *)
        error "Unknown command: $CMD"
        show_help
        exit 1
        ;;
esac
