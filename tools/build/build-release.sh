#!/usr/bin/env bash
# AdaTP portable release builder.
#
#   bash tools/build/build-release.sh [output-dir]
#
# Produces: adatp-server-<version>-<os>-<arch>.tar.gz + SHA256SUMS
# Tarball layout:
#   adatp-<version>/
#     bin/adatp-server        the server
#     bin/adatp-admin         API-key / stats CLI
#     bin/adatp-cli           protocol test tool (handshake+login probe)
#     plugins/echo/…          bundled example plugins
#     plugins/moderation/…
#     users.json.example      demo user file (dev only)
#     .env.example            configuration template
#     README.md               quickstart for the unpacked artifact
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SERVER="$ROOT/server"
OUT="${1:-$ROOT/dist}"

VERSION="$(grep -m1 '^version' "$SERVER/server/Cargo.toml" | sed 's/.*"\(.*\)"/\1/')"
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"
case "$ARCH" in x86_64) ARCH=amd64;; aarch64|arm64) ARCH=arm64;; esac

NAME="adatp-server-${VERSION}-${OS}-${ARCH}"
STAGE="$(mktemp -d)/adatp-${VERSION}"
mkdir -p "$STAGE/bin" "$OUT"

echo "[1/4] Building release binaries (${VERSION}, ${OS}/${ARCH})..."
(cd "$SERVER" && cargo build --release --offline --workspace)

cp "$SERVER/target/release/adatp-server" "$STAGE/bin/"
cp "$SERVER/target/release/adatp-admin" "$STAGE/bin/"
cp "$SERVER/target/release/adatp-cli" "$STAGE/bin/"

echo "[2/4] Staging support files..."
cp -R "$SERVER/plugins" "$STAGE/plugins"
find "$STAGE" -name ".DS_Store" -delete
cp "$SERVER/server/users.json" "$STAGE/users.json.example"
cp "$SERVER/server/.env.example" "$STAGE/.env.example"

cat > "$STAGE/README.md" <<EOF
# AdaTP server ${VERSION} (${OS}/${ARCH})

Quickstart:

    cp users.json.example users.json     # dev/demo credentials — replace!
    cp .env.example .env                 # review configuration
    ./bin/adatp-server                   # listens on 0.0.0.0:3000

Verify end to end:

    ./bin/adatp-cli -a 127.0.0.1:3000 -u user1 -p password123

Operator UI:    http://127.0.0.1:3000/silo  (token: see ADMIN_TOKEN / log)
Health:         /healthz    Readiness: /readyz
Docs:           https://github.com/Ada-Transfer-Protocol/Server (docs/)
EOF

echo "[3/4] Creating tarball..."
TARBALL="$OUT/${NAME}.tar.gz"
tar -czf "$TARBALL" -C "$(dirname "$STAGE")" "$(basename "$STAGE")"

echo "[4/4] Checksums..."
(cd "$OUT" && { shasum -a 256 "${NAME}.tar.gz" 2>/dev/null || sha256sum "${NAME}.tar.gz"; } >> SHA256SUMS && sort -u SHA256SUMS -o SHA256SUMS)

rm -rf "$(dirname "$STAGE")"
echo
echo "Artifact: $TARBALL"
echo "Checksums: $OUT/SHA256SUMS"
tar -tzf "$TARBALL" | head -12
