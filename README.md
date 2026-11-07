# Gaming Room

Config, launcher and tests for Nimtech's LAN gaming room: ten CachyOS seats and one server on their own VLAN.

The goal: power on, type a name, and be in a game with voice in under a minute — and leave nothing behind when you go.

## Status

Scoping is done. **Phase 0** (inventory and spike) happens at the first dugnad. The **Phase 1** seat roles are in [`ansible/`](ansible/) and pass the container tests; they still need a run on a real seat. The full scope is in [docs/scope.md](docs/scope.md).

## What lives where

| Path | What |
| --- | --- |
| [`ansible/`](ansible/) | Seat and server configuration, applied by `ansible-pull` |
| [`games/`](games/) | One manifest file per game |
| [`launcher/`](launcher/) | The launcher: nickname, game grid, End session |
| [`server/`](server/) | Docker services: Mumble, Lancache, game servers, PC Café VM |
| [`tests/gates/`](tests/gates/) | Acceptance tests, one per phase gate |
| [`docs/`](docs/) | Scope and runbooks |

## How a change lands

1. Branch and open a pull request.
2. Test it on the canary seat; attach the gate results to the PR.
3. A person reviews and merges.
4. Seats pick it up with `ansible-pull` at boot and nightly. The canary seat goes first; the rest follow a day later.

Nobody configures a seat by hand. If a seat breaks, it is rebuilt from this repo.

## Contributing

- Pick a work package in [docs/scope.md](docs/scope.md#work-packages-and-roles).
- Adding a game is one YAML file in [`games/`](games/) — see [`games/README.md`](games/README.md).
- Claude Code works in this repo as orchestrator and tester; its rules are in [`CLAUDE.md`](CLAUDE.md).

## This repo is public

No secrets, keys, passwords, tokens, real IP addresses or internal hostnames go in here. Real values live in an untracked `ansible/inventory.yml` and in Bitwarden. Copy [`ansible/inventory.example.yml`](ansible/inventory.example.yml) to start.

## License

[MIT](LICENSE)
