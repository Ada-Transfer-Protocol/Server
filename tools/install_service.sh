#!/bin/bash

# AdaTP Server & CLI Installer Script
# Bu script projeyi derler, systemd servisi oluşturur ve kısayolları (alias) ayarlar.

set -e # Hata olursa dur

echo "🚀 AdaTP Kurulumu Başlatılıyor..."

# 1. Rust Kontrolü
if ! command -v cargo &> /dev/null; then
    echo "❌ Rust (cargo) bulunamadı. Lütfen önce Rust yükleyin: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

# 2. Build İşlemleri
echo "📦 Sunucu derleniyor (Release mod)..."
cargo build --release --bin adatp-server

echo "📦 CLI Aracı derleniyor..."
# CLI aracı tools/adatp-cli altındaysa oraya git, yoksa ana projeden bak
if [ -d "tools/adatp-cli" ]; then
    cd tools/adatp-cli
    cargo build --release
    cd ../..
    CLI_BIN="./tools/adatp-cli/target/release/adatp-cli"
else
    # Eğer tools altında değilse ana workspace'dedir
    cargo build --release --bin adatp-cli
    CLI_BIN="./target/release/adatp-cli"
fi

SERVER_BIN="./target/release/adatp-server"

# 3. Binary'leri Taşıma (Sudo gerekebilir)
echo "📂 Binary dosyaları /usr/local/bin konumuna kopyalanıyor..."
if [ -w /usr/local/bin ]; then
    cp $SERVER_BIN /usr/local/bin/adatp-server
    cp $CLI_BIN /usr/local/bin/adatp
else
    sudo cp $SERVER_BIN /usr/local/bin/adatp-server
    sudo cp $CLI_BIN /usr/local/bin/adatp
fi

echo "✅ 'adatp-server' ve 'adatp' (CLI) komutları yüklendi."

# 4. Systemd Servisi (Sadece Linux)
if [ -d "/etc/systemd/system" ]; then
    echo "⚙️  Systemd servisi oluşturuluyor..."
    
    SERVICE_FILE="/etc/systemd/system/adatp.service"
    
    # Servis dosyasını oluştur
    sudo bash -c "cat > $SERVICE_FILE" <<EOF
[Unit]
Description=AdaTP Real-Time Server
After=network.target

[Service]
Type=simple
User=$USER
ExecStart=/usr/local/bin/adatp-server
Restart=always
RestartSec=3
Environment=RUST_LOG=info
WorkingDirectory=$(pwd)

[Install]
WantedBy=multi-user.target
EOF

    echo "🔄 Servis başlatılıyor..."
    sudo systemctl daemon-reload
    sudo systemctl enable adatp
    sudo systemctl restart adatp
    
    echo "✅ Servis 'adatp' adıyla çalışıyor."
else
    echo "⚠️  Systemd bulunamadı (Mac/Windows?). Servis kurulumu atlanıyor."
    echo "ℹ️  Sunucuyu başlatmak için: adatp-server"
fi

# 5. Alias Ekleme (.bashrc / .zshrc)
SHELL_RC=""
if [ -f "$HOME/.bashrc" ]; then SHELL_RC="$HOME/.bashrc"; fi
if [ -f "$HOME/.zshrc" ]; then SHELL_RC="$HOME/.zshrc"; fi

if [ -n "$SHELL_RC" ]; then
    echo "🔗 Alias'lar $SHELL_RC dosyasına ekleniyor..."
    
    # CLI kullanım kolaylığı için
    if ! grep -q "alias adatp-log=" "$SHELL_RC"; then
        echo "alias adatp-log='journalctl -u adatp -f'" >> "$SHELL_RC"
        echo "alias adatp-restart='sudo systemctl restart adatp'" >> "$SHELL_RC"
        echo "alias adatp-stop='sudo systemctl stop adatp'" >> "$SHELL_RC"
        echo "alias adatp-status='sudo systemctl status adatp'" >> "$SHELL_RC"
        echo "✅ Aliaslar eklendi: adatp-log, adatp-restart, adatp-status"
        echo "ℹ️  Aktif etmek için: source $SHELL_RC"
    else
        echo "ℹ️  Aliaslar zaten mevcut."
    fi
fi

echo ""
echo "🎉 Kurulum Tamamlandı!"
echo "------------------------------------------------"
echo " sunucu durumu : adatp-status"
echo " canlı loglar  : adatp-log"
echo " cli kullanımı : adatp --help"
echo "------------------------------------------------"
