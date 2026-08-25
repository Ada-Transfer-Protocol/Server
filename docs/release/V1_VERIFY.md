# AdaTP v1.0.0 — Release Verification Checklist

Run every step from the workspace root. A release may be tagged only when
all boxes check.

## 1. Build

```bash
(cd server && cargo build --release --offline --workspace)
```

- [ ] Builds with zero errors and zero warnings on macOS and Linux
- [ ] `target/release/{adatp-server, adatp-admin, adatp-cli}` all produced

## 2. Conformance (golden vectors, 3 implementations)

```bash
bash tests/conformance/run.sh
```

- [ ] Rust: 9/9 tests pass (`cargo test -p adatp-core`)
- [ ] Node.js runner prints `PASS — Node.js SDK conforms`
- [ ] Python runner prints `PASS — Python SDK conforms`

## 3. Integration (live end-to-end)

```bash
bash tests/integration/run.sh
```

- [ ] `PASS` × 4 suites, exit code 0 (61 assertions):
      plaintext, secure session (incl. GameState), plugins/tools,
      admin/webhooks (incl. HMAC verification, kick, drain, Silo)

## 4. Protocol probe

```bash
(cd server && cargo run --release -p adatp-cli -- -a 127.0.0.1:3000 -u user1 -p password123)
```

- [ ] Prints `Secure session established 🔒` and `✅ Login OK`

## 5. Silo Panel live proof

1. Start the server with a known `ADMIN_TOKEN`.
2. Open `http://127.0.0.1:3000/silo`, authorize, keep OVERVIEW visible.
3. Run the probe from step 4 in a second terminal.

- [ ] ACTIVE CONNECTIONS increments while the probe runs, returns after
- [ ] LOGS tab streams the auth/join lines live

## 6. Load sanity

```bash
(cd tools/loadtest && node loadtest.mjs --url ws://127.0.0.1:3000/ws \
    --clients 40 --rooms 4 --rate 10 --duration 8)
```

- [ ] `LOAD TEST PASS`, zero socket errors, p99 latency single-digit ms on
      localhost release build

## 7. Artifacts

```bash
bash tools/build/build-release.sh
shasum -a 256 -c dist/SHA256SUMS
```

- [ ] Tarball produced with `bin/`, `plugins/`, examples; checksums verify
- [ ] (If Docker available) `docker build server/` succeeds; container turns
      healthy; `/healthz` 200

## 8. Documentation integrity

- [ ] `SPEC.md` links resolve; `docs/spec/04-packets.md` type table matches
      `server/core/src/codec/packet.rs`
- [ ] Version reads 1.0.0 in: 3× Cargo.toml, node/js package.json,
      pyproject.toml, library.properties
- [ ] `CHANGELOG.md` has the 1.0.0 section with today's date
- [ ] No `8443`/`8444` references outside legacy notes
      (`grep -rn "8444" --include="*.md" . | grep -v legacy` is empty of
      non-legacy hits)

## 9. Hygiene

```bash
git -C server status --porcelain   # and each sdks/<name>
```

- [ ] No secrets, no `.DS_Store`, no `target/`, `node_modules/`, `dist/`,
      `*.db` staged
- [ ] Each repo on branch `release/v1.0.0` with a clean working tree after
      commit

Sign-off: tagging `v1.0.0` is authorized only when every box above is
checked by a human on the release machine. Otherwise push the branch and
leave the tag for later (see PUBLISH.md).
