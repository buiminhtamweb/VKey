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
    echo "Builds and packages the openkey-rs workspace."
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

# 5. Packaging
DIST_DIR="target/dist"
PKG_NAME="openkey-rs-${VERSION}"
PKG_DIR="${DIST_DIR}/${PKG_NAME}"
ARCHIVE_PATH="${DIST_DIR}/${PKG_NAME}.tar.gz"

info "Creating package directory in ${PKG_DIR}..."
rm -rf "${DIST_DIR}"
mkdir -p "${PKG_DIR}/bin"
mkdir -p "${PKG_DIR}/config"

# Copy binary helper to handle optional .exe suffix (e.g., Windows build environment using Git Bash)
copy_binary() {
    local bin_name=$1
    local dest=$2
    if [ -f "target/release/${bin_name}" ]; then
        cp "target/release/${bin_name}" "$dest"
    elif [ -f "target/release/${bin_name}.exe" ]; then
        cp "target/release/${bin_name}.exe" "$dest"
    else
        error "Binary ${bin_name} not found in target/release/"
        exit 1
    fi
}

# Copy binaries
info "Copying binaries..."
copy_binary "openkey-rs" "${PKG_DIR}/bin/"
copy_binary "keyboard-debug" "${PKG_DIR}/bin/"
copy_binary "keyboard-core-debug" "${PKG_DIR}/bin/"
copy_binary "openkey-core-test" "${PKG_DIR}/bin/"

# Copy other assets
info "Copying configuration and documents..."
if [ -d "config" ]; then
    cp -r config/* "${PKG_DIR}/config/"
fi
if [ -f "README.md" ]; then
    cp README.md "${PKG_DIR}/"
fi

# Create archive
info "Compressing into ${ARCHIVE_PATH}..."
if command -v tar &> /dev/null; then
    (cd target/dist && tar -czf "${PKG_NAME}.tar.gz" "${PKG_NAME}")
    success "Packaging complete!"
    info "Package contents:"
    tar -tf "${ARCHIVE_PATH}" | sed 's/^/  /'
    echo ""
    success "Release archive created successfully at: ${ARCHIVE_PATH}"
else
    warn "tar is not available. Compiled files are placed in: ${PKG_DIR}"
fi
