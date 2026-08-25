# Contributing to AdaTP

## Ground rules

1. **The spec is normative.** [`SPEC.md`](docs/SPEC.md) indexes the pack under
   [`docs/spec/`](docs/spec/). Code and spec must always match: a change to
   wire behavior lands together with its spec amendment, or not at all.
2. **Never touch `server/vendor/**`.** The Rust workspace builds offline
   against vendored crates. New dependencies are a design decision, not a
   `cargo add` — open an issue first.
3. **No secrets in git.** No `.env`, keys, or tokens. `users.json` contains
   demo credentials only and is documented as such.
4. **Honesty over marketing.** Docs must describe what the code does today.
   Roadmap items are labeled as such.

## Repository layout

This workspace spans several public repos:

| Path | Repo |
| :-- | :-- |
| `server/` | Ada-Transfer-Protocol/Server (server + core + CLIs + canonical docs) |
| `sdks/js` `sdks/node` `sdks/python` `sdks/c` `sdks/php` `sdks/arduino-esp32` | matching SDK repos |

## Before you open a PR

Run the full verification set from the workspace root:

```bash
bash tests/conformance/run.sh      # golden vectors: Rust + Node + Python
bash tests/integration/run.sh      # 4 live end-to-end suites (61 assertions)
cd server && cargo test --workspace --offline
```

If you changed the wire format intentionally:

```bash
cd tests/conformance
node generate_vectors.mjs > vectors/adatp-v1-vectors.json
cp vectors/adatp-v1-vectors.json ../../server/core/tests/vectors.json
# …and update docs/spec/appendix-test-vectors.md + 04-packets.md in the same PR.
```

## Code style

- Rust: rustfmt defaults; no `unwrap()` on network-facing paths; errors are
  explicit (`AuthFailure` codes, close reasons from the registry in
  `docs/spec/appendix-error-codes.md`).
- SDKs: match each language's idiom and the existing public API surface;
  breaking API changes need a major-version discussion.
- Comments state constraints the code can't express — not narration.

## Commits & branches

- Feature branches off `main`; release branches are `release/vX.Y.Z`.
- Conventional, imperative commit subjects ("Add tool rate limiting"), body
  explains *why* when non-obvious.

## Plugins & integrations

Third-party plugins/webhook receivers don't need PRs here — see
[`docs/platform/PLUGIN_DEVELOPMENT.md`](docs/platform/PLUGIN_DEVELOPMENT.md)
and [`docs/platform/WEBHOOK_DEVELOPMENT.md`](docs/platform/WEBHOOK_DEVELOPMENT.md).
Bundled example plugins live in `server/plugins/` and must stay minimal.
