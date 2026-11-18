# Runbook: World of Warcraft: Forever

WoW Forever has no Linux version. It runs through Battle.net on Proton. Each player signs in with their own Blizzard account; a WoW subscription includes Forever.

```
Battle.net PC ──copy by hand──▶ server /srv/games/wow ──rsync, once──▶ each seat /games/battlenet/wow
                                                                           │
                                       Battle.net patches it here ◀────────┘ (through Lancache)
```

## What stays and what goes

| Data | Where it lives | After End session |
| --- | --- | --- |
| WoW game files (about 50 GB) | `/games/battlenet/wow` | Kept; Battle.net patches them |
| Battle.net, its saved accounts, cookies and "Keep me logged in" token | Wine prefix `~/.local/share/arcade/prefixes/battlenet` | Wiped (home reset) |
| WoW account name, settings, addons, chat cache, logs, screenshots | `WTF`, `Interface`, `Cache`, `Logs`, `Errors`, `Screenshots` in each version folder, linked to `~/.local/share/arcade/wow/` | Wiped (home reset) |

Characters live on Blizzard's servers, so nothing a player needs is lost.

The personal folders in `/games` are links into the home. `wow-links` runs after every reset and puts back any link that Battle.net or WoW replaced with a real folder, and deletes that folder. Gate 1 (`tests/gates/token-leak.sh`) fails if one is not a link.

## 1. Get WoW onto the server

1. On a PC with Battle.net, install **World of Warcraft: Forever**. Start it once and log out.
2. Copy the `World of Warcraft` folder to the server, without the personal folders:

   ```bash
   rsync -a --info=progress2 \
     --exclude='/_*_/WTF' --exclude='/_*_/Interface' --exclude='/_*_/Cache' \
     --exclude='/_*_/Logs' --exclude='/_*_/Errors' --exclude='/_*_/Screenshots' \
     "/path/to/World of Warcraft/" <login-user>@<server>:/srv/games/wow/
   ```

   From Windows, delete those folders in the copy, then copy it with WinSCP. Seats never copy them anyway.
3. Run the server playbook, so the files are readable for the share: `ansible-playbook server.yml`.

The copy only seeds new seats. It may fall behind; Battle.net patches a seat after the first sign-in. Refresh it after big patches, so a new seat has less to download.

## 2. Copy it to the canary

```bash
ansible-playbook seat.yml -l canary --check --diff
ansible-playbook seat.yml -l canary
```

A seat copies WoW only when it has none (`/games/battlenet/wow/.build.info` missing). Later runs leave it alone, so they never undo a patch. To copy it again, delete the folder on the seat.

## 3. Battle.net in the golden home

At the canary, in the player's session:

1. Run `battlenet-seat --install`. It downloads Battle.net's installer and runs it in the prefix.
2. Sign in to Battle.net with any account that has WoW. Choose WoW Forever, then **Locate the game** and pick `Z:\games\battlenet\wow`. Let Battle.net check the files.
3. Sign out of Battle.net (account menu, **Log Out**). Close it.
4. As admin over SSH: `sudo seat-golden-capture`. It refuses while any Battle.net or WoW login is left.

## 4. Test on the canary

```bash
sudo -u player battlenet-seat --check
```

Then:

- [ ] The tile starts Battle.net; WoW Forever is found and does not download again
- [ ] Sign in with "Keep me logged in" ticked, play, press End session
- [ ] `ssh seat-01 sudo bash -s < tests/gates/token-leak.sh` passes
- [ ] Battle.net asks for the full sign-in again, with no saved account name
- [ ] A patch is applied in place, and the next seat gets it from Lancache
- [ ] Tier C seat: playable frame rate

Still open: the folder name of Forever's version folder (`_forever_` is a guess; the scripts handle any `_name_` folder), and its Battle.net launch code. With the code, add `--game <code>` to the manifest, so WoW starts right after sign-in.
