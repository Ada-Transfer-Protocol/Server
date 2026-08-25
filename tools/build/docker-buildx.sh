#!/usr/bin/env bash
# Multi-architecture Docker image build for the AdaTP server.
#
#   bash tools/build/docker-buildx.sh [image-tag]
#
# Requires a running Docker daemon with buildx (Docker Desktop ships it).
# Produces linux/amd64 + linux/arm64 images. Add --push to publish.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TAG="${1:-adatp-server:latest}"

if ! docker info >/dev/null 2>&1; then
    echo "ERROR: Docker daemon is not running." >&2
    exit 1
fi

docker buildx create --name adatp-builder --use 2>/dev/null || docker buildx use adatp-builder

exec docker buildx build \
    --platform linux/amd64,linux/arm64 \
    -t "$TAG" \
    "${EXTRA_ARGS:---load}" \
    "$ROOT/server"
