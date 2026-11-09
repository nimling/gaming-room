# Ansible

Everything on a seat is described here. Seats apply it themselves with `ansible-pull` at boot and every night; nobody configures a seat by hand.

## Roles (Phase 1)

| Role | Does |
| --- | --- |
| `base` | Base packages, `/etc/seat/seat.conf`, sshd (keys only, named users only), time sync, Wake-on-LAN |
| `accounts` | `admin`, `player` and `orchestrator` accounts; removes old users from `wheel`; polkit rule for the player |
| `tier` | `seat-tier` detects GPU memory and RAM at every boot and writes `/etc/seat/tier` (A, B or C) |
| `games` | `/games` as a btrfs subvolume: `/games/steam` (player-writable), `/games/classic` (read-only) |
| `session` | Golden home, `seat-reset`, logout watcher, idle logout, SDDM auto-login, token check, golden capture |
| `updates` | `seat-update` (snapshot, then full update) and the update freeze |
| `pull` | `seat-pull.timer`: `ansible-pull` at boot and nightly |

Launcher roles come in Phase 3.

## Server roles (Phase 2)

| Role | Does |
| --- | --- |
| `server_base` | Ubuntu check, Docker, sshd (keys only), `ufw`, time zone, KVM check |
| `server_mumble` | Mumble in Docker; root channel "Lobby" |
| `server_lancache` | Lancache and its DNS in Docker |
| `server_wol` | `wake-seats`, using `seat_mac` from the inventory |

See [server/README.md](../server/README.md). Install the collections first: `ansible-galaxy collection install -r requirements.yml`.

## How the player session works

- At boot, `seat-reset-boot.service` replaces `/home/player` with a btrfs snapshot of `/home/.golden/player`.
- SDDM logs the player in.
- **End session** (launcher), **20 min idle** (`seat-idle`) and a **KDE log out** (`seat-logout-watch.service`) all lead to `seat-reset`. It stops SDDM, ends the player's processes, replaces the home, removes Proton prefixes and temp files, and starts SDDM again.
- `seat-reset` refuses to touch the home if the golden home is missing.
- Changes to the golden home: log in as the player, set things up, sign out of every account, then run `sudo seat-golden-capture` as admin. It refuses while any login token is left (`seat-token-check`).

## Commands

```bash
cp inventory.example.yml inventory.yml            # real values; gitignored
ansible-playbook seat.yml -l canary --check --diff
ansible-playbook seat.yml -l canary
ansible-playbook update.yml -l canary              # a day later: -l seats
ansible-playbook server.yml --check --diff         # the gaming server
```

First run on a freshly installed seat (only the installer's `admin` user exists yet):

```bash
ansible-playbook seat.yml -l seat-01 -u admin --ask-become-pass
```

## Branches

- The canary group pulls `main`.
- All other seats pull `stable`.
- Promote by fast-forwarding `stable` to `main` once the canary has played fine for a day.

## Tests

```bash
tests/container/run.sh     # playbook + Phase 1 gates in a CachyOS container
```

CI runs the same script on every pull request that touches `ansible/` or `tests/`.

## Not covered by the container test

These need a real seat (Phase 1 dugnad):
- SDDM auto-login.
- The logout watcher and idle logout. `swayidle` on KWin needs ext-idle-notify-v1.
- polkit decisions inside a live session.
- Wake-on-LAN.
- GPU detection.
- The display manager restart during a reset.
