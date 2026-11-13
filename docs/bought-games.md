# Runbook: bought games on open-source engines

Some classics run best on a modern open-source engine with the data files from a bought copy. We copy the data once to the gaming server, the server adds the pinned engine, and every seat copies the finished folder.

```
Steam (one seat) ──copy the data files──▶ server /srv/games/<name> ──server.yml adds the engine──▶ rsync (read-only) ──▶ each seat /games/classic/<name>
```

| Game | Folder | Engine | Data files to copy |
| --- | --- | --- | --- |
| Quake III Arena | `quake3` | [Quake3e](https://github.com/ec-/Quake3e) | `baseq3/pak0.pk3` |
| Star Wars Jedi Knight: Jedi Academy | `jka` | [OpenJK](https://github.com/JACoders/OpenJK) | `GameData/base/assets0.pk3` to `assets3.pk3` → `base/` |
| Serious Sam Classic: The Second Encounter (+ The First Encounter) | `serioussam` | [SeriousSamClassic](https://github.com/tx00100xt/SeriousSamClassic) 1.10.7 | TSE: all `*.gro` and `Help/` → `SamTSE/`; TFE: all `*.gro`, `Help/` and `Levels/` → `SamTFE/` |

The server also downloads the free Quake 3 point release (`pak1.pk3` to `pak8.pk3`). Nothing from a CD key is needed for LAN play.

## Licences

Buy the game on Steam (GOG also works). Decide the licence model per game before it goes to all seats: one copy per seat, or the per-player ownership check Nimtech uses for MW2 and Warcraft III.

## 1. Get the data onto the server

1. On a seat, sign in to Steam with the account that owns the game and install it. Steam puts it in `/games/steam/steamapps/common/`.
2. From the server, copy only the data files into the share (the folder already exists after `server.yml`):

   ```bash
   # Quake III Arena
   rsync -a orchestrator@<seat>:"/games/steam/steamapps/common/Quake 3 Arena/baseq3/pak0.pk3" /srv/games/quake3/baseq3/
   # Jedi Academy
   rsync -a orchestrator@<seat>:"/games/steam/steamapps/common/Jedi Academy/GameData/base/assets[0-3].pk3" /srv/games/jka/base/
   # Serious Sam Classic: The Second Encounter (the First Encounter likewise into SamTFE/, plus Levels/)
   rsync -a orchestrator@<seat>:"/games/steam/steamapps/common/Serious Sam Classic The Second Encounter/"{'*.gro',Help} /srv/games/serioussam/SamTSE/
   ```

3. Uninstall the game from Steam on that seat if it should not stay there.
4. Run `ansible-playbook server.yml`. It finds the data, downloads the pinned engine and adds it. The folder now has `.engine-version`.

Serious Sam saves player profiles and games next to its program, so the launcher starts it through `game-view`, which builds a writable view of the read-only folder in the player's home.

## 2. Seats

Seats copy the game at their next pull, once the server folder has the data, the engine and `.engine-version`. The launcher shows the game when `/games/classic/<name>` exists.

## Update the engine

Change the part's `name`, `url` and `sha256` in `ansible/roles/server_gameshare/defaults/main.yml` (`gameshare_engine_games`), run `server.yml`, and seats pick it up at their next pull.
