# AdaTP server — multi-stage build.
# The workspace vendors all crates (vendor/ + .cargo/config.toml), so the
# cargo build runs fully offline; only the base images need network.
#
#   docker build -t adatp-server .
#   docker run -p 3000:3000 adatp-server

FROM rust:1.83-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY . .
RUN cargo build --release --offline --bin adatp-server

# ---------------------------------------------------------------------------
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --home-dir /app adatp

WORKDIR /app
COPY --from=builder /build/target/release/adatp-server /usr/local/bin/adatp-server
COPY --chown=adatp server/users.json /app/users.json

USER adatp
ENV HOST=0.0.0.0 \
    PORT=3000 \
    AUTH_DRIVER=file \
    AUTH_FILE_PATH=/app/users.json \
    DATABASE_URL=sqlite:/app/data/adatp.db \
    RUST_LOG=info

RUN mkdir -p /app/data
VOLUME ["/app/data"]
EXPOSE 3000

HEALTHCHECK --interval=15s --timeout=3s --start-period=5s --retries=3 \
    CMD ["adatp-server", "--healthcheck"]

CMD ["adatp-server"]
