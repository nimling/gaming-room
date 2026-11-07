# Games

One YAML file per game. Adding a game is a pull request with one file here (plus any settings templates); the launcher needs no code change.

The format is a draft and will be fixed in Phase 3. See [`ut99.yaml`](ut99.yaml) for the current shape.

| Key | Meaning |
| --- | --- |
| `id` | Short unique name, also the file name |
| `name` | What the launcher shows |
| `runtime` | `native`, `proton`, `openra` or `dosbox` |
| `path` | Install folder under `/games` |
| `players` | Supported player count |
| `launch.direct` | Command that starts the game straight into its menu or a server |
| `join` | How to join a server; `{server}` is filled in by the launcher |
| `config` | Settings templates rendered at every launch, with the nickname and tier |
| `profiles` | Settings per hardware tier (A, B, C) |
| `wrappers` | Extra runners, for example `gamemoderun` or `gamescope` |

## Before merging a game

- [ ] Starts with no dialogs on a tier C seat
- [ ] Nickname shows up in game
- [ ] 10-player LAN match tested, or the game's maximum
- [ ] License allows play on several seats
