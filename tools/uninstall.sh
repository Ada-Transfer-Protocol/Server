#!/bin/bash
# AdaTP Uninstaller Script

echo "🗑️  Uninstalling AdaTP Server..."

# 1. Stop Service
if systemctl is-active --quiet adatp-server; then
    echo "Stopping service..."
    systemctl stop adatp-server
fi

if systemctl is-active --quiet adatp; then
    systemctl stop adatp
fi

# 2. Disable & Remove Service
echo "Removing systemd service..."
systemctl disable adatp-server 2>/dev/null
systemctl disable adatp 2>/dev/null
rm -f /etc/systemd/system/adatp-server.service
rm -f /etc/systemd/system/adatp.service
systemctl daemon-reload

# 3. Remove Binaries
echo "Removing binaries..."
rm -f /usr/local/bin/adatp-server
rm -f /usr/local/bin/adatp

# 4. Remove Aliases (Cleaner)
# This is tricky without messing up .bashrc, maybe just warn user or use sed
if [ -f "$HOME/.bashrc" ]; then
    sed -i '/alias adatp-/d' "$HOME/.bashrc"
    echo "Removed aliases from .bashrc"
fi

if [ -f "$HOME/.zshrc" ]; then
    sed -i '/alias adatp-/d' "$HOME/.zshrc"
    echo "Removed aliases from .zshrc"
fi

echo "✅ AdaTP has been completely removed from this system."
