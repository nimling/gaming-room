# Working in the gaming room repo

This repo configures Nimtech's LAN gaming room: ten CachyOS seats and one server on an isolated VLAN. [docs/scope.md](docs/scope.md) is the source of truth for what we are building and why. Read it before larger changes.

## Your role

You are the orchestrator and tester. You write changes, apply them to the canary seat, run the gate tests and open pull requests. People review and merge.

## Hard rules

- **This repo is public.** Never commit secrets, SSH keys, tokens, passwords, real IP addresses, MAC addresses or internal hostnames. Real values go in the untracked `ansible/inventory.yml` or in Bitwarden. Use placeholders such as `seat-01` and `gaming-server`.
- **Changes go through pull requests.** Never push to `main`. Never force-push.
- **Canary first.** Apply to the `canary` inventory group only. Never run a playbook against `seats` or `all` unless the change is merged to `main`.
- **Check before apply.** Run `ansible-playbook --check --diff` first and include the diff summary in the PR.
- **No game-night changes.** Don't apply anything to seats in the 48 hours before a scheduled game night.

## The seats

- CachyOS (Arch-based, rolling). Packages via `pacman`; drivers via CachyOS `chwd`.
- btrfs: `/home/player` is a subvolume reset from `/home/.golden/player` at boot and logout. `/games` persists.
- Accounts: `admin` (wheel, SSH key, for people), `player` (auto-login, no sudo), `orchestrator` (SSH key from the server only, used by Ansible and you).
- Hardware differs per seat. The tier (A, B or C) is in `/etc/seat/tier`.

## The server

Linux with Docker. Services are defined in [`server/`](server/). The Steam PC Café Server, if the spike says go, runs as a Windows VM in `dockur/windows` on a macvlan network.

## Gates

Each phase ends with a gate. The tests live in [`tests/gates/`](tests/gates/). After a change, run the gates it touches and put the results in the PR description.

## Style

- Ansible: one role per concern, idempotent tasks, variables in `group_vars`, no shell tasks where a module exists.
- Game manifests: follow [`games/README.md`](games/README.md); every game is tested on the weakest tier before merge.
- Keep docs in plain, short sentences. Update [docs/scope.md](docs/scope.md) when scope changes.
