#!/bin/bash
# Gate 1: the player account cannot change the system.
# Run on a seat as root, for example from the server:
#   ssh seat-01 sudo bash -s < tests/gates/player-lockdown.sh
set -uo pipefail

# shellcheck source=/dev/null
source /etc/seat/seat.conf
fail=0

pass() { echo "PASS  $1"; }
flunk() { echo "FAIL  $1"; fail=1; }
as_player() { runuser -u "$SEAT_PLAYER" -- "$@"; }

if id -nG "$SEAT_PLAYER" | tr ' ' '\n' | grep -qx wheel; then
    flunk "player is in wheel"
else
    pass "player is not in wheel"
fi

if as_player sudo -n true >/dev/null 2>&1; then
    flunk "player can use sudo"
else
    pass "sudo is refused"
fi

if as_player pacman -S --noconfirm --needed pacman >/dev/null 2>&1; then
    flunk "player can install packages"
else
    pass "package install is refused"
fi

if [[ $(passwd -S "$SEAT_PLAYER" | awk '{print $2}') == L ]]; then
    pass "player password is locked"
else
    flunk "player password is not locked"
fi

if grep -rqs -- "^${SEAT_PLAYER}[[:space:]]" /etc/sudoers /etc/sudoers.d; then
    flunk "a sudoers entry names the player"
else
    pass "no sudoers entry for the player"
fi

if [[ -f /etc/polkit-1/rules.d/49-gaming-room.rules ]]; then
    pass "polkit rule is installed"
else
    flunk "polkit rule is missing"
fi

exit "$fail"
