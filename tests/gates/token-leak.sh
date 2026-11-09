#!/bin/bash
# Gate 1: logout leaves no login tokens.
# Procedure on the seat: sign in to Steam and a browser, press End session.
# Then run as root, for example from the server:
#   ssh seat-01 sudo bash -s < tests/gates/token-leak.sh
# Add --reset to trigger the reset from here instead of pressing End session:
#   ssh seat-01 sudo bash -s -- --reset < tests/gates/token-leak.sh
set -uo pipefail

# shellcheck source=/dev/null
source /etc/seat/seat.conf
marker=/run/seat-last-reset
fail=0

if [[ ${1:-} == --reset ]]; then
    if [[ -d /run/systemd/system ]]; then
        systemctl start seat-reset.service
    else
        /usr/local/sbin/seat-reset session
    fi
fi

if [[ ! -f $marker ]]; then
    echo "FAIL  no reset has run since boot ($marker missing)"
    exit 1
fi
echo "INFO  last reset: $(cat "$marker")"

if /usr/local/sbin/seat-token-check "$SEAT_HOME"; then
    echo "PASS  no login tokens in $SEAT_HOME"
else
    echo "FAIL  login tokens survived the reset"
    fail=1
fi

# Proton prefixes hold saves and sometimes logins; none may predate the reset.
compatdata="$SEAT_STEAM_LIBRARY/steamapps/compatdata"
if [[ -d $compatdata ]] && [[ -n $(find "$compatdata" -mindepth 1 -maxdepth 1 ! -newer "$marker") ]]; then
    echo "FAIL  Proton prefixes from before the reset are still in $compatdata"
    fail=1
else
    echo "PASS  no Proton prefixes from before the reset"
fi

exit "$fail"
