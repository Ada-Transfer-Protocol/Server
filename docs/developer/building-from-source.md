# Building from Source

Release artifacts and scripts: [`docs/release/ARTIFACTS.md`](../release/ARTIFACTS.md).
This page is the per-component build reference.

## Server (Rust)

The workspace **vendors every crate** (`vendor/` + `.cargo/config.toml`),
so builds are reproducible and work offline — and adding new dependencies
outside the vendored set will not compile.

Prerequisites:

- Rust stable (≥ 1.75 recommended) via rustup
- **Linux**: `pkg-config` + OpenSSL headers — `sudo apt-get install
  pkg-config libssl-dev` (Debian/Ubuntu) or `openssl-devel` (RHEL/Fedora)
- **macOS**: Xcode command-line tools (`xcode-select --install`);
  system/Homebrew OpenSSL is found automatically

```bash
cd server
cargo build --release --offline
# → target/release/adatp-server   (server + embedded Silo Panel)
# → target/release/adatp-admin    (API-key manager)
cargo build --release --offline -p adatp-cli
# → target/release/adatp-cli     (protocol test tool)
```

Verify the build:

```bash
cargo test --workspace --offline      # unit + golden-vector conformance
./target/release/adatp-server &       # then:
cargo run -p adatp-cli -- -a 127.0.0.1:3000 -u user1 -p password123
```

### Docker image

```bash
cd server
docker build -t adatp-server .        # multi-stage; cargo runs offline inside
```

Compose deployment: [`docs/deployment/quickstart.md`](../deployment/quickstart.md).

### Cross-compiling (honest note)

Static Linux binaries via `x86_64-unknown-linux-musl` are the usual route
(`rustup target add x86_64-unknown-linux-musl`, plus a musl-target OpenSSL
or switching TLS features) — **not part of the verified build matrix**;
the supported artifacts are native builds on the CI's Linux/macOS runners.

## SDKs

| SDK | Build | Output |
| :-- | :-- | :-- |
| Browser JS | `cd sdks/js && npm install && npm run build` | `dist/adatp.js`, `dist/client.js` (+ `.d.ts`) via tsup |
| Node.js | `cd sdks/node && npm install && npx tsc` | `dist/*.js` (CommonJS) |
| Python | nothing to compile — `pip install cryptography websocket-client`, use `PYTHONPATH=src` or `pip install .` | wheel/sdist via `python -m build` if you need packages |
| PHP | `cd sdks/php && composer install` | autoloaded `src/` |
| C | `cd sdks/c && cmake -B build -DCMAKE_BUILD_TYPE=Release && cmake --build build` (needs OpenSSL) | `libadatp` + `adatp_example` |
| Arduino/ESP32 | open in Arduino IDE / arduino-cli with an ESP32 board; the library is `sdks/arduino-esp32` | flashed sketch |

## Workspace test batteries

```bash
bash tests/conformance/run.sh     # Rust + Node + Python golden vectors
bash tests/integration/run.sh     # live end-to-end suites (auto free port)
```

Both must pass before claiming a working build —
[testing guide](../testing/README.md).

## CI reference

`server/.github/workflows/ci.yml` builds and tests on Ubuntu + macOS and
uploads release binaries; each SDK repo has a minimal build/lint workflow.
The Arduino SDK has no CI (needs an ESP32 toolchain) — verified by
host-side syntax builds only.
