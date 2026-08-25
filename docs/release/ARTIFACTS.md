# AdaTP Release Artifacts

## Binary tarballs

Built with [`tools/build/build-release.sh`](../../tools/build/build-release.sh):

```bash
bash tools/build/build-release.sh          # → dist/adatp-server-<ver>-<os>-<arch>.tar.gz
```

Naming: `adatp-server-<version>-<os>-<arch>.tar.gz`
(`os` ∈ linux|darwin, `arch` ∈ amd64|arm64).

Layout inside the tarball:

```
adatp-<version>/
├── bin/
│   ├── adatp-server        # the server (single static-ish binary)
│   ├── adatp-admin         # API-key & stats management CLI
│   └── adatp-cli           # protocol test tool (handshake + login probe)
├── plugins/                # bundled example plugins (echo, moderation)
├── users.json.example      # demo credentials — replace before production
├── .env.example            # configuration template
└── README.md               # quickstart for this artifact
```

## Checksums & verification

Every build appends to `dist/SHA256SUMS`. Verify a download:

```bash
shasum -a 256 -c SHA256SUMS --ignore-missing
```

Publish `SHA256SUMS` next to the tarballs on the GitHub release; consumers
MUST verify before executing.

## Docker image

```bash
docker build -t adatp-server:<version> server/           # single-arch
bash tools/build/docker-buildx.sh adatp-server:<version> # amd64+arm64
```

Image properties: non-root user, `VOLUME /app/data` (SQLite state),
built-in `HEALTHCHECK` via `adatp-server --healthcheck`, `EXPOSE 3000`.
The cargo step runs `--offline` against the vendored crates, so image builds
are reproducible without crates.io access.

## CI-built artifacts

`server/.github/workflows/ci.yml` builds and uploads release binaries for
`ubuntu-latest` and `macos-latest` on every push to `main`/`release/**` —
these are the same binaries the tarball script packages.

## Platform matrix

| Target | Status |
| :-- | :-- |
| linux amd64 / arm64 | built by CI (ubuntu runner) / buildx |
| macOS amd64 / arm64 | built by CI (macos runner) / local script |
| Windows | not produced in v1.0.0 — build from source with `cargo build --release` (untested; report issues) |

## Reproducing a release locally

```bash
git clone https://github.com/Ada-Transfer-Protocol/Server.git && cd Server
cargo build --release --offline
./target/release/adatp-server --healthcheck || true   # binary self-check probe
```

See [`docs/release/V1_VERIFY.md`](./V1_VERIFY.md) for the full release
verification checklist.
