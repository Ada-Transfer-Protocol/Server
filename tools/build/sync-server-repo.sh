#!/usr/bin/env bash
# Syncs the workspace-level canonical documentation and test assets into the
# Server repository, which is the published home of the project (the
# workspace root itself is not a git repository).
#
#   bash tools/build/sync-server-repo.sh
#
# Idempotent: re-running produces the same tree.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SERVER="$ROOT/server"

copy_dir() { # src dst
    rm -rf "$2"
    mkdir -p "$2"
    cp -R "$1/." "$2/"
}

echo "Syncing canonical docs into $SERVER ..."

# The spec pack and companion docs
copy_dir "$ROOT/docs/spec"        "$SERVER/docs/spec"
copy_dir "$ROOT/docs/platform"    "$SERVER/docs/platform"
copy_dir "$ROOT/docs/developer"   "$SERVER/docs/developer"
copy_dir "$ROOT/docs/production"  "$SERVER/docs/production"
copy_dir "$ROOT/docs/deployment"  "$SERVER/docs/deployment"
copy_dir "$ROOT/docs/testing"     "$SERVER/docs/testing"
copy_dir "$ROOT/docs/release"     "$SERVER/docs/release"
copy_dir "$ROOT/docs/enterprise"  "$SERVER/docs/enterprise"
mkdir -p "$SERVER/docs/protocol" "$SERVER/docs/architecture"
cp "$ROOT/docs/protocol/crypto.md"          "$SERVER/docs/protocol/crypto.md"
cp "$ROOT/docs/architecture/reliability.md" "$SERVER/docs/architecture/reliability.md"
cp "$ROOT/docs/legacy.md"                   "$SERVER/docs/legacy.md"
cp "$ROOT/SPEC.md"                          "$SERVER/docs/SPEC.md"
cp "$ROOT/SECURITY.md"                      "$SERVER/SECURITY.md"
cp "$ROOT/CONTRIBUTING.md"                  "$SERVER/CONTRIBUTING.md"

# Conformance assets: vectors + generator only. The Node/Python replay
# runners and the live integration suites need the sibling SDK checkouts,
# so they stay in the multi-repo workspace (documented in docs/testing/).
# The Rust replay is already self-contained at core/tests/conformance.rs.
mkdir -p "$SERVER/tests/conformance/vectors"
cp "$ROOT/tests/conformance/generate_vectors.mjs" "$SERVER/tests/conformance/"
cp "$ROOT/tests/conformance/vectors/adatp-v1-vectors.json" "$SERVER/tests/conformance/vectors/"
cat > "$SERVER/tests/conformance/README.md" <<'NOTE'
# Conformance assets

`vectors/adatp-v1-vectors.json` is the machine-readable golden vector set
(also embedded in docs/spec/appendix-test-vectors.md). The Rust replay runs
with `cargo test -p adatp-core` (see core/tests/conformance.rs). The
Node.js/Python replay runners and the live integration suites require the
sibling SDK checkouts — see docs/testing/README.md for the workspace layout.
NOTE

# Demos and companion tools
copy_dir "$ROOT/demos"            "$SERVER/demos"
copy_dir "$ROOT/tools/loadtest"   "$SERVER/tools/loadtest"
rm -rf "$SERVER/tools/loadtest/node_modules"
copy_dir "$ROOT/tools/webhook-receiver" "$SERVER/tools/webhook-receiver"
copy_dir "$ROOT/tools/build"      "$SERVER/tools/build"

# Housekeeping: never ship OS litter
find "$SERVER/docs" "$SERVER/tests" "$SERVER/demos" "$SERVER/tools" -name ".DS_Store" -delete 2>/dev/null || true

# Path-depth fixes for the copies: the workspace root's links assume the
# workspace layout; inside the Server repo the same files live one level
# deeper (docs/) or reference repo-root paths.
python3 - "$SERVER" <<'PYFIX'
import sys, re
server = sys.argv[1]

def rewrite(path, pairs):
    try:
        s = open(path, encoding='utf-8').read()
    except FileNotFoundError:
        return
    for a, b in pairs:
        s = s.replace(a, b)
    open(path, 'w', encoding='utf-8').write(s)

# docs/SPEC.md sits inside docs/, so strip the docs/ prefix and step up for
# repo-root paths.
rewrite(f"{server}/docs/SPEC.md", [
    ("](docs/spec/", "](spec/"),
    ("](docs/protocol/", "](protocol/"),
    ("](docs/architecture/", "](architecture/"),
    ("](docs/deployment/", "](deployment/"),
    ("](docs/legacy.md", "](legacy.md"),
    ("](docs/", "]("),
    ("](tests/conformance/", "](../tests/conformance/"),
    ("](server/docs/", "]("),
])

# CONTRIBUTING/SECURITY at repo root: SPEC.md lives under docs/ there.
for f in ("CONTRIBUTING.md", "SECURITY.md"):
    rewrite(f"{server}/{f}", [
        ("](SPEC.md)", "](docs/SPEC.md)"),
        ("](docs/spec/", "](docs/spec/"),
    ])

# Portal indexes reference the workspace-root SPEC.md two levels up; in the
# Server repo it is one level up inside docs/.
for f in ("developer/README.md", "production/README.md", "platform/README.md",
          "testing/README.md"):
    rewrite(f"{server}/docs/{f}", [("../../SPEC.md", "../SPEC.md")])
PYFIX

echo "Done. Review with: git -C server status"
