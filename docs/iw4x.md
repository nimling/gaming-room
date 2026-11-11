# Runbook: Modern Warfare 2 (2009) with IW4x

IW4x is a community client for MW2 multiplayer. It needs the original game files. We copy MW2 once to the gaming server, the server adds IW4x on top, and every seat copies the finished folder from there.

```
Steam PC ──copy by hand──▶ server /srv/games/mw2 ──server.yml adds IW4x──▶ rsync (read-only) ──▶ each seat /games/classic/mw2
```

## Before you start

- MW2 from Steam only. IW4x does not support the Microsoft Store version or other copies.
- Licences: one owned copy per seat that plays. Check this with WP6 before game night.
- All seats must run the same IW4x build, or they cannot join each other. The server pins the version, so they do.

## 1. Get MW2 onto the server

1. Run the server playbook once, so the share exists: `ansible-playbook server.yml`.
2. On a PC with Steam, install **Call of Duty: Modern Warfare 2 (2009) – Multiplayer**.
3. In Steam: right-click the game → Properties → Installed Files → **Verify integrity of game files**. A broken copy gives `Could not find zone 'code_pre_gfx_mp.ff'` later.
4. Copy the game folder to the server (about 15 GB):

   ```bash
   rsync -a --info=progress2 \
     "$HOME/.local/share/Steam/steamapps/common/Call of Duty Modern Warfare 2/" \
     <login-user>@<server>:/srv/games/mw2/
   ```

   From Windows, copy the folder with WinSCP to the same place.
5. Run the server playbook again. It finds `iw4mp.exe`, downloads IW4x (about 750 MB) and puts it in the folder. The folder now has `iw4x.exe`, `iw4x.dll`, the `iw4x/` folder and `.iw4x-version`.

## 2. Turn it on for the seats

In the private `ansible/inventory.yml`:

```yaml
all:
  vars:
    seat_gameshare: rsync://<server-address>/games
```

Then, canary first:

```bash
ansible-playbook seat.yml -l canary --check --diff   # lists the files rsync would copy
ansible-playbook seat.yml -l canary
```

The first copy takes a few minutes per seat. Later runs copy only what changed.

## 3. Test on the canary

On the seat, as admin:

```bash
sudo -u player iw4x-seat --check
```

It prints the installed version and `ok: ready to play`, or what is missing.

Then in the launcher, test the **Before merging a game** list in [games/README.md](../games/README.md). For MW2, also check:

- [ ] The game starts full screen with no dialogs. Note how long the first start takes.
- [ ] Your launcher nickname is your name in game.
- [ ] Resolution and frame cap match the seat's tier.
- [ ] Join (`+connect`) reaches a server.
- [ ] Two seats see each other in one match.

Then roll out to the other seats as usual.

## How a seat runs it

- `/games/classic/mw2` is read-only and the same on every seat.
- `iw4x-seat` builds a writable view of the folder in the player's home at every start: real folders, linked game files, copied `.exe` and `.dll` files. IW4x writes its settings and player key to `players/` there.
- The home reset at End session throws the view away. Each session starts clean and gets its own IW4x key, so seats never clash.
- It starts IW4x with `umu-run`, using the Proton build pinned in `/etc/arcade/arcade.yaml`, or GE-Proton if none is pinned.
- Output goes to `~/.local/share/arcade/iw4x.log` for the rest of the session.

## Fixing problems

| Problem | Fix |
| --- | --- |
| Game is not in the launcher | The seat has no `/games/classic/mw2`. Check `seat_gameshare` and that the server's folder has `iw4mp.exe`, `iw4x.exe` and `iw4x.dll`. The playbook output says what is missing. |
| Game crashes or misses files on one seat | Run the seat playbook with `-e seat_gameshare_verify=true`. It compares every file by content and copies back anything damaged or missing. |
| `Could not find zone 'code_pre_gfx_mp.ff'` | The copy on the server is incomplete. Verify integrity in Steam, copy again (step 1), then run the seat playbook. |
| One seat cannot join the others | Different builds. Run `iw4x-seat --check` on both; run the seat playbook on the odd one out. |
| Something else | Read `~/.local/share/arcade/iw4x.log` on the seat before End session. |

## Updating IW4x

1. Check the newest releases: [iw4x-client](https://github.com/iw4x/iw4x-client/releases) and [iw4x-rawfiles](https://github.com/iw4x/iw4x-rawfiles/releases).
2. Change `iw4x_client_version`, `iw4x_rawfiles_version` and their checksums (`roles/server_gameshare/defaults`) in a pull request.
3. Run `server.yml`. Seats pick the update up at their next pull. Not in the 48 hours before game night.

## Not done yet

- **Host button.** No LAN host from the launcher yet. Next step: an IW4x dedicated server on the gaming server (see IW4x's server hosting guide).
- **DLC maps.** IW4x can download the DLC maps from its CDN. Not part of this yet.
- **Proton prefix in the golden home.** The prefix is made on the first start in each session, which adds some seconds.

## Sources

- [IW4x quickstart](https://docs.iw4x.io/get-started/quickstart/)
- [IW4x Linux guide](https://docs.iw4x.io/get-started/manual-install/linux-guide/)
- [IW4x performance guide](https://docs.iw4x.io/guides/performance/)
- [iw4x-updoot](https://github.com/kkrruumm/iw4x-updoot), a community update script
