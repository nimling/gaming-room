# Ansible

Everything on a seat is described here. Seats apply it themselves with `ansible-pull` at boot and every night; nobody configures a seat by hand.

## Roles (Phase 1)

| Role | Does |
| --- | --- |
| `base` | Base packages, `/etc/seat/seat.conf`, sshd (keys only, named users only), time sync, Wake-on-LAN |
| `accounts` | `admin`, `player` and `orchestrator` accounts; removes old users from `wheel`; polkit rule for the player |
| `tier` | `seat-tier` detects GPU memory and RAM at every boot and writes `/etc/seat/tier` (A, B or C) |
| `games` | `/games` as a btrfs subvolume: `/games/steam` (player-writable), `/games/classic` (read-only); copies games from the server's share (`seat_gameshare`) and installs `iw4x-seat` |
| `session` | Golden home, `seat-reset`, logout watcher, idle logout, auto-login (SDDM or Plasma Login Manager), token check, golden capture |
| `updates` | `seat-update` (snapshot, then full update) and the update freeze |
| `pull` | `seat-pull.timer`: `ansible-pull` at boot and nightly |

Launcher roles come in Phase 3.

## Server roles (Phase 2)

| Role | Does |
| --- | --- |
| `server_base` | Ubuntu check, Docker, sshd (keys only), `ufw`, time zone, KVM check |
| `server_mumble` | Mumble in Docker; root channel "Lobby" |
| `server_lancache` | Lancache and its DNS in Docker |
| `server_gameshare` | Read-only rsync share for game files in `/srv/games`; installs IW4x on top of MW2 there |
| `server_wol` | `wake-seats`, using `seat_mac` from the inventory |
| `server_claude` | `claude` account with sudo and its own SSH key, Claude Code, GitHub CLI, Ansible, and `claude-orchestrator.service` (Remote Control in tmux) |

See [server/README.md](../server/README.md). Install the collections first: `ansible-galaxy collection install -r requirements.yml`.

## How the player session works

- At boot, `seat-reset-boot.service` replaces `/home/player` with a btrfs snapshot of `/home/.golden/player`.
- The login manager logs the player in. Newer CachyOS uses KDE's Plasma Login Manager (`/etc/plasmalogin.conf`); older installs use SDDM.
- **End session** (launcher), **20 min idle** (`seat-idle`) and a **KDE log out** (`seat-logout-watch.service`) all lead to `seat-reset`. It stops the login manager, ends the player's processes, replaces the home, removes Proton prefixes and temp files, and starts the login manager again. On ext4 it copies the golden home instead of taking a snapshot; on seat-01 a reset takes about 1.2 seconds.
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

### First run on a freshly installed seat

1. At the seat, once, from the installer's account (here `nim`): the bootstrap line in [`../bootstrap/seat.sh`](../bootstrap/seat.sh). It turns on SSH with the GitHub keys, sets the host name, adds a temporary passwordless sudo rule (`/etc/sudoers.d/90-seat-bootstrap`), and prints the seat, user, address, MAC and filesystem.
2. In `inventory.yml`: the seat's `ansible_host`, and `seat_mac` for `wake-seats`.
3. First run as that user, keeping its SSH login for this one run. Pass the user with `-e`: the inventory's `ansible_user` (orchestrator) would win over `-u`.

   ```bash
   ansible-playbook seat.yml -l seat-02 -e '{"ansible_user": "nim", "seat_extra_ssh_users": ["nim"]}' --check --diff
   ansible-playbook seat.yml -l seat-02 -e '{"ansible_user": "nim", "seat_extra_ssh_users": ["nim"]}'
   ```

   - A fresh seat may carry a months-old package list. The first run updates the whole system once and restarts sshd (an OpenSSH upgrade otherwise resets every new connection). If the kernel changed, it reboots at the end of the run, so the seat comes back with auto-login in place.
   - CachyOS has ufw on and it drops SSH. The bootstrap opens SSH to the seat's own network; the base role then allows SSH from `seat_orchestrator_source`.
   - The last task deletes the temporary sudo rule. `nim` leaves `wheel` and SSH.
4. From now on Ansible logs in as `orchestrator` with passwordless sudo, from the addresses in `seat_orchestrator_source` only.

Measured on seat-01 (RTX 3090, ext4): full system update about 2 minutes, configuration 84 seconds, second run 0 changes.

### Running Ansible from a Mac

Ansible is not installed on the Mac; it runs in a small container with the SSH key mounted read-only:

```bash
docker run --rm -v "$PWD":/repo:ro -v ~/.ssh/id_ed25519:/root/.ssh/id_ed25519:ro \
  -v ~/.ssh/known_hosts:/root/.ssh/known_hosts:ro -w /repo/ansible gaming-room-ansible \
  ansible-playbook seat.yml -l canary --check --diff
```

The image is `python:3.13-slim` plus `openssh-client`, `ansible-core` and the `ansible.posix`, `community.docker` and `community.general` collections.

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
