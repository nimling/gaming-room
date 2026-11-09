#!/bin/bash
# Gate 1: a freshly installed seat is fully configured in under 30 minutes.
# Procedure: reinstall CachyOS on the canary seat (default btrfs layout,
# KDE Plasma, one user named admin). Then on the gaming server:
#   tests/gates/rebuild-time.sh seat-01 -u admin --ask-become-pass
# The first run uses the installer's admin account; it installs the
# orchestrator key, so later runs need no extra arguments.
set -uo pipefail

if [[ $# -lt 1 ]]; then
    echo "usage: $0 <seat> [extra ansible-playbook args]" >&2
    exit 2
fi
seat=$1
shift
repo=$(cd "$(dirname "$0")/../.." && pwd)
limit_minutes=30

cd "$repo/ansible" || exit 1
start=$(date +%s)
if ! ansible-playbook seat.yml -l "$seat" "$@"; then
    echo "FAIL  playbook failed on $seat"
    exit 1
fi
minutes=$(( ($(date +%s) - start + 59) / 60 ))

if (( minutes <= limit_minutes )); then
    echo "PASS  $seat configured in $minutes min (limit $limit_minutes)"
else
    echo "FAIL  $seat took $minutes min (limit $limit_minutes)"
    exit 1
fi

# A second run must change nothing, or the seats will drift.
if ansible-playbook seat.yml -l "$seat" | tee /dev/stderr | grep -Eq "changed=0 +unreachable=0 +failed=0"; then
    echo "PASS  second run changed nothing"
else
    echo "FAIL  second run changed something; the playbook is not idempotent"
    exit 1
fi
