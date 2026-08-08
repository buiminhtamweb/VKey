#!/usr/bin/env bash

# Exit immediately if a command exits with a non-zero status
set -eo pipefail

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

# Display help message
show_help() {
    echo "Usage: ./build.sh [OPTIONS]"
    echo ""
    echo "Builds and packages the VKey-rs workspace."
    echo ""
    echo "Options:"
    echo "  --no-check    Skip quality checks (cargo fmt, cargo clippy, cargo test)"
    echo "  -h, --help    Show this help message"
}

# Parse command line options
RUN_CHECKS=true
for arg in "$@"; do
    case "$arg" in
        --no-check)
            RUN_CHECKS=false
            shift
            ;;
        -h|--help)
            show_help
            exit 0
            ;;
        *)
            error "Unknown argument: $arg"
            show_help
            exit 1
            ;;
    esac
done

info "Starting build process..."

# 1. Check requirements
if ! command -v cargo &> /dev/null; then
    error "Cargo is not installed or not in PATH."
    exit 1
fi

# 2. Extract version from Cargo.toml
info "Extracting project version..."
VERSION=$(grep -A 5 "\[workspace.package\]" Cargo.toml 2>/dev/null | grep "^version =" | cut -d '"' -f2 || true)
if [ -z "$VERSION" ]; then
    VERSION="0.1.0"
    warn "Could not extract version from Cargo.toml. Defaulting to $VERSION"
else
    info "Found project version: $VERSION"
fi

# 3. Quality Checks (Formatter, Linter, Tests)
if [ "$RUN_CHECKS" = true ]; then
    info "Running code quality checks..."
    
    info "Checking formatting (cargo fmt)..."
    cargo fmt --all -- --check
    
    info "Running linter (cargo clippy)..."
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    
    info "Running tests (cargo test)..."
    cargo test --workspace
    
    success "All checks passed!"
else
    warn "Skipping quality checks as requested."
fi

# 4. Compile in Release Mode
info "Compiling release binaries..."
cargo build --workspace --release

# 5. OS Detection & Packaging
OS_TYPE=$(uname -s)
case "$OS_TYPE" in
    Linux*)
        OS_NAME="linux"
        BIN_EXT=""
        ARCHIVE_EXT="tar.gz"
        ;;
    Darwin*)
        OS_NAME="macos"
        BIN_EXT=""
        ARCHIVE_EXT="tar.gz"
        ;;
    CYGWIN*|MINGW*|MSYS*|Windows_NT*)
        OS_NAME="windows"
        BIN_EXT=".exe"
        ARCHIVE_EXT="zip"
        ;;
    *)
        OS_NAME="unknown"
        BIN_EXT=""
        ARCHIVE_EXT="tar.gz"
        ;;
esac

info "Detected operating system: ${OS_NAME} (Binary suffix: '${BIN_EXT}')"

DIST_DIR="target/dist"
PKG_NAME="VKey-rs-${VERSION}-${OS_NAME}"
PKG_DIR="${DIST_DIR}/${PKG_NAME}"

info "Creating package directory in ${PKG_DIR}..."
rm -rf "${DIST_DIR}"
mkdir -p "${PKG_DIR}/bin"
mkdir -p "${PKG_DIR}/config"

# Copy binary helper using detected BIN_EXT
copy_binary() {
    local bin_name=$1
    local dest=$2
    local src_path="target/release/${bin_name}${BIN_EXT}"
    if [ -f "$src_path" ]; then
        cp "$src_path" "$dest"
    else
        error "Binary ${bin_name}${BIN_EXT} not found in target/release/"
        exit 1
    fi
}

# Copy binaries
info "Copying binaries..."
copy_binary "VKey-rs" "${PKG_DIR}/bin/"
copy_binary "keyboard-debug" "${PKG_DIR}/bin/"
copy_binary "keyboard-core-debug" "${PKG_DIR}/bin/"
copy_binary "VKey-core-test" "${PKG_DIR}/bin/"

# Copy other assets
info "Copying configuration and documents..."
if [ -d "config" ]; then
    cp -r config/* "${PKG_DIR}/config/"
fi
if [ -f "README.md" ]; then
    cp README.md "${PKG_DIR}/"
fi

# Create archive
if [ "$OS_NAME" = "windows" ] && command -v zip &> /dev/null; then
    ARCHIVE_PATH="${DIST_DIR}/${PKG_NAME}.zip"
    info "Compressing into ${ARCHIVE_PATH}..."
    (cd target/dist && zip -r "${PKG_NAME}.zip" "${PKG_NAME}")
    success "Packaging complete! Release archive created successfully at: ${ARCHIVE_PATH}"
elif command -v tar &> /dev/null; then
    ARCHIVE_PATH="${DIST_DIR}/${PKG_NAME}.${ARCHIVE_EXT}"
    info "Compressing into ${ARCHIVE_PATH}..."
    (cd target/dist && tar -czf "${PKG_NAME}.${ARCHIVE_EXT}" "${PKG_NAME}")
    success "Packaging complete!"
    info "Package contents:"
    tar -tf "${ARCHIVE_PATH}" | sed 's/^/  /'
    echo ""
    success "Release archive created successfully at: ${ARCHIVE_PATH}"
else
    warn "Neither zip nor tar is available. Compiled files are placed in: ${PKG_DIR}"
fi
