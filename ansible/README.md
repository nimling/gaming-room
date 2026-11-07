# Ansible

Everything on a seat or the server is described here. Seats apply it themselves with `ansible-pull` at boot and nightly; nobody configures a seat by hand.

## Planned roles (Phase 1)

| Role | Does |
| --- | --- |
| `base` | Packages, locale, time sync, Wake-on-LAN, SSH hardening |
| `accounts` | `admin`, `player` and `orchestrator` users; sudo and polkit rules |
| `session` | SDDM auto-login, golden home, `seat-reset.service`, idle logout |
| `tier` | Detects GPU, RAM and display; writes `/etc/seat/tier` |
| `games` | `/games` subvolume, classic game files, Steam library folder |
| `launcher` | Installs the launcher and the game manifests |
| `voice` | Mumble client and its golden config |
| `updates` | Update window, canary ordering, snapshot before update |

## Getting started

```bash
cp inventory.example.yml inventory.yml   # real hostnames go here; it is gitignored
ansible-inventory -i inventory.yml --graph
```

Always run with `--check --diff` against the `canary` group first.
