#!/bin/bash
# Runs check.sh in a Debian container with Rust and the Tauri libraries,
# so a Mac without Rust can run the same checks as CI.
# Cargo downloads and build output stay in Docker volumes between runs.
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)

docker run --rm \
    -v "$repo":/repo -w /repo/launcher \
    -v arcade-cargo-registry:/usr/local/cargo/registry \
    -v arcade-target:/repo/launcher/target \
    rust:1-bookworm bash -c '
        set -e
        apt-get update -qq
        apt-get install -y -qq --no-install-recommends \
            libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev \
            libsoup-3.0-dev libjavascriptcoregtk-4.1-dev >/dev/null
        rustup component add rustfmt clippy >/dev/null 2>&1
        [ -f Cargo.lock ] || cargo generate-lockfile
        scripts/check.sh
    '
