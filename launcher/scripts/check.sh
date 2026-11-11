#!/bin/bash
# Lints, tests and builds the launcher, and checks every game manifest.
# CI runs this; check-in-docker.sh runs it on a Mac.
set -euo pipefail
cd "$(dirname "$0")/.."

step() { printf '\n=== %s ===\n' "$*"; }

step "Format"
cargo fmt --all -- --check

step "Lint"
cargo clippy --workspace --all-targets --locked -- -D warnings

step "Unit tests"
cargo test --workspace --locked

step "Game manifests"
arcade() { cargo run -q --locked -p arcade-cli -- "$@"; }
arcade --games ../games validate

step "Plans on the test seat (tier B, 2560x1440)"
export ARCADE_ROOT=tests/seat-root
arcade --games ../games list --all
arcade --games ../games plan ut99 --nick Tester
arcade --games ../games plan ut99 --nick Tester --profile C --join 192.0.2.20:7777
arcade --games ../games plan openra-ra --nick Tester
arcade ips

step "Build the app"
cargo build --locked -p arcade-app

printf '\nALL LAUNCHER CHECKS PASSED\n'
