#!/bin/bash

# AdaTP Server & CLI Installer Script (Fixed Paths)

# Hata olursa durdurma (bazı adımlar opsiyonel olabilir) ama kopyalamada dur
# set -e 

echo "🚀 AdaTP Kurulumu Başlatılıyor..."

# 1. Rust Kontrolü
if ! command -v cargo &> /dev/null; then
    echo "❌ Rust (cargo) bulunamadı."
    exit 1
fi

# 2. Build İşlemleri
# HACK: Vendor klasörü ile ilgili checksum sorunlarını aşmak için online build yapıyoruz.
if [ -f ".cargo/config.toml" ]; then
    echo "⚠️  Vendoring bypass ediliyor (Online Build Modu)..."
    rm -f .cargo/config.toml
fi

echo "📦 Sunucu derleniyor (Release mod)..."
cargo build --release --bin adatp-server

echo "📦 CLI Aracı derleniyor..."
if [ -d "tools/adatp-cli" ]; then
    cd tools/adatp-cli
    cargo build --release
    cd ../..
else
    cargo build --release --bin adatp-cli
fi

# 3. Binary Konumlarını Bulma (Akıllı Arama)
SERVER_BIN=$(find . -type f -name adatp-server | grep "release/adatp-server" | head -n 1)
CLI_BIN=$(find . -type f -name adatp-cli | grep "release/adatp-cli" | head -n 1)

if [ -z "$SERVER_BIN" ]; then
    echo "❌ HATA: adatp-server binary dosyası bulunamadı!"
    exit 1
fi

if [ -z "$CLI_BIN" ]; then
    echo "❌ HATA: adatp-cli binary dosyası bulunamadı!"
    exit 1
fi

echo "✅ Binary bulundu: $SERVER_BIN"
echo "✅ Binary bulundu: $CLI_BIN"

# 4. Binary'leri Taşıma
echo "📂 Binary dosyaları /usr/local/bin konumuna kopyalanıyor..."
cp "$SERVER_BIN" /usr/local/bin/adatp-server
cp "$CLI_BIN" /usr/local/bin/adatp

# İzinleri ayarla
chmod +x /usr/local/bin/adatp-server
chmod +x /usr/local/bin/adatp

echo "✅ 'adatp-server' ve 'adatp' yüklendi."

# 5. Systemd Servisi
echo "⚙️  Systemd servisi oluşturuluyor..."
SERVICE_FILE="/etc/systemd/system/adatp.service"

# Servis içeriği
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
# WorkingDirectory=/var/lib/adatp/ (Opsiyonel, DB için)

[Install]
WantedBy=multi-user.target
EOF

# Systemd Reload & Start
systemctl daemon-reload
systemctl enable adatp
systemctl restart adatp

if systemctl is-active --quiet adatp; then
    echo "✅ Servis BAŞARIYLA başlatıldı."
else
    echo "⚠️  Servis başlatılamadı. 'systemctl status adatp' ile kontrol edin."
fi

# 6. Alias Ekleme
SHELL_RC="$HOME/.bashrc"
if [ -f "$HOME/.zshrc" ]; then SHELL_RC="$HOME/.zshrc"; fi

grep -q "alias adatp-log=" "$SHELL_RC" || echo "alias adatp-log='journalctl -u adatp -f'" >> "$SHELL_RC"
grep -q "alias adatp-restart=" "$SHELL_RC" || echo "alias adatp-restart='systemctl restart adatp'" >> "$SHELL_RC"
grep -q "alias adatp-stop=" "$SHELL_RC" || echo "alias adatp-stop='systemctl stop adatp'" >> "$SHELL_RC"
grep -q "alias adatp-status=" "$SHELL_RC" || echo "alias adatp-status='systemctl status adatp'" >> "$SHELL_RC"

echo ""
echo "🎉 Kurulum Tamamlandı!"
echo "------------------------------------------------"
echo " sunucu durumu : systemctl status adatp"
echo " loglar        : journalctl -u adatp -f"
echo "------------------------------------------------"
