# Games

One YAML file per game. Adding a game is a pull request with one file here (plus any settings templates); the launcher needs no code change.

Check your file before you open the pull request:

```bash
cd launcher && cargo run -q -p arcade-cli -- --games ../games validate
cargo run -q -p arcade-cli -- --games ../games plan ut99 --profile C --nick Tester
```

`plan` shows the settings files the launcher writes and the exact command it runs, without running anything.

## Keys

| Key | Required | Meaning |
| --- | --- | --- |
| `id` | yes | Lowercase letters, digits and `-`. Also the file name. |
| `name` | yes | What the launcher shows. |
| `subtitle` | | One line under the name. |
| `group` | | `classic` (default) or `steam`. The launcher's filter tabs use it. |
| `runtime` | yes | `native`, `proton` (Windows game through `umu-run`) or `steam`. |
| `engine` | | Shown on the screen, for example `OldUnreal 469e`. |
| `path` | | Install folder under `/games`. Leave out for system packages. The game shows only if the folder (or the command) exists. |
| `players` | yes, except Steam | `{ min: 2, max: 16 }` |
| `launch.command` | yes | Program and fixed arguments. |
| `launch.args` | | More arguments; templates allowed. |
| `launch.workdir` | | Work folder, relative to `path`. |
| `launch.env` | | Environment variables; templates allowed. |
| `join.args` | | Arguments to join a server. Must use `{{ server }}`. Without `join` the screen has no Join button. |
| `host.args`, `host.map` | | Arguments to host a LAN game. Without `host` the screen has no Host button. |
| `config` | | Settings files written before every start. See below. |
| `profiles` | yes, except Steam | Per tier A, B and C: `resolution` (`native` or `1920x1080`) and `fps_cap` (`refresh` or a number). |
| `gamescope` | | `{ width: 640, height: 480 }` runs the game in gamescope with sharp integer scaling. |
| `gamemode` | | `true` by default. Starts the game with `gamemoderun`. |
| `lowest_tier` | | Weakest tier the game runs well on. Weaker seats do not show it. |
| `art` | | Placeholder art: `{ hue, glow, motif }`. Motifs: rings, grid, hex, contour, streaks, dots. |
| `why` | | Why the game is good in a LAN room: 1 to 3 short sentences, at most 400 characters. Shown in the game details view. |
| `highlights` | | Up to 5 short points for the details view, each at most 80 characters. |
| `ingame` | | Up to 6 tips for the "Playing now" panel the launcher shows while the game runs, for example how to vote for a map. Each tip has `title` (at most 60 characters), `text` (at most 200) and optional `keys`, pressed together and shown as keycaps. See below. |

## In-game tips

When a player switches back to the launcher while a game runs, the launcher shows a "Playing now" panel with these tips.

```yaml
ingame:
  - title: Vote for the next map or mode
    keys: ["F7"]
    text: Opens the vote menu. It opens by itself at the end of each match.
  - title: Without the key
    text: "Press ~ (console) and type: mutate votemenu"
```

Keep each tip to one or two short sentences. Quote a text that has `: ` in it.

## Templates

Strings in `launch`, `join`, `host` and `config` can use these values, written as `{{ name }}`:

`nickname`, `tier`, `width`, `height`, `fps_cap`, `refresh`, `server` (join only), `map`, `id`, `path`, `home`.

A misspelled name is an error, not an empty string.

## Settings files

```yaml
config:
  # Set keys in an INI file and keep everything else in it
  - ini: "~/.utpg/System/User.ini"
    set:
      DefaultPlayer:
        Name: "{{ nickname }}"
  # Or write a whole file from a template in this folder
  - template: mygame/settings.cfg.j2
    to: "~/.config/mygame/settings.cfg"
```

## Before merging a game

- [ ] `arcade validate` passes
- [ ] Starts with no dialogs on a tier C seat
- [ ] Nickname shows up in game
- [ ] 10-player LAN match tested, or the game's maximum
- [ ] License allows play on several seats
