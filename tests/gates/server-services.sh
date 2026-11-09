#!/bin/bash
# Phase 2: the server's shared services answer on the gaming VLAN.
# Run from any machine on the VLAN (a seat, or a laptop on the VLAN):
#   tests/gates/server-services.sh <server-address>
# Checks: Mumble port, Lancache DNS points Steam at the cache, the cache
# answers its heartbeat. The voice and cache-hit gates still need people.
set -uo pipefail

server=${1:?usage: server-services.sh <server-address>}
fail=0
pass() { echo "PASS  $*"; }
bad() { echo "FAIL  $*"; fail=1; }

port_open() {
    if command -v nc >/dev/null; then
        nc -z -w 3 "$1" "$2" 2>/dev/null
    else
        timeout 3 bash -c "exec 3<>/dev/tcp/$1/$2" 2>/dev/null
    fi
}

if port_open "$server" 64738; then
    pass "Mumble answers on TCP 64738"
else
    bad "Mumble does not answer on TCP 64738"
fi

if command -v dig >/dev/null; then
    answer=$(dig +short +time=3 +tries=1 @"$server" lancache.steamcontent.com A | tail -n 1)
    if [[ $answer == "$server" ]]; then
        pass "Lancache DNS sends Steam downloads to the cache ($answer)"
    else
        bad "Lancache DNS answered '${answer:-nothing}' for lancache.steamcontent.com, not $server"
    fi
else
    bad "dig is not installed (dnsutils / bind-tools), cannot check DNS"
fi

headers=$(curl -s -m 5 -o /dev/null -D - "http://$server/lancache-heartbeat" || true)
if grep -qi '^x-lancache-processed-by' <<<"$headers"; then
    pass "Lancache answers its heartbeat"
else
    bad "Lancache heartbeat missing on http://$server/lancache-heartbeat"
fi

exit "$fail"
