#!/bin/bash

# AdaTP Server & CLI Installer Script (English)

echo "🚀 Starting AdaTP Installation..."

# 1. Rust Check
if ! command -v cargo &> /dev/null; then
    echo "❌ Rust (cargo) not found."
    exit 1
fi

# 2. Build Process
# HACK: Bypass vendoring issues by removing local config if exists, forcing online build.
if [ -f ".cargo/config.toml" ]; then
    echo "⚠️  Bypassing vendoring (Online Build Mode)..."
    rm -f .cargo/config.toml
fi

echo "📦 Building Server (Release mode)..."
cargo build --release --bin adatp-server

echo "📦 Building CLI Tool..."
if [ -d "tools/adatp-cli" ]; then
    cd tools/adatp-cli
    cargo build --release
    cd ../..
else
    # Fallback if in root workspace
    cargo build --release --bin adatp-cli
fi

# 3. Locate Binaries (Smart Search)
SERVER_BIN=$(find . -type f -name adatp-server | grep "release/adatp-server" | head -n 1)
CLI_BIN=$(find . -type f -name adatp-cli | grep "release/adatp-cli" | head -n 1)

if [ -z "$SERVER_BIN" ]; then
    echo "❌ ERROR: adatp-server binary not found!"
    exit 1
fi

if [ -z "$CLI_BIN" ]; then
    echo "❌ ERROR: adatp-cli binary not found!"
    exit 1
fi

echo "✅ Found Binary: $SERVER_BIN"
echo "✅ Found Binary: $CLI_BIN"

# 4. Move Binaries
echo "📂 Installing binaries to /usr/local/bin..."
# Remove old links if any
rm -f /usr/local/bin/adatp-server
rm -f /usr/local/bin/adatp

cp "$SERVER_BIN" /usr/local/bin/adatp-server
cp "$CLI_BIN" /usr/local/bin/adatp

chmod +x /usr/local/bin/adatp-server
chmod +x /usr/local/bin/adatp

echo "✅ Installed 'adatp-server' and 'adatp' (CLI)."

# 5. Create Systemd Service
echo "⚙️  Creating Systemd Service..."
SERVICE_FILE="/etc/systemd/system/adatp-server.service"

# Service Content
cat > $SERVICE_FILE <<EOF
[Unit]
Description=AdaTP Real-Time Server
After=network.target

[Service]
Type=simple
User=root
ExecStart=/usr/local/bin/adatp-server
Restart=always
RestartSec=3
Environment=RUST_LOG=info
Environment=PORT=3000
Environment=HOST=0.0.0.0

[Install]
WantedBy=multi-user.target
EOF

# Reload & Start
systemctl daemon-reload
systemctl enable adatp-server
systemctl restart adatp-server

# Alias for 'adatp' service name as well for backward compatibility logic
# (Optional)

if systemctl is-active --quiet adatp-server; then
    echo "✅ Service 'adatp-server' started SUCCESSFULLY."
else
    echo "⚠️  Service failed to start. Check with 'systemctl status adatp-server'."
fi

# 6. Shell Aliases
SHELL_RC="$HOME/.bashrc"
if [ -f "$HOME/.zshrc" ]; then SHELL_RC="$HOME/.zshrc"; fi

# Add useful aliases if not present
grep -q "alias adatp-log=" "$SHELL_RC" || echo "alias adatp-log='journalctl -u adatp-server -f'" >> "$SHELL_RC"
grep -q "alias adatp-restart=" "$SHELL_RC" || echo "alias adatp-restart='systemctl restart adatp-server'" >> "$SHELL_RC"
grep -q "alias adatp-stop=" "$SHELL_RC" || echo "alias adatp-stop='systemctl stop adatp-server'" >> "$SHELL_RC"
grep -q "alias adatp-status=" "$SHELL_RC" || echo "alias adatp-status='systemctl status adatp-server'" >> "$SHELL_RC"

# 7. Add SSH Welcome Message (MOTD)
echo "🎨 Configuring SSH Welcome Message..."
MOTD_FILE="/etc/profile.d/99-adatp-motd.sh"

cat > \$MOTD_FILE <<'EOF'
#!/bin/bash
# AdaTP Welcome Screen

# Only show on interactive shells
if [ -n "\$PS1" ]; then
    echo -e "\033[1;36m"
    echo '   _       _       _____ ____  '
    echo '  /_\   __| | __ _|_   _|  _ \ '
    echo ' //_\\ / _` |/ _` | | | | |_) |'
    echo '/  _  \ (_| | (_| | | | |  __/ '
    echo ' \_/ \_/\__,_|\__,_| |_| |_|    '
    echo -e "\033[0m"
EOF

cat >> \$MOTD_FILE <<EOF
    
    # Check Status
    if systemctl is-active --quiet adatp-server; then
        STATUS="\033[1;32mACTIVE\033[0m"
    else
        STATUS="\033[1;31mSTOPPED\033[0m"
    fi
    
    # Detect IP
    IP_ADDR=\$(hostname -I | cut -d' ' -f1)
    
    echo -e " :: AdaTP Server ::    [ \$STATUS ]"
    echo -e " :: Internal URL ::    ws://127.0.0.1:3000"
    echo -e " :: External URL ::    ws://\$IP_ADDR:3000"
    echo -e " :: Monitor      ::    adatp-log"
    echo ""
fi
EOF

chmod +x $MOTD_FILE

echo ""
echo "🎉 Installation Complete!"
echo "------------------------------------------------"
echo " Server Status : adatp-status  (or systemctl status adatp-server)"
echo " Live Logs     : adatp-log"
echo " CLI Tool      : adatp --help"
echo "------------------------------------------------"
