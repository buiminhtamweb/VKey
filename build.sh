#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
cd "$SCRIPT_DIR"

info() { printf '\033[0;34m[build]\033[0m %s\n' "$*"; }
ok() { printf '\033[0;32m[done]\033[0m %s\n' "$*"; }
warn() { printf '\033[0;33m[warn]\033[0m %s\n' "$*"; }
die() { printf '\033[0;31m[error]\033[0m %s\n' "$*" >&2; exit 1; }
trap 'die "Command failed at line $LINENO"' ERR

usage() {
    cat <<'EOF'
Usage: bash build.sh [OPTIONS]

Build, verify, and package VKey-rs.

Options:
  --quick         Skip fmt, clippy, and tests; build/package release directly
  --no-check      Alias for --quick
  --no-package    Build release binaries without creating an archive
  --check-only    Run the complete validation suite without building a package
  -h, --help      Show this help

Artifacts are written to target/dist/.
EOF
}

RUN_CHECKS=true
RUN_BUILD=true
RUN_PACKAGE=true
while (($#)); do
    case "$1" in
        --quick|--no-check) RUN_CHECKS=false ;;
        --no-package) RUN_PACKAGE=false ;;
        --check-only) RUN_BUILD=false; RUN_PACKAGE=false ;;
        -h|--help) usage; exit 0 ;;
        *) die "Unknown option: $1" ;;
    esac
    shift
done

command -v cargo >/dev/null 2>&1 || die "Cargo is not available in PATH"

VERSION="$({
    awk '
        /^\[workspace\.package\]$/ { in_package=1; next }
        /^\[/ { in_package=0 }
        in_package && /^version[[:space:]]*=/ {
            gsub(/^[^"]*"|".*$/, "", $0); print; exit
        }
    ' Cargo.toml
} || true)"
[[ -n "$VERSION" ]] || die "Cannot read workspace version from Cargo.toml"

case "$(uname -s 2>/dev/null || printf unknown)" in
    Linux*) OS_NAME=linux; BIN_EXT= ;;
    Darwin*) OS_NAME=macos; BIN_EXT= ;;
    CYGWIN*|MINGW*|MSYS*|Windows_NT*) OS_NAME=windows; BIN_EXT=.exe ;;
    *) OS_NAME=unknown; BIN_EXT= ;;
esac

if [[ "$RUN_CHECKS" == true ]]; then
    info "Checking formatting"
    cargo fmt --all -- --check
    info "Checking workspace types"
    cargo check --workspace --all-targets --all-features --locked
    info "Running Clippy with warnings denied"
    cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
    info "Running workspace tests"
    cargo test --workspace --all-features --locked
    ok "Validation suite passed"
fi

if [[ "$RUN_BUILD" != true ]]; then
    exit 0
fi

info "Building release workspace"
cargo build --workspace --release --locked

if [[ "$RUN_PACKAGE" != true ]]; then
    ok "Release binaries are in target/release/"
    exit 0
fi

DIST_DIR="$SCRIPT_DIR/target/dist"
EXPECTED_DIST="$SCRIPT_DIR/target/dist"
[[ "$DIST_DIR" == "$EXPECTED_DIST" ]] || die "Refusing to replace unexpected path: $DIST_DIR"

PACKAGE_NAME="VKey-rs-${VERSION}-${OS_NAME}"
PACKAGE_DIR="$DIST_DIR/$PACKAGE_NAME"
rm -rf -- "$DIST_DIR"
mkdir -p -- "$PACKAGE_DIR/bin" "$PACKAGE_DIR/config"

BINARIES=(VKey-rs VKey-core-test keyboard-debug keyboard-core-debug)
for binary in "${BINARIES[@]}"; do
    source_path="$SCRIPT_DIR/target/release/${binary}${BIN_EXT}"
    [[ -f "$source_path" ]] || die "Missing release binary: $source_path"
    cp -- "$source_path" "$PACKAGE_DIR/bin/"
done

cp -- README.md "$PACKAGE_DIR/"
cp -R -- config/. "$PACKAGE_DIR/config/"
for asset in vkey_icon_*.png vkey_logo_*.png; do
    [[ -f "$asset" ]] && cp -- "$asset" "$PACKAGE_DIR/"
done

(
    cd "$PACKAGE_DIR"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum bin/* > SHA256SUMS
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 bin/* > SHA256SUMS
    else
        warn "sha256sum/shasum not found; SHA256SUMS was not generated"
    fi
)

if [[ "$OS_NAME" == windows ]] && command -v zip >/dev/null 2>&1; then
    ARCHIVE_PATH="$DIST_DIR/${PACKAGE_NAME}.zip"
    (cd "$DIST_DIR" && zip -q -r "$(basename "$ARCHIVE_PATH")" "$PACKAGE_NAME")
else
    ARCHIVE_PATH="$DIST_DIR/${PACKAGE_NAME}.tar.gz"
    command -v tar >/dev/null 2>&1 || die "tar is required to create $ARCHIVE_PATH"
    (cd "$DIST_DIR" && tar -czf "$(basename "$ARCHIVE_PATH")" "$PACKAGE_NAME")
fi

ok "Package directory: $PACKAGE_DIR"
ok "Release archive: $ARCHIVE_PATH"
