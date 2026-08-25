# AdaTP Testing Guide

Four layers of verification, all runnable from the workspace root.

## 1. Unit + conformance (golden vectors)

The protocol's source of truth is the deterministic vector set at
[`tests/conformance/vectors/adatp-v1-vectors.json`](../../tests/conformance/vectors/adatp-v1-vectors.json)
(9 cases: framing, HKDF, nonce, AES-GCM both directions, negatives). It is
embedded in [`docs/spec/appendix-test-vectors.md`](../spec/appendix-test-vectors.md)
and replayed by three independent implementations:

```bash
bash tests/conformance/run.sh
# == Rust reference ==   cargo test -p adatp-core   (9 tests)
# == Node.js SDK ==      tests/conformance/run_node.mjs
# == Python SDK ==       tests/conformance/run_python.py
```

Regenerating the vectors (only when the spec changes intentionally):

```bash
cd tests/conformance
node generate_vectors.mjs > vectors/adatp-v1-vectors.json
cp vectors/adatp-v1-vectors.json ../../server/core/tests/vectors.json
```

## 2. Integration (live server, end to end)

```bash
bash tests/integration/run.sh          # picks a free port automatically
PORT=3210 bash tests/integration/run.sh # or pin one
```

Four suites, 61 assertions:

| Suite | Covers |
| :-- | :-- |
| `ws_text_roundtrip.mjs` | plaintext clients: auth success/failure, unauthenticated refusal, JoinRoom→RoomJoined, echo, room isolation |
| `secure_roundtrip.mjs` | X25519 handshake, encrypted auth/join/text, GameState roundtrip, tamper (wrong password) |
| `tools_plugins.mjs` | listTools/callTool, tool error contract, TOOL: text fallback, moderation veto hook |
| `admin_webhooks.mjs` | admin token auth, overview/live counters, webhook CRUD + HMAC-verified delivery, plugin custom events, kick, drain, Silo serving |

The runner builds the server + Node SDK, starts an isolated server
(temp DB, example plugins, `ADMIN_TOKEN=itest-admin-token`,
`ADATP_WEBHOOK_ALLOW_PRIVATE=1`), runs all suites, and tears down.

## 3. Load

```bash
cd tools/loadtest && npm install
node loadtest.mjs --url ws://127.0.0.1:3000/ws \
    --clients 100 --rooms 10 --rate 5 --duration 30
```

Reports connect/auth/join success, send/receive throughput, and
end-to-end latency percentiles (p50/p95/p99) measured through the full
send→route→receive path. Reference numbers on a laptop (debug build):
40 clients × 10 msg/s ⇒ ~3 900 deliveries/s, p99 = 8 ms, zero errors.

## 4. SDK smoke tests

Each non-JS SDK has a live smoke path exercised during development
(PHP/Python/C connect → handshake → auth → join → echo → negative auth).
The C build doubles as its test: `cd sdks/c && cmake -B build && cmake
--build build`.

## CI

- `server/.github/workflows/ci.yml` — Linux + macOS matrix: offline vendored
  build, full `cargo test` (includes the conformance vectors), release
  binary artifact upload.
- Each SDK repo carries a minimal build/lint workflow
  (`.github/workflows/ci.yml`). The Arduino SDK has no CI (needs hardware
  or an ESP32 toolchain image — tracked as a known gap).
