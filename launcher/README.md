# Launcher

The one screen everyone sees. It follows Fluent 2 and puts ease of use first; see [DESIGN.md](DESIGN.md).

Status: Phase 3 MVP. The code builds and its tests pass in CI. It has not run on a seat yet.

## How it fits together

| Part | Folder | What it does |
| --- | --- | --- |
| Screen | [`app/ui/index.html`](app/ui/index.html) | The Fluent screen. Open it in a browser and it is the clickable prototype with sample data. Inside the app it shows the real seat. |
| App | [`app/src-tauri/`](app/src-tauri/) | Tauri window, full screen. Gives the screen the seat, the games and the power to start them. |
| Core | [`crates/arcade-core/`](crates/arcade-core/) | Reads manifests and seat files, picks the tier's settings, writes settings files, builds the command. No side effects until `apply` and `spawn`. |
| CLI | [`crates/arcade-cli/`](crates/arcade-cli/) | The `arcade` command. Everything the screen does, from a terminal. |

Starting a game: the core reads `games/<id>.yaml` and `/etc/seat/tier`, picks that tier's resolution and frame cap, writes the nickname and settings into the game's own files, and runs:

```text
[gamemoderun] [gamescope … --] [umu-run] <game> <args> [join or host args]
```

## The arcade command

```bash
arcade list                                   # games this seat shows
arcade validate                               # check every manifest
arcade plan ut99 --profile C                  # what a launch would do
arcade launch ut99 --join 192.0.2.20:7777     # start and join
arcade launch ut99 --screenshot-after 20      # test mode: screenshot, then close
arcade nick Kaptein                           # set the nickname
arcade end-session                            # reset the seat
```

## Seat files it reads

| File | Written by | Used for |
| --- | --- | --- |
| `/etc/seat/tier` | `seat-tier` (Phase 1) | Settings per tier |
| `/etc/seat/hardware.json` | `seat-tier` | Graphics card shown in the seat panel |
| `/etc/seat/display` | Not yet; Phase 3 Ansible role | Screen size, for example `2560x1440@144`. Default 1920x1080 at 60 Hz |
| `/etc/arcade/arcade.yaml` | Phase 3 Ansible role | `games_dir`, pinned `proton` path, `voice.server` |
| `~/.config/arcade/nickname` | The launcher | Wiped by the seat reset |

## Develop and test

```bash
launcher/scripts/check-in-docker.sh   # on a Mac: same checks as CI, in Docker
launcher/scripts/check.sh             # on Linux with Rust and the Tauri libraries
```

Run the app off a seat against the test seat:

```bash
cd launcher && ARCADE_ROOT=tests/seat-root ARCADE_GAMES=../games ARCADE_WINDOWED=1 cargo run -p arcade-app
```

## Not done yet

- Ansible role that installs the app, writes `/etc/arcade/arcade.yaml` and `/etc/seat/display`, and starts it at login.
- LAN game list service on the server (the screen shows no LAN games until then).
- Live voice status from Mumble (the screen starts Mumble and joins the channel).
- Testing each manifest on the canary seat.
