# VKey

## Tiếng Việt

### Giới thiệu

VKey là bộ gõ Tiếng Việt viết bằng Rust.

- Có ứng dụng GUI và tray icon.
- Hỗ trợ Windows và Linux X11.
- Wayland hiện chưa được hỗ trợ.

### URL tải về

- Bản phát hành: `https://github.com/buiminhtamweb/VKey/releases`
- Mã nguồn: `https://github.com/buiminhtamweb/VKey`

### Cài đặt

#### Windows

1. Tải file `.zip` từ trang Releases.
2. Giải nén.
3. Chạy `VKey-rs.exe`.

#### Linux Mint / Ubuntu

Khuyến nghị dùng gói `.deb`:

```bash
sudo apt install ./vkey_<version>_amd64.deb
```

Nếu dùng bản `.tar.gz`, chỉ cần giải nén rồi chạy:

```bash
./bin/VKey-rs
```

### Gỡ bỏ

#### Windows

Thoát ứng dụng rồi xóa thư mục đã giải nén.

#### Linux Mint / Ubuntu

Nếu cài bằng `.deb`:

```bash
sudo apt remove vkey
```

Nếu chạy từ `.tar.gz`, xóa thư mục đã giải nén.

### Setup source, run, build

Yêu cầu:

- Rust stable `1.85+`
- Cargo
- Linux cần phiên X11 nếu muốn chạy bộ gõ trên desktop

Clone source:

```bash
git clone https://github.com/buiminhtamweb/VKey.git
cd VKey
```

#### Linux Mint / Ubuntu

Cài dependency native:

```bash
./build-linux.sh --install-deps --check-deps-only
```

Chạy ứng dụng:

```bash
./dev-linux.sh run
```

Chạy debug tool:

```bash
./dev-linux.sh kbd-debug
./dev-linux.sh core-debug
```

Build release:

```bash
./build-linux.sh
```

Build binary release không chạy full check:

```bash
cargo build --workspace --release
```

Artifact sau khi build:

- `target/release/`
- `target/dist/`

#### Windows

Chạy ứng dụng:

```bat
dev.bat run
```

Build release:

```bat
build.bat
```

## English

### Introduction

VKey is a Vietnamese input method written in Rust.

- It includes a GUI and tray icon.
- It supports Windows and Linux X11.
- Wayland is not supported yet.

### Download URLs

- Releases: `https://github.com/buiminhtamweb/VKey/releases`
- Source code: `https://github.com/buiminhtamweb/VKey`

### Install

#### Windows

1. Download the `.zip` file from Releases.
2. Extract it.
3. Run `VKey-rs.exe`.

#### Linux Mint / Ubuntu

The recommended option is the `.deb` package:

```bash
sudo apt install ./vkey_<version>_amd64.deb
```

If you use the `.tar.gz` package, extract it and run:

```bash
./bin/VKey-rs
```

### Uninstall

#### Windows

Exit the app and delete the extracted folder.

#### Linux Mint / Ubuntu

If installed from `.deb`:

```bash
sudo apt remove vkey
```

If you ran the `.tar.gz` package, delete the extracted folder.

### Source setup, run, and build

Requirements:

- Rust stable `1.85+`
- Cargo
- Linux requires an X11 session for desktop input

Clone the source:

```bash
git clone https://github.com/buiminhtamweb/VKey.git
cd VKey
```

#### Linux Mint / Ubuntu

Install native dependencies:

```bash
./build-linux.sh --install-deps --check-deps-only
```

Run the app:

```bash
./dev-linux.sh run
```

Run debug tools:

```bash
./dev-linux.sh kbd-debug
./dev-linux.sh core-debug
```

Build a release package:

```bash
./build-linux.sh
```

Build release binaries only without the full validation pass:

```bash
cargo build --workspace --release
```

Build artifacts:

- `target/release/`
- `target/dist/`

#### Windows

Run the app:

```bat
dev.bat run
```

Build a release:

```bat
build.bat
```
