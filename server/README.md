# Server

One Ubuntu Server LTS box on the gaming VLAN runs everything shared as Docker containers. The Ansible playbook [`ansible/server.yml`](../ansible/server.yml) sets it up.

Status: Phase 2 prepared. The playbook passes in an Ubuntu container, and Mumble and Lancache were tested live in Docker. It has not run on the real server yet.

## Services

| Service | Role | Ports (gaming VLAN only) | Status |
| --- | --- | --- | --- |
| Mumble | `server_mumble` | 64738 TCP and UDP | Ready |
| Lancache and its DNS | `server_lancache` | 53 TCP and UDP, 80, 443 | Ready |
| Game share (rsync, read-only) | `server_gameshare` | 873 TCP | Ready: MW2 with IW4x ([docs/iw4x.md](../docs/iw4x.md)); Unreal Tournament built from the free GOTY CD and OldUnreal 469e |
| Wake-on-LAN | `server_wol` | — | Ready: `wake-seats`, `wake-seats seat-03` |
| Firewall, Docker, sshd | `server_base` | 22 | Ready |
| UT99 dedicated server | `server_gameshare` | 7777-7781 and 8777 UDP | Ready: InstaGib on DM-Deck16][, `ut99_server_enabled` in the private inventory |
| OpenRA game server | — | 1234 TCP | Next |
| LAN game list for the launcher | — | — | Later |
| Steam PC Café Server | — | UDP 27036, TCP 27037 | Only if Valve says go. Windows in `dockur/windows` (KVM) on macvlan |
| Claude Code (orchestrator) | `server_claude` | — | Ready: `claude` account, Remote Control in tmux, own SSH key for the seats |

All service ports bind to the server's LAN address only. Docker publishes ports past `ufw`, so the VLAN is the real boundary; `ufw` guards SSH and anything else on the host.

## Voice

- The root channel is called **Lobby**. Seats join it with `mumble://<nickname>@<server>/`.
- No server password by default: the VLAN is closed to the rest of the network. Set `mumble_server_password` in the private inventory to add one.
- Opus always (`opusthreshold=0`), 72 kbit/s per person, up to 30 people.
- Seats trust the server's self-signed certificate in advance (`roles/launcher`). Recreating the data volume makes a new certificate; run the seat playbook again to re-pin it, or seats ask about it.
- The SuperUser password is printed once in `docker logs mumble`. Store it in Bitwarden. Team channels can be added with it from any Mumble client.

## Cache

- Steam, Epic, Battle.net, Riot and other CDNs go through the cache (`USE_GENERIC_CACHE`).
- The cache DNS answers those names with the server's address and passes the rest to the VLAN gateway.
- **Seats must use the server as DNS.** Set it as the DNS server in the gaming VLAN's DHCP settings in UniFi.
- Cache on `/srv/lancache/cache`, 1000 GB by default (`lancache_cache_size`).

## First day

1. Install Ubuntu Server LTS. Tick **Install OpenSSH server** and import SSH keys from GitHub.
2. In the BIOS, turn on VT-x or AMD-V and power-on after power loss.
3. Give the server a fixed address (DHCP reservation in UniFi).
4. Put the real address and user name in `ansible/inventory.yml` (untracked).
5. Run:

   ```bash
   cd ansible
   ansible-galaxy collection install -r requirements.yml
   ansible-playbook server.yml --check --diff
   ansible-playbook server.yml
   ../tests/gates/server-services.sh <server-address>
   ```

6. In UniFi, set the gaming VLAN's DHCP DNS server to the server's address.
7. At each seat, once: the bootstrap line in [`../bootstrap/seat.sh`](../bootstrap/seat.sh).

## Hardware minimum

CPU with VT-x or AMD-V, 16 GB RAM, 1–2 TB SSD, wired gigabit (2.5 GbE preferred).

## Claude Code on the server

Claude runs here as the orchestrator. People talk to it from the Claude app or claude.ai through Remote Control, from anywhere, without VPN. The server only makes outbound HTTPS connections.

- Account `claude`: sudo without a password, its own SSH key (added as the seats' `orchestrator` key), the repo in `~/gaming-room` and the private inventory there.
- `claude-orchestrator.service` starts `claude --remote-control` in tmux at boot (`server_claude_autostart: true` after the first sign-in).
- Look at the session on the server: `sudo -iu claude tmux attach -t claude`, then Ctrl+B, D to leave it running.

One-time setup by a person:

1. Sign in: start `claude` as the `claude` account, open the sign-in link on any device, paste the code into the session. Needs a claude.ai plan with Remote Control (Team: an Owner turns it on).
2. GitHub: a fine-grained token for `nimling/gaming-room` only, with Contents and Pull requests set to Read and write, approved by the organisation. Paste it with a hidden prompt, never on screen:

   ```bash
   read -rs T && printf '%s\n' "$T" | ssh <you>@<server> 'sudo -iu claude gh auth login --with-token'; unset T
   ```

   Then `gh auth setup-git` as `claude`. The token expires after 90 days.

