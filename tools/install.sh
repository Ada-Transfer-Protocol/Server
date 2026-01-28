#!/bin/bash
# AdaTP One-Line Installer
# Usage: curl -sSL https://raw.githubusercontent.com/Ada-Transfer-Protocol/Server/main/tools/install.sh | bash

set -e

echo -e "\033[1;36m"
echo "   _       _       _____ ____  "
echo "  /_\   __| | __ _|_   _|  _ \ "
echo " //_\\\\ / _\` |/ _\` | | | | |_) |"
echo "/  _  \ (_| | (_| | | | |  __/ "
echo "\_/ \_/\__,_|\__,_| |_| |_|    "
echo -e "\033[0m"
echo "Select Install Mode:"
echo "1) Full Installation (Server + CLI + Service)"
echo "2) Development Setup (Clone only)"
read -p "Choose (1/2): " choice

if [ "$choice" == "1" ]; then
    echo "⬇️  Downloading AdaTP Server..."
    TEMP_DIR=$(mktemp -d)
    git clone --depth 1 https://github.com/Ada-Transfer-Protocol/Server.git "$TEMP_DIR/AdaTP"
    
    cd "$TEMP_DIR/AdaTP"
    chmod +x tools/install_service.sh
    ./tools/install_service.sh
    
    rm -rf "$TEMP_DIR"
    echo "✅ Cleaned up temporary files."
    echo "🚀 AdaTP is ready! Type 'adatp-status' to check."

elif [ "$choice" == "2" ]; then
    echo "📂 Cloning into ./AdaTP-Server..."
    git clone https://github.com/Ada-Transfer-Protocol/Server.git AdaTP-Server
    cd AdaTP-Server
    echo "✅ Cloned. Run 'cargo run --bin adatp-server' to start."
else
    echo "Aborted."
    exit 1
fi
