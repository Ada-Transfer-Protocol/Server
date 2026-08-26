# AdaTP codec fuzzing

[`cargo-fuzz`](https://rust-fuzz.github.io/book/cargo-fuzz.html) harness for the
AdaTP wire-format decoder in the `adatp-core` crate. The decoder
(`Packet::from_bytes`, `core/src/codec/packet.rs`) parses every inbound frame
on a live WebSocket connection, so it is the highest-value fuzz surface in the
server.

## Targets

| Target | What it checks |
| :-- | :-- |
| `frame_parser` | `Packet::from_bytes` never panics / over-reads / over-allocates on arbitrary bytes — it only returns `Ok(Packet)` or `Err`. |
| `frame_roundtrip` | Any frame the decoder accepts survives `to_bytes()` → `from_bytes()` unchanged (encoder/decoder symmetry). |

A small seed corpus of valid frames lives under `corpus/<target>/` and is
committed to bootstrap coverage.

## Prerequisites

`cargo-fuzz` is a nightly-only tool built on libFuzzer:

```bash
rustup toolchain install nightly
cargo install cargo-fuzz          # or: cargo binstall cargo-fuzz
```

> `libfuzzer-sys` is **not** vendored (it is a dev-only dependency). The
> `.cargo/config.toml` in this directory points the crates-io source back at
> the live registry so it can be fetched, overriding the workspace's offline
> vendored source. Fuzz builds therefore need network access; the main
> `cargo build --offline` release build is unaffected.
>
> Cargo discovers config from the working directory upward, so the override
> above applies when the fuzz build runs from here. If you invoke
> `cargo +nightly fuzz run` from the repository root and it fails to fetch
> `libfuzzer-sys` (because the root's offline vendored config is picked up
> instead), temporarily move that config aside — this is exactly what CI does:
>
> ```bash
> mv ../.cargo/config.toml ../.cargo/config.toml.disabled
> cargo +nightly fuzz run frame_parser
> mv ../.cargo/config.toml.disabled ../.cargo/config.toml
> ```

## Run

```bash
# From the repository root or this directory:
cargo +nightly fuzz run frame_parser
cargo +nightly fuzz run frame_roundtrip

# Bounded smoke run (what CI does):
cargo +nightly fuzz run frame_parser -- -max_total_time=30

# List targets:
cargo +nightly fuzz list
```

Crashing inputs are written to `artifacts/<target>/`; reproduce one with:

```bash
cargo +nightly fuzz run frame_parser artifacts/frame_parser/crash-<hash>
```
