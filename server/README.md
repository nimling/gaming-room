# Server

One Ubuntu Server LTS box on the gaming VLAN runs everything shared as Docker containers. The Ansible playbook [`ansible/server.yml`](../ansible/server.yml) sets it up.

Status: Phase 2 prepared. The playbook passes in an Ubuntu container, and Mumble and Lancache were tested live in Docker. It has not run on the real server yet.

## Services

| Service | Role | Ports (gaming VLAN only) | Status |
| --- | --- | --- | --- |
| Mumble | `server_mumble` | 64738 TCP and UDP | Ready |
| Lancache and its DNS | `server_lancache` | 53 TCP and UDP, 80, 443 | Ready |
| Wake-on-LAN | `server_wol` | — | Ready: `wake-seats`, `wake-seats seat-03` |
| Firewall, Docker, sshd | `server_base` | 22 | Ready |
| UT99 and OpenRA game servers | — | 7777 UDP, 1234 TCP | Next: needs the UT99 data files on the server |
| LAN game list for the launcher | — | — | Later |
| Steam PC Café Server | — | UDP 27036, TCP 27037 | Only if Valve says go. Windows in `dockur/windows` (KVM) on macvlan |
| Claude Code | — | — | Later: dedicated user, logs every command |

All service ports bind to the server's LAN address only. Docker publishes ports past `ufw`, so the VLAN is the real boundary; `ufw` guards SSH and anything else on the host.

## Voice

- The root channel is called **Lobby**. Seats join it with `mumble://<nickname>@<server>/`.
- No server password by default: the VLAN is closed to the rest of the network. Set `mumble_server_password` in the private inventory to add one.
- Opus always (`opusthreshold=0`), 72 kbit/s per person, up to 30 people.
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
