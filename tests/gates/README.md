# Gate tests

Each phase ends with a gate. A gate passes only when its test passes on real seats. Scripts land here as each phase is built.

| Phase | Gate | Test (planned) |
| --- | --- | --- |
| 0 | Server hardware chosen; PC Café go or no-go decided | Manual: recorded in [docs/scope.md](../../docs/scope.md) |
| 1 | A wiped seat rebuilds in under 30 min | `rebuild-time.sh`: wipe the canary seat, run the playbook, time it |
| 1 | Logout leaves no tokens | `token-leak.sh`: sign in, end session, search for known token files |
| 1 | `player` cannot change the system | `player-lockdown.sh`: try `sudo` and `pacman -S` as `player` |
| 2 | 10 seats in one voice channel | `voice-count.sh`: read user count from the Mumble server |
| 2 | Repeat installs come from the cache | `cache-hit.sh`: look for HITs in the Lancache logs |
| 3 | Power on to in a UT99 match in under 60 s, zero dialogs | `cold-start.sh`: launcher test mode plus a screenshot |
| 4 | Someone outside the project adds a game unaided | Manual |
