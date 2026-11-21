# Runbook: World of Warcraft: Forever

WoW Forever has no Linux version. It runs through Battle.net on Proton. Each player signs in with their own Blizzard account; a WoW subscription includes Forever.

```
Battle.net PC ──copy by hand──▶ server /srv/games/wow ──rsync, once──▶ each seat /games/battlenet/wow
                                                                           │
                                       Battle.net patches it here ◀────────┘ (through Lancache)

canary golden home ──battlenet-prefix-publish──▶ server /srv/games/battlenet/prefix
  (Battle.net, signed out)                         │
                                                   └──each pull──▶ each seat's golden home ──reset──▶ live home
```

## What stays and what goes

| Data | Where it lives | After End session |
| --- | --- | --- |
| WoW game files (about 50 GB) | `/games/battlenet/wow` | Kept; Battle.net patches them |
| Battle.net, its saved accounts, cookies and "Keep me logged in" token | Wine prefix `~/.local/share/arcade/prefixes/battlenet` | Wiped (home reset); the golden home's signed-out copy comes back |
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

Set Battle.net up by hand once, on the canary. Then publish it. Every seat gets it at its next pull, and the player gets it at the next reset.

At the canary, in the player's session:

1. Run `battlenet-seat --install`. It downloads Battle.net's installer and runs it in the prefix.
2. Sign in to Battle.net with any account that has WoW. Choose WoW Forever, then **Locate the game** and pick `Z:\games\battlenet\wow`. Let Battle.net check the files.
3. Sign out of Battle.net (account menu, **Log Out**). Close it.
4. As admin over SSH: `sudo seat-golden-capture`. It refuses while any Battle.net or WoW login is left.

Right after that, on the server (a person, or the Claude session there):

```bash
sudo battlenet-prefix-publish seat-01
```

- It copies `/home/.golden/player/.local/share/arcade/prefixes/battlenet` from the canary. It logs in as `orchestrator` with the server's Claude key, the same path Ansible uses. No new access.
- It refuses when the copy holds a login from `token-paths`. That is the same check and list as golden capture and gate 1 (`seat-token-check`). Sign out on the canary, capture, and publish again.
- It leaves out Battle.net's logs, its web caches (`BrowserCaches`, `Cache`), Wine's temp folders and crash dumps. Battle.net makes them again; they would only make every reset slower.
- It writes `/srv/games/battlenet/prefix/.prefix-version` (time and source) last, and removes it first. Seats take only a prefix with that file, so never a half copy.
- It gives the canary's golden home the same version, so the canary keeps its own copy. Publish right after the capture: until then, the canary's next pull would replace a new setup with the older published one.

On every seat, `seat.yml` runs `battlenet-prefix-pull` (`roles/games`, `tasks/battlenet.yml`). When the share's version differs from the one in the golden home, it copies the prefix to a staging folder, checks that the version did not change meanwhile, and swaps it in with one rename. On ext4 the staging folder is next to the golden home, not in it, because `seat-reset` copies the golden home with `cp`. It never touches the live home. Same version, no prefix on the share, or no golden home yet: no change. A new seat gets it at its second pull, because `roles/session` makes the golden home after `roles/games`.

### What is the same on every seat

Proton runs every prefix as the Windows user `steamuser`, the home is `/home/player` on every seat, and the Proton path is pinned (`/games/runtime`). So the paths in the prefix fit every seat.

- Wine stores the computer name in `system.reg`, so the published copy has the canary's. Wine sets it again from the seat's own hostname at every start. The publish prints where it found the seat's name.
- Machine IDs that Wine or Battle.net make once per prefix are the same on every seat. Each player signs in to their own account, so this looks like one PC to Blizzard. Not checked yet; see the open points.

### Refresh now and then

Battle.net updates itself after each reset, in the live home, and the reset throws the update away. The download comes from Lancache, so it is quick, but it happens every session. After a big Battle.net update, run it on the canary, sign out, capture and publish again. Then seats start with the new version.

## 4. Test on the canary

```bash
sudo -u player battlenet-seat --check
```

Then:

- [ ] `sudo battlenet-prefix-publish seat-01` on the server says "published"; on a canary with a login left, it refuses
- [ ] After the merge, on a seat whose GPU differs from the canary's, after its pull and a reset: `sudo -u player battlenet-seat --check` passes with no setup by hand
- [ ] On that seat, Battle.net starts signed out and finds WoW Forever without a download
- [ ] The tile starts Battle.net; WoW Forever is found and does not download again
- [ ] Sign in with "Keep me logged in" ticked, play, press End session
- [ ] `ssh seat-01 sudo bash -s < tests/gates/token-leak.sh` passes
- [ ] Battle.net asks for the full sign-in again, with no saved account name
- [ ] A patch is applied in place, and the next seat gets it from Lancache
- [ ] Tier C seat: playable frame rate

Still open: whether Battle.net or Blizzard mind that all seats share the prefix's machine IDs (look for repeated security mails or device prompts), and how large the published prefix is (each reset copies it). Also open: the folder name of Forever's version folder (`_forever_` is a guess; the scripts handle any `_name_` folder), and its Battle.net launch code. With the code, add `--game <code>` to the manifest, so WoW starts right after sign-in.
