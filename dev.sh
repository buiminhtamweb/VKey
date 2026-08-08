#!/usr/bin/env bash

# Exit immediately if a command exits with a non-zero status
set -e

# ANSI color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging helpers
info() { echo -e "${BLUE}[INFO]${NC} $*"; }
success() { echo -e "${GREEN}[SUCCESS]${NC} $*"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*"; }

show_help() {
    echo "Usage: ./dev.sh [COMMAND] [ARGS...]"
    echo ""
    echo "Development helper script for openkey-rs."
    echo ""
    echo "Commands:"
    echo "  run [args]     Run openkey-rs with debug-input and optional arguments (default)"
    echo "  test           Run all tests in the workspace"
    echo "  check          Run cargo check, format, and clippy checks"
    echo "  kbd-debug      Run X11 raw keyboard decoder debug binary"
    echo "  core-debug     Run X11 to Vietnamese engine composition observer"
    echo "  test-cli       Run Vietnamese core CLI with manual input (e.g. ./dev.sh test-cli \"tieengs\")"
    echo "  help           Show this help message"
    echo ""
    echo "If no command is provided, it defaults to: run --debug-input"
}

CMD=${1:-run}
if [ $# -gt 0 ]; then
    shift
fi

case "$CMD" in
    run)
        # Default flags for development run if no arguments are provided
        ARGS=("$@")
        if [ ${#ARGS[@]} -eq 0 ]; then
            ARGS=("--debug-input")
        fi
        info "Starting openkey-rs daemon (debug profile) with args: ${ARGS[*]}..."
        export RUST_BACKTRACE=1
        export RUST_LOG=debug
        exec cargo run -p openkey-rs -- "${ARGS[@]}"
        ;;
    test)
        info "Running workspace tests..."
        exec cargo test --workspace "$@"
        ;;
    check)
        info "Formatting check..."
        cargo fmt --all -- --check
        info "Clippy lints..."
        cargo clippy --workspace --all-targets --all-features -- -D warnings
        info "Type check..."
        cargo check --workspace
        success "Checks finished successfully!"
        ;;
    kbd-debug)
        info "Starting keyboard-debug..."
        export RUST_LOG=debug
        exec cargo run -p keyboard-debug -- "$@"
        ;;
    core-debug)
        info "Starting keyboard-core-debug..."
        export RUST_LOG=debug
        exec cargo run -p keyboard-core-debug -- "$@"
        ;;
    test-cli)
        info "Starting openkey-core-test CLI..."
        exec cargo run -p openkey-core-test -- "$@"
        ;;
    help|-h|--help)
        show_help
        exit 0
        ;;
    *)
        error "Unknown command: $CMD"
        show_help
        exit 1
        ;;
esac
