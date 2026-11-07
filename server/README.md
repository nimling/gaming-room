# Server

One Linux box on the gaming VLAN runs everything shared as Docker containers. Built in Phase 2.

| Service | Purpose | Notes |
| --- | --- | --- |
| Mumble server | Low-latency voice | Official `mumble-server` image, port 64738 TCP and UDP |
| Lancache + DNS | Download each game once | `lancachenet/monolithic`; VLAN DNS points CDN names at it; 1–2 TB SSD |
| Steam PC Café Server | Café license pool and content cache | Only if the Phase 0 spike says go. Windows in `dockur/windows` (KVM) on a macvlan network, same subnet as the seats (UDP 27036, TCP 27037) |
| Game servers | UT99, OpenRA and others | One container or systemd unit per game |
| Claude Code | Orchestrator and tester | Dedicated `claude` user; see [`../CLAUDE.md`](../CLAUDE.md) |
| Wake-on-LAN | Start all seats | `wakeonlan` |

## Hardware minimum

CPU with VT-x or AMD-V, 16 GB RAM, 1–2 TB SSD, wired gigabit (2.5 GbE preferred).
