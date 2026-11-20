# Server

One Ubuntu Server LTS box on the gaming VLAN runs everything shared as Docker containers. The Ansible playbook [`ansible/server.yml`](../ansible/server.yml) sets it up.

Status: Phase 2 prepared. The playbook passes in an Ubuntu container, and Mumble and Lancache were tested live in Docker. It has not run on the real server yet.

## Services

| Service | Role | Ports (gaming VLAN only) | Status |
| --- | --- | --- | --- |
| Mumble | `server_mumble` | 64738 TCP and UDP | Ready |
| Lancache and its DNS | `server_lancache` | 53 TCP and UDP, 80, 443 | Ready |
| Game share (rsync, read-only) | `server_gameshare` | 873 TCP | Ready: MW2 with IW4x ([docs/iw4x.md](../docs/iw4x.md)); Unreal Tournament built from the free GOTY CD and OldUnreal 469e |
| Steam café library | `server_steam_cafe` | — (SSH out to the seats) | Tested in containers, not yet on the real room: the same Steam games on every seat; see below |
| Wake-on-LAN | `server_wol` | — | Ready: `wake-seats`, `wake-seats seat-03` |
| Firewall, Docker, sshd | `server_base` | 22 | Ready |
| Always-on game servers: UT99, UT2004, Quake 3, Jedi Academy | `server_gameservers` | 7777-7779 and 8777, 7800-7801, 27960, 29070 UDP; 7787 TCP (UT99 vote menu) | Ready: in-game voting for map and mode; see below |
| OpenRA game server | — | 1234 TCP | Next |
| LAN game list for the launcher | — | — | Later |
| Steam PC Café Server | — | UDP 27036, TCP 27037 | Only if Valve says go. Windows in `dockur/windows` (KVM), NIC on the host bridge `br0` |
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
- Cache on `/srv/lancache/cache`, 250 GB by default (`lancache_cache_size`). The server has one 468 GB disk, shared with `/srv/games` and the Steam café library.
- **The real server sets `lancache_cache_size` in `ansible/inventory.yml`.** Lower it there by hand to `250g` (or less) before the café library fills the disk. The cache keeps its files; it only stops growing and evicts the oldest.

## Steam café library

Every seat gets the same Steam café games, ready to play, without anyone clicking Install on ten PCs. No Steam login and no steamcmd on the server: Steam checks the licence on the seat when the game starts (café pool or the player's own account).

- **The list:** `steam_cafe_apps` in [`ansible/vars/steam_cafe.yml`](../ansible/vars/steam_cafe.yml), read by both `server.yml` and `seat.yml`. Apps not on it are never touched.
- **Collect, on the server, 02:00** (`steam-cafe-collect.timer`, before the seats' 03:30 pull). As `cafe-sync`, it reads each seat's appmanifests. Per app it takes the seat with the highest `buildid` and `StateFlags` 4 (fully installed). If that is newer than the server's copy, it copies `common/<installdir>/` with `rsync --delete`, then the appmanifest last, into `/srv/games/steam-cafe/steamapps`. One log line per app: `journalctl -u steam-cafe-collect`.
- **Skipped seats:** a seat that does not answer, where Steam or a game runs, or that is in the launcher's People list. If no seat can be read, nothing changes.
- **Distribute, on each seat, at every pull:** for each listed app where the share's `buildid` is higher than the seat's (or the seat lacks it), it copies the game files into `/games/steam/steamapps/common/` (owner `player`, group `games`), then the appmanifest. Skipped while Steam runs or a session is in use. Equal versions: nothing happens.
- **Access:** `cafe-sync` on the server has its own SSH key (`/var/lib/cafe-sync/.ssh/id_ed25519`), made on the server; the private key never leaves it. The public key is on the share (`steam-cafe/cafe-sync.pub`). On each seat, `cafe-sync` accepts only that key, only from the server's address (`seat_steam_cafe_server`, default `arcade_voice_server`, must be an IP), with `restrict` and the forced command `cafe-sync-serve`: a status line, or `rrsync -ro` of the Steam library. It is not in the `games` group, so it cannot write there.
- **A rebuilt seat** has a new SSH host key. The collect then skips it ("did not answer ... host key"). Remove the old key: `sudo -u cafe-sync ssh-keygen -R <seat-address> -f /var/lib/cafe-sync/.ssh/known_hosts`.
- **Space:** a copy that would leave less than 20 GB free on the server (`steam_cafe_reserve_gib`) is left out.

**Add a game:** install it with Steam on any seat. Add one line with its appid to `steam_cafe_apps` (the number in `steamapps/appmanifest_<appid>.acf`) in a pull request. After the merge, run `server.yml`. The next night the server takes it. Seats copy it at their pull once their branch has the change (canary: `main`; the rest: `stable`).

Run it by hand: `sudo systemctl start steam-cafe-collect`, or `sudo -u cafe-sync steam-cafe collect --dry-run` to only see what it would copy.

## Game servers

- Off by default. Switch on with `gameservers_enabled: true` in `ansible/inventory.yml`.
- **Always on, one per game**, so players can join at once. Each lets players vote in game for the next map and mode:

  | Server | Port | Voting |
  | --- | --- | --- |
  | UT99 (`ut99-classic`) | 7777 | VoteSys: the menu opens at the end of each match; mid-match with F7 on the seats (`mutate votemenu`). Tabs InstaGib, Classic (DM, TDM, CTF, Domination, Assault) and Co-op (Monster Hunt). |
  | UT2004 (`ut2004-instagib`) | 7800 | Built in: press X, then Map Voting (`ShowVoteMenu`); it also opens at the end of each match. InstaGib DM and CTF, DM, TDM, CTF, Bombing Run, Double Domination, Onslaught. |
  | Quake III Arena (`quake3-ffa`) | 27960 | Built in: `/callvote map q3dm6`, `/callvote g_gametype 4`; F1 yes, F2 no. |
  | Jedi Academy (`jka-ffa`) | 29070 | Built in, the same as Quake 3. |

- **On demand**: UT99 Monster Hunt and UT2004 Onslaught as their own servers.
- One file per server type in [`presets/`](presets/README.md), for example UT2004 InstaGib with map voting. A preset shows up once its game is in `/srv/games`.
- Each preset runs as its own service, `gs-<id>.service`, as the account `gameserver`, with its settings in `/var/lib/gameservers/<id>`. It reads the game from the share; it can only write its own settings folder.
- The control service (`gameservers-api.service`, port 8790, gaming VLAN only) lists the presets with their players and map, and starts or stops them. A polkit rule lets its account start and stop `gs-*.service`, nothing else. The launcher will use it for a Servers view.
- On-demand servers stop after 15 minutes without players (`gameservers_idle_minutes`); at most 4 run at once (`gameservers_max_running`). A server that does not answer the player query is never stopped for being empty.
- The old `ut99-server.service` is gone; `ut99-classic` replaced it on the same ports.

```bash
curl http://<server>:8790/api/servers                              # list
curl -X POST http://<server>:8790/api/servers/ut2004-instagib/start # start
curl -X POST http://<server>:8790/api/servers/ut2004-instagib/stop  # stop
```

## Steam PC Café

- Off by default. Switch on with `pccafe_enabled: true` and `pccafe_bridge_nic: <NIC>` (for example `enp2s0`) in `ansible/inventory.yml`.
- The server's NIC joins the bridge `br0`; the bridge takes over the server's address (same MAC, so the DHCP reservation holds). The VM's NIC is the tap `tap-cafe` on `br0`, so Windows gets its own address from the router, like a seat.
- Web viewer: `http://<server>:8006`, from the gaming VLAN only. The Windows password is in `/srv/gaming-room/pccafe/windows-password` (root only).
- Why not macvlan: QEMU's macvtap path stopped carrying the VM's traffic on this server once Windows' driver took over.
- **First switch to the bridge, by hand** (rolls back by itself after 2 minutes unless confirmed):

  ```bash
  ansible-playbook server.yml --check --diff     # shows the new netplan file
  sudo install -m 600 <rendered file> /etc/netplan/60-gaming-room-bridge.yaml
  sudo netplan try --timeout 120                 # check route, DNS, a seat; then Enter
  ansible-playbook server.yml                    # tap, firewall, VM
  ```

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

6. In UniFi, set the gaming VLAN's DHCP DNS server to the server's address. (untick Auto DNS Server; DNS Server 1 = the server; leave DNS Server 2 empty, or seats skip the cache at random). The server itself keeps the router as its DNS (`roles/server_pccafe`, netplan `use-dns: false`).
7. At each seat, once: the bootstrap line in [`../bootstrap/seat.sh`](../bootstrap/seat.sh).

## Hardware minimum

CPU with VT-x or AMD-V, 16 GB RAM, 1–2 TB SSD, wired gigabit (2.5 GbE preferred).

## Shared login account

The server has one shared account for automation, such as an MCP server: `orchestrator`.

- It is in the `sudo` group, with sudo and no password (`/etc/sudoers.d/orchestrator`).
- Login is by SSH key only. The password is locked.
- Morten has the private key. Ask him for access, or give him the public key of a new client and he adds it to `~orchestrator/.ssh/authorized_keys`.
- The private key is not in this repo. Keep copies only in Bitwarden Secrets Manager.
- A person made the account by hand on the server. It is not in Ansible yet.
- A temporary `admin` account with a password was used for the setup. It is deleted.

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

