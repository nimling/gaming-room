# Runbook: bought games from GOG installers

GOG's offline installers are DRM-free. The server unpacks them with `innoextract` (installed with TrackMania Nations Forever), so no Windows or GOG Galaxy is needed. The data files then go into the share exactly as from a Steam copy.

```
GOG offline installers ──▶ server /srv/game-installers ──innoextract──▶ data files ──▶ /srv/games/<game> ──server.yml adds the engine──▶ seats
```

## 1. Download the installers

1. On gog.com, sign in, go to **Library**, open the game, choose **Download offline backup game installers**, Windows, English.
2. Download every file of the installer (large games come as one `.exe` plus one or more `.bin` files).
3. Get the files onto the server, into `/srv/game-installers/` (create it once, owned by the login user, mode 700). It must be outside `/srv/games`: the game share serves everything there to the whole VLAN. From a seat, the server session copies them with `rsync`; from elsewhere, any way that reaches the server (USB, `scp`).

## 2. Unpack and place the data

On the server, for each game:

```bash
mkdir -p /tmp/gog/<game> && innoextract -s -d /tmp/gog/<game> /srv/game-installers/setup_<game>_*.exe
```

Then copy only the data files. Find them by name; GOG's folder layout varies between installer versions.

| Game | Find | Copy into |
| --- | --- | --- |
| Quake III Arena | the `baseq3/` folder with `pak0.pk3` | `/srv/games/quake3/baseq3/pak0.pk3` (Team Arena, if present: `missionpack/pak0.pk3` → `/srv/games/quake3/missionpack/`) |
| Jedi Academy | the `base/` folder with `assets0.pk3` | `assets0.pk3` to `assets3.pk3` → `/srv/games/jka/base/` |
| Worms Armageddon | the folder with `WA.exe` | the whole folder → `/srv/games/worms/` |
| Serious Sam: The Second Encounter | the folder with `SE1_00.gro` | all `*.gro` and `Help/` → `/srv/games/serioussam/SamTSE/` |
| Serious Sam: The First Encounter | the folder with `1_00c.gro` | all `*.gro`, `Help/` and `Levels/` → `/srv/games/serioussam/SamTFE/` |

```bash
find /tmp/gog/<game> -iname pak0.pk3        # or assets0.pk3, WA.exe, SE1_00.gro
chmod -R u+rwX,go+rX /srv/games/<game>
rm -rf /tmp/gog/<game>
```

Keep the installers in `/srv/game-installers/` in case the share has to be rebuilt.

## 3. Finish

Run `ansible-playbook server.yml`. It adds the engines (Quake 3, Jedi Academy, Serious Sam) and the seats copy the games at their next pull. Then test each game on the canary seat. See [bought-games.md](bought-games.md) and [worms.md](worms.md) for the rest.
