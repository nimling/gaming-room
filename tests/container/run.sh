#!/bin/bash
# Runs the seat playbook and the Phase 1 gate tests in a throwaway CachyOS
# container, with a real btrfs /home on a loop device.
#   tests/container/run.sh
# Needs Docker. On Apple Silicon it runs the amd64 image under emulation (slow).
set -euo pipefail

repo=$(cd "$(dirname "$0")/../.." && pwd)
image=${SEAT_TEST_IMAGE:-cachyos/cachyos:latest}

exec docker run --rm --platform linux/amd64 --privileged \
    -v "$repo:/repo:ro" \
    "$image" \
    bash /repo/tests/container/test-seat.sh
