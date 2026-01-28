#!/bin/bash
# AdaTP Setup Script (Fresh File to bypass Cache)
# Usage: curl -sSL https://raw.githubusercontent.com/Ada-Transfer-Protocol/Server/main/tools/setup.sh | bash

# Reset color
NC='\033[0m'
CYAN='\033[1;36m'
GREEN='\033[0;32m'
RED='\033[0;31m'

echo -e "${CYAN}"
echo "   _       _       _____ ____  "
echo "  /_\   __| | __ _|_   _|  _ \ "
echo " //_\\\\ / _\` |/ _\` | | | | |_) |"
echo "/  _  \ (_| | (_| | | | |  __/ "
echo "\_/ \_/\__,_|\__,_| |_| |_|    "
echo -e "${NC}"

echo "🚀 Starting AdaTP Automated Installation..."
echo "------------------------------------------------"

# --- 1. Detect Package Manager & Install Dependencies ---
echo "📦 Checking System Dependencies..."

if [ -f /etc/os-release ]; then
    . /etc/os-release
    OS=$NAME
    VER=$VERSION_ID
else
    OS=$(uname -s)
    VER=$(uname -r)
fi

echo "   Detected OS: $OS"

if command -v apt-get &> /dev/null; then
    # Debian / Ubuntu
    echo "   Using apt-get..."
    export DEBIAN_FRONTEND=noninteractive
    sudo apt-get update -y
    sudo apt-get install -y build-essential git curl libssl-dev pkg-config
elif command -v dnf &> /dev/null; then
    # Fedora / RHEL 8+
    echo "   Using dnf..."
    sudo dnf groupinstall -y "Development Tools"
    sudo dnf install -y git curl openssl-devel perl-core
elif command -v yum &> /dev/null; then
    # CentOS / RHEL 7
    echo "   Using yum..."
    sudo yum groupinstall -y "Development Tools"
    sudo yum install -y git curl openssl-devel
elif command -v pacman &> /dev/null; then
    # Arch Linux
    echo "   Using pacman..."
    sudo pacman -Syu --noconfirm base-devel git curl openssl
elif command -v apk &> /dev/null; then
    # Alpine Linux
    echo "   Using apk..."
    sudo apk add build-base git curl openssl-dev
else
    echo -e "${RED}⚠️  Unknown Package Manager! Assuming dependencies are installed...${NC}"
fi

# --- 2. Install Rust (if missing) ---
if ! command -v cargo &> /dev/null; then
    echo "🦀 Rust not found. Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
else
    echo "✅ Rust is already installed."
fi

# Ensure cargo is in PATH for this session
export PATH="$HOME/.cargo/bin:$PATH"

# --- 3. Clone & Build ---
echo "⬇️  Cloning AdaTP Server Repository..."
TEMP_DIR=$(mktemp -d)
git clone --depth 1 https://github.com/Ada-Transfer-Protocol/Server.git "$TEMP_DIR/AdaTP"

cd "$TEMP_DIR/AdaTP"
chmod +x tools/install_service.sh

echo "⚙️  Building and Installing AdaTP Service..."
# Run the service installer (handles build, moving binaries, systemd)
./tools/install_service.sh

# --- 4. Cleanup ---
cd ~
rm -rf "$TEMP_DIR"
echo "🧹 Cleaned up temporary files."

echo "------------------------------------------------"
echo -e "${GREEN}🎉 Installation Complete!${NC}"
echo "------------------------------------------------"
echo "Check Status : adatp-status"
echo "View Logs    : adatp-log"
echo "Stop Server  : adatp-stop"
echo "------------------------------------------------"
