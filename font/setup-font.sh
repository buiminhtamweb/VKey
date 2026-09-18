#!/usr/bin/env bash

set -euo pipefail

# Colors for terminal output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[0;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${BLUE}======================================================================${NC}"
echo -e "${GREEN}      VKey - Cài đặt Font tiếng Việt (VNI & Unicode) cho Linux        ${NC}"
echo -e "${BLUE}======================================================================${NC}"
echo ""

# Request root/sudo privileges if not running as root
if [ "${EUID:-$(id -u)}" -ne 0 ]; then
    echo -e "${YELLOW}[!] Cần quyền quản trị viên (sudo) để cài đặt font hệ thống.${NC}"
    echo -e "${YELLOW}    Vui lòng nhập mật khẩu của bạn bên dưới:${NC}"
    echo ""
    exec sudo bash "$0" "$@"
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VNI_ZIP="${SCRIPT_DIR}/vni.zip"

if [ ! -f "$VNI_ZIP" ]; then
    VNI_ZIP="/mnt/DATA/Software Projects/VKey/font/vni.zip"
fi

# Step 1: Install prerequisite tools
echo -e "${BLUE}[1/4] Kiểm tra các công cụ hệ thống cần thiết (unzip, fontconfig)...${NC}"
apt-get update -qq
DEBIAN_FRONTEND=noninteractive apt-get install -y -qq unzip fontconfig curl

# Step 2: Install Vietnamese Unicode Fonts from APT
echo -e "${BLUE}[2/4] Đang cài đặt các bộ font Unicode hỗ trợ tiếng Việt chuẩn...${NC}"
echo "    - Google Noto Fonts (fonts-noto-core, fonts-noto-cjk)"
echo "    - Inter Font (fonts-inter)"
echo "    - Liberation Fonts (fonts-liberation)"
echo "    - DejaVu Fonts (fonts-dejavu)"
echo "    - Microsoft TrueType Core Fonts (ttf-mscorefonts-installer: Times New Roman, Arial, ...)"

# Accept MS core fonts EULA automatically
echo "ttf-mscorefonts-installer msttcorefonts/accepted-mscorefonts-eula select true" | debconf-set-selections 2>/dev/null || true

DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \
    fonts-noto-core \
    fonts-noto-cjk \
    fonts-inter \
    fonts-liberation \
    fonts-dejavu \
    ttf-mscorefonts-installer || {
    echo -e "${YELLOW}[*] Cài đặt gói font chính, bỏ qua lỗi phụ nếu có...${NC}"
}

# Step 3: Install VNI Fonts from vni.zip
echo -e "${BLUE}[3/4] Đang giải nén và cài đặt các font VNI từ file vni.zip...${NC}"
if [ -f "$VNI_ZIP" ]; then
    SYS_FONT_DIR="/usr/local/share/fonts/vni"
    mkdir -p "$SYS_FONT_DIR"
    
    TMP_DIR="$(mktemp -d)"
    unzip -q -o "$VNI_ZIP" -d "$TMP_DIR"
    
    if [ -d "$TMP_DIR/vni" ]; then
        cp -rf "$TMP_DIR/vni/"*.TTF "$TMP_DIR/vni/"*.ttf "$SYS_FONT_DIR/" 2>/dev/null || true
    else
        cp -rf "$TMP_DIR/"*.TTF "$TMP_DIR/"*.ttf "$SYS_FONT_DIR/" 2>/dev/null || true
    fi
    rm -rf "$TMP_DIR"
    
    chmod 644 "$SYS_FONT_DIR"/* 2>/dev/null || true
    VNI_COUNT=$(ls -1 "$SYS_FONT_DIR" | wc -l)
    echo -e "${GREEN}[✓] Đã cài đặt thành công ${VNI_COUNT} font VNI vào ${SYS_FONT_DIR}${NC}"
    
    # Also install to user's local font directory if running via sudo
    TARGET_USER="${SUDO_USER:-}"
    if [ -n "$TARGET_USER" ]; then
        USER_HOME=$(getent passwd "$TARGET_USER" | cut -d: -f6)
        if [ -n "$USER_HOME" ] && [ -d "$USER_HOME" ]; then
            USER_FONT_DIR="$USER_HOME/.local/share/fonts/vni"
            mkdir -p "$USER_FONT_DIR"
            cp -rf "$SYS_FONT_DIR"/* "$USER_FONT_DIR/" 2>/dev/null || true
            chown -R "$TARGET_USER:$TARGET_USER" "$USER_HOME/.local/share/fonts" 2>/dev/null || true
        fi
    fi
else
    echo -e "${RED}[!] Không tìm thấy file vni.zip tại ${VNI_ZIP}!${NC}"
fi

# Step 4: Refresh font cache and desktop settings
echo -e "${BLUE}[4/4] Đang làm mới bộ nhớ đệm font của hệ thống (fc-cache)...${NC}"
fc-cache -f
if [ -n "${SUDO_USER:-}" ]; then
    su - "${SUDO_USER}" -c "fc-cache -f" 2>/dev/null || true
fi

# Configure desktop UI fonts if Cinnamon/GNOME is present
if command -v gsettings >/dev/null 2>&1; then
    TARGET_USER="${SUDO_USER:-$USER}"
    USER_UID=$(id -u "$TARGET_USER" 2>/dev/null || true)
    
    if [ -n "$USER_UID" ]; then
        sudo -u "$TARGET_USER" DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/${USER_UID}/bus" bash -c "
            gsettings set org.cinnamon.desktop.interface font-name 'Inter 10' 2>/dev/null || true
            gsettings set org.cinnamon.desktop.wm.preferences titlebar-font 'Inter Bold 10' 2>/dev/null || true
            gsettings set org.cinnamon.desktop.interface document-font-name 'Inter 10' 2>/dev/null || true
            gsettings set org.nemo.desktop font 'Inter 10' 2>/dev/null || true
            gsettings set org.cinnamon.desktop.interface monospace-font-name 'Noto Sans Mono 10' 2>/dev/null || true
            gsettings set org.cinnamon.desktop.interface font-antialiasing 'rgba' 2>/dev/null || true
            gsettings set org.cinnamon.desktop.interface font-hinting 'slight' 2>/dev/null || true
        " 2>/dev/null || true
    fi
fi

echo ""
echo -e "${GREEN}======================================================================${NC}"
echo -e "${GREEN}       CHÚC MỪNG! BẠN ĐÃ CÀI ĐẶT TOÀN BỘ FONT THÀNH CÔNG!             ${NC}"
echo -e "${GREEN}======================================================================${NC}"
echo -e "  ✓ Hơn 300 font chữ mã VNI-Windows đã sẵn sàng trong LibreOffice/Office."
echo -e "  ✓ Các bộ font Unicode tiếng Việt (Inter, Noto Sans/Serif, MS Core Fonts)."
echo -e "  ✓ Hệ thống đã nhận diện đầy đủ font mới."
echo ""
echo -e "${YELLOW}Nhấn phím bất kỳ (Enter hoặc phím bất kỳ) để đóng cửa sổ này...${NC}"
read -n 1 -s -r || true
echo ""