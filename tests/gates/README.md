# Gate tests

Each phase ends with a gate. A gate passes only when its test passes on real seats.

| Phase | Gate | Test |
| --- | --- | --- |
| 0 | Server hardware chosen; PC Café go or no-go decided | Manual: recorded in [docs/scope.md](../../docs/scope.md) |
| 1 | A fresh seat is configured in under 30 min, and a second run changes nothing | [`rebuild-time.sh`](rebuild-time.sh), from the server |
| 1 | Logout leaves no login tokens | [`token-leak.sh`](token-leak.sh), on the seat |
| 1 | `player` cannot change the system | [`player-lockdown.sh`](player-lockdown.sh), on the seat |
| 2 | 10 seats in one voice channel | `voice-count.sh` (planned) |
| 2 | Repeat installs come from the cache | `cache-hit.sh` (planned) |
| 3 | Power on to in a UT99 match in under 60 s, zero dialogs | `cold-start.sh` (planned) |
| 4 | Someone outside the project adds a game unaided | Manual |

## Running the Phase 1 gates

From the gaming server:

```bash
tests/gates/rebuild-time.sh seat-01 -u admin --ask-become-pass   # fresh install
ssh seat-01 sudo bash -s < tests/gates/player-lockdown.sh
# On the seat: sign in to Steam and a browser, then press End session. Then:
ssh seat-01 sudo bash -s < tests/gates/token-leak.sh
```

[`tests/container/run.sh`](../container/run.sh) runs the token-leak and lockdown gates in a container on every pull request. Passing there does not replace the run on a real seat.
