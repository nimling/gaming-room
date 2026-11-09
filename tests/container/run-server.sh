#!/bin/bash
# Lints and applies the server playbook in a throwaway Ubuntu container.
#   tests/container/run-server.sh
# Set SERVER_TEST_IMAGE to try another release, for example ubuntu:26.04.
# Pass an output folder to keep the rendered compose files:
#   tests/container/run-server.sh /tmp/server-out
set -euo pipefail

repo=$(cd "$(dirname "$0")/../.." && pwd)
image=${SERVER_TEST_IMAGE:-ubuntu:24.04}
out=()
if [[ $# -gt 0 ]]; then
    mkdir -p "$1"
    out=(-v "$(cd "$1" && pwd):/out")
fi

exec docker run --rm \
    -v "$repo:/repo:ro" \
    ${out[@]+"${out[@]}"} \
    "$image" \
    bash /repo/tests/container/test-server.sh
