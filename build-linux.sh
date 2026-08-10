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

APT_PACKAGES=(
    build-essential
    pkg-config
    libxcb1-dev
    libxkbcommon-dev
    libxkbcommon-x11-dev
    libglib2.0-dev
    libgtk-3-dev
    libayatana-appindicator3-dev
    libxdo-dev
    rpm
)

RUN_CHECKS=true
INSTALL_DEPS=false
CHECK_DEPS_ONLY=false

show_help() {
    echo "Usage: ./build-linux.sh [OPTIONS]"
    echo ""
    echo "Linux Mint/Ubuntu build helper for VKey-rs."
    echo ""
    echo "Options:"
    echo "  --install-deps     Install required apt packages before building"
    echo "  --check-deps-only  Only verify Linux native dependencies"
    echo "  --no-check         Skip cargo fmt, clippy, and tests"
    echo "  -h, --help         Show this help message"
}

for arg in "$@"; do
    case "$arg" in
        --install-deps)
            INSTALL_DEPS=true
            ;;
        --check-deps-only)
            CHECK_DEPS_ONLY=true
            ;;
        --no-check)
            RUN_CHECKS=false
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

require_linux() {
    local os_type
    os_type="$(uname -s)"
    if [ "$os_type" != "Linux" ]; then
        error "This script is for Linux only. Detected: $os_type"
        exit 1
    fi
}

install_deps() {
    info "Installing Linux Mint/Ubuntu native dependencies..."
    sudo apt-get update
    sudo apt-get install -y "${APT_PACKAGES[@]}"
}

has_pkg_config_module() {
    pkg-config --exists "$1"
}

has_appindicator() {
    has_pkg_config_module ayatana-appindicator3-0.1 || has_pkg_config_module appindicator3-0.1
}

has_linker_library() {
    local lib_name="$1"
    local search_path

    if ldconfig -p 2>/dev/null | grep -Eq "lib${lib_name}\.so(\s|$)"; then
        return 0
    fi

    for search_path in /usr/lib /usr/local/lib /lib; do
        if find "$search_path" -name "lib${lib_name}.so" -print -quit 2>/dev/null | grep -q .; then
            return 0
        fi
    done

    return 1
}

check_deps() {
    local missing=()

    command -v cargo >/dev/null 2>&1 || missing+=("rust/cargo")
    command -v cc >/dev/null 2>&1 || missing+=("build-essential")
    command -v pkg-config >/dev/null 2>&1 || missing+=("pkg-config")

    if command -v pkg-config >/dev/null 2>&1; then
        has_pkg_config_module xcb || missing+=("libxcb1-dev")
        has_pkg_config_module xkbcommon || missing+=("libxkbcommon-dev")
        has_pkg_config_module xkbcommon-x11 || missing+=("libxkbcommon-x11-dev")
        has_pkg_config_module glib-2.0 || missing+=("libglib2.0-dev")
        has_pkg_config_module gtk+-3.0 || missing+=("libgtk-3-dev")
        has_appindicator || missing+=("libayatana-appindicator3-dev")
    fi

    has_linker_library xdo || missing+=("libxdo-dev")

    if [ "${#missing[@]}" -gt 0 ]; then
        error "Missing native dependencies: ${missing[*]}"
        echo ""
        echo "Install them on Linux Mint/Ubuntu with:"
        echo "  sudo apt update"
        echo "  sudo apt install ${APT_PACKAGES[*]}"
        echo ""
        echo "The current linker error 'unable to find library -lxdo' is fixed by libxdo-dev."
        exit 1
    fi

    success "Linux native dependencies look good."
}

build_deb() {
    local version="$1"
    local dist_dir="$2"

    if ! command -v dpkg-deb >/dev/null 2>&1; then
        warn "dpkg-deb tool not found. Skipping Debian (.deb) package creation."
        return 0
    fi

    info "Building Debian (.deb) package..."
    local deb_dir="/tmp/vkey-deb-build-${version}"
    local deb_pkg_dir="${deb_dir}/vkey_${version}_amd64"

    rm -rf "$deb_dir"
    mkdir -p "${deb_pkg_dir}/DEBIAN"
    mkdir -p "${deb_pkg_dir}/usr/bin"
    mkdir -p "${deb_pkg_dir}/usr/share/applications"
    mkdir -p "${deb_pkg_dir}/usr/share/pixmaps"
    mkdir -p "${deb_pkg_dir}/etc/xdg/autostart"

    # Create control file
    cat << EOF > "${deb_pkg_dir}/DEBIAN/control"
Package: vkey
Version: ${version}
Section: utils
Priority: optional
Architecture: amd64
Maintainer: Bùi Minh Tâm <buiminhtamweb@gmail.com>
Depends: libxcb1, libxkbcommon0, libxkbcommon-x11-0, libglib2.0-0, libgtk-3-0, libayatana-appindicator3-1, libxdo3
Description: Native Vietnamese input method in Rust
 VKey is a native Vietnamese input method written in Rust. It does not use
 Fcitx5, IBus, Electron, an external keyboard command, or the clipboard as its
 primary input path.
EOF

    # Create desktop file
    cat << EOF > "${deb_pkg_dir}/usr/share/applications/vkey.desktop"
[Desktop Entry]
Name=VKey
Comment=Native Vietnamese Input Method
Exec=VKey-rs
Icon=vkey
Terminal=false
Type=Application
Categories=Utility;Settings;
StartupNotify=true
EOF

    # Copy desktop file to autostart
    cp "${deb_pkg_dir}/usr/share/applications/vkey.desktop" "${deb_pkg_dir}/etc/xdg/autostart/"

    # Copy binary and icon
    cp target/release/VKey-rs "${deb_pkg_dir}/usr/bin/"
    if [ -f "vkey_icon_1786215513207.png" ]; then
        cp "vkey_icon_1786215513207.png" "${deb_pkg_dir}/usr/share/pixmaps/vkey.png"
    fi

    # Set correct permissions
    find "${deb_pkg_dir}" -type d -exec chmod 755 {} +
    find "${deb_pkg_dir}" -type f -exec chmod 644 {} +
    chmod 755 "${deb_pkg_dir}/usr/bin/VKey-rs"

    # Ensure target/dist exists in workspace
    mkdir -p "${dist_dir}"

    dpkg-deb --root-owner-group --build "${deb_pkg_dir}" "${dist_dir}/vkey_${version}_amd64.deb" >/dev/null
    success "Debian package created at: ${dist_dir}/vkey_${version}_amd64.deb"
    rm -rf "$deb_dir"
}

build_rpm() {
    local version="$1"
    local dist_dir="$2"

    if ! command -v rpmbuild >/dev/null 2>&1; then
        warn "rpmbuild tool not found. Skipping RPM (.rpm) package creation. (Install the 'rpm' package to enable)"
        return 0
    fi

    info "Building RPM (.rpm) package..."
    local rpm_dir="/tmp/vkey-rpm-build-${version}"

    rm -rf "$rpm_dir"
    mkdir -p "${rpm_dir}"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}

    # Copy binary, desktop, and icon to SOURCES
    cp target/release/VKey-rs "${rpm_dir}/SOURCES/"

    cat << EOF > "${rpm_dir}/SOURCES/vkey.desktop"
[Desktop Entry]
Name=VKey
Comment=Native Vietnamese Input Method
Exec=VKey-rs
Icon=vkey
Terminal=false
Type=Application
Categories=Utility;Settings;
StartupNotify=true
EOF

    if [ -f "vkey_icon_1786215513207.png" ]; then
        cp "vkey_icon_1786215513207.png" "${rpm_dir}/SOURCES/vkey.png"
    else
        touch "${rpm_dir}/SOURCES/vkey.png"
    fi

    # Create spec file
    cat << EOF > "${rpm_dir}/SPECS/vkey.spec"
Name:           vkey
Version:        ${version}
Release:        1%{?dist}
Summary:        Native Vietnamese input method in Rust
License:        MIT
URL:            https://github.com/buiminhtamweb/VKey
Packager:       Bùi Minh Tâm <buiminhtamweb@gmail.com>

%description
VKey is a native Vietnamese input method written in Rust.

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}/usr/bin
mkdir -p %{buildroot}/usr/share/applications
mkdir -p %{buildroot}/usr/share/pixmaps
mkdir -p %{buildroot}/etc/xdg/autostart

install -D -m 755 %{_sourcedir}/VKey-rs %{buildroot}/usr/bin/VKey-rs
install -D -m 644 %{_sourcedir}/vkey.desktop %{buildroot}/usr/share/applications/vkey.desktop
install -D -m 644 %{_sourcedir}/vkey.desktop %{buildroot}/etc/xdg/autostart/vkey.desktop
if [ -f %{_sourcedir}/vkey.png ]; then
    install -D -m 644 %{_sourcedir}/vkey.png %{buildroot}/usr/share/pixmaps/vkey.png
fi

%files
/usr/bin/VKey-rs
/usr/share/applications/vkey.desktop
/etc/xdg/autostart/vkey.desktop
/usr/share/pixmaps/vkey.png

%changelog
* Sun Aug 09 2026 Bùi Minh Tâm <buiminhtamweb@gmail.com> - ${version}-1
- Initial RPM packaging
EOF

    rpmbuild --define "_topdir ${rpm_dir}" -bb "${rpm_dir}/SPECS/vkey.spec" >/dev/null 2>&1

    # Ensure target/dist exists in workspace
    mkdir -p "${dist_dir}"

    local rpm_file
    rpm_file="$(find "${rpm_dir}/RPMS" -name "*.rpm" | head -n 1)"
    if [ -n "$rpm_file" ]; then
        cp "$rpm_file" "${dist_dir}/"
        success "RPM package created at: ${dist_dir}/$(basename "$rpm_file")"
    else
        error "Failed to generate RPM package."
    fi
    rm -rf "$rpm_dir"
}

package_release() {
    local version
    local dist_dir
    local pkg_name
    local pkg_dir
    local archive_path

    version="$(grep -A 5 "\[workspace.package\]" Cargo.toml 2>/dev/null | grep "^version =" | cut -d '"' -f2 || true)"
    if [ -z "$version" ]; then
        version="0.1.0"
        warn "Could not extract version from Cargo.toml. Defaulting to $version"
    fi

    dist_dir="target/dist"
    pkg_name="VKey-rs-${version}-linux"
    pkg_dir="${dist_dir}/${pkg_name}"

    info "Creating Linux package directory: ${pkg_dir}"
    rm -rf "$dist_dir"
    mkdir -p "${pkg_dir}/bin" "${pkg_dir}/config"

    for bin_name in VKey-rs keyboard-debug keyboard-core-debug VKey-core-test; do
        if [ ! -f "target/release/${bin_name}" ]; then
            error "Binary target/release/${bin_name} not found."
            exit 1
        fi
        cp "target/release/${bin_name}" "${pkg_dir}/bin/"
    done

    if [ -d config ]; then
        cp -r config/* "${pkg_dir}/config/"
    fi
    cp README.md "${pkg_dir}/"

    archive_path="${dist_dir}/${pkg_name}.tar.gz"
    info "Compressing release archive: ${archive_path}"
    (cd "$dist_dir" && tar -czf "${pkg_name}.tar.gz" "$pkg_name")
    success "Release archive created at: ${archive_path}"

    # Build DEB and RPM packages
    build_deb "$version" "$dist_dir"
    build_rpm "$version" "$dist_dir"
}

require_linux

if [ "$INSTALL_DEPS" = true ]; then
    install_deps
fi

check_deps

if [ "$CHECK_DEPS_ONLY" = true ]; then
    exit 0
fi

if [ "$RUN_CHECKS" = true ]; then
    info "Checking formatting..."
    cargo fmt --all -- --check

    info "Running clippy..."
    cargo clippy --workspace --all-targets --all-features -- -D warnings

    info "Running tests..."
    cargo test --workspace
else
    warn "Skipping quality checks."
fi

info "Building Linux release binaries..."
cargo build --workspace --release

package_release
success "Linux build finished."
