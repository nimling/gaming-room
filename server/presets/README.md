# Game server presets

One file per server type the gaming server can run on demand. Players start a preset from the launcher (or the control service's API); it stops by itself after a while without players, unless it is `always_on`. Adding a preset is a pull request with one file here.

The server runs each preset as its own service (`gs-<id>.service`, account `gameserver`, settings in `/var/lib/gameservers/<id>`), from the same game files the seats have in the share. A preset only appears once its game is on the server (`needs`).

## Keys

| Key | Required | Meaning |
| --- | --- | --- |
| `id` | yes | Lowercase letters, digits and `-`. Also the file name. |
| `name` | yes | What the launcher shows. |
| `game` | yes | The launcher game id, for the Join button (`ut99`, `ut2004`, ...). |
| `needs` | yes | A file under `/srv/games` that must exist, for example `ut2004/System/ucc-bin`. |
| `port` | yes | Game port. Pick one no other preset uses. |
| `query` | yes | How the control service counts players: `{ type: ut99, port }` (GameSpy), `{ type: ut2004, port }` or `{ type: quake3, port }`. |
| `firewall_udp` | yes | UDP ports or ranges to open to the gaming VLAN. |
| `workdir` | yes | Folder under `/srv/games` to start in. |
| `command` | yes | Program and arguments. `{home}` is the preset's settings folder. |
| `seed` | | Files copied once from `/srv/games` into the settings folder: `{ from, to }`. |
| `ini` | | Keys set in a settings file on every run: `{ file, set: { section: { key: value } } }`. A list value sets the key once per item (for keys like `ServerPackages`). |
| `always_on` | | `true`: runs all the time, never stopped for being empty. |
