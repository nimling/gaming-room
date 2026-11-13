#!/bin/bash
# Runs inside an Ubuntu container (see run-server.sh). Lints the server
# playbook, applies it twice with server_in_container=true, and checks the
# rendered files. Services and the firewall are not started here.
set -euo pipefail

step() { printf '\n=== %s ===\n' "$*"; }
ok() { echo "ok    $*"; }
die() { echo "FAIL  $*" >&2; exit 1; }

step "Install tools"
export DEBIAN_FRONTEND=noninteractive
# Some CI runners cannot resolve *.ubuntu.com (GitHub, 2026-11-11). Then use
# the kernel.org mirror, which is outside that domain and carries every pocket.
if ! getent hosts archive.ubuntu.com >/dev/null; then
    echo "archive.ubuntu.com does not resolve; using mirrors.edge.kernel.org"
    sed -i -E 's#https?://(archive|security)\.ubuntu\.com/ubuntu#http://mirrors.edge.kernel.org/ubuntu#g' \
        /etc/apt/sources.list.d/ubuntu.sources
fi
apt-get update -qq
apt-get install -y -qq ansible ansible-lint yamllint shellcheck openssh-server sudo python3 python3-apt iproute2 >/dev/null
mkdir -p /run/sshd

cp -a /repo /work
cd /work

step "Lint"
shellcheck bootstrap/seat.sh ansible/roles/server_gameshare/files/ut2004-build ansible/roles/server_gameshare/files/tmnf-build tests/gates/server-services.sh tests/container/test-server.sh tests/container/run-server.sh
ok "shellcheck"
yamllint ansible
ok "yamllint"
(cd ansible && ansible-lint --offline -c ../.ansible-lint server.yml)
ok "ansible-lint"

step "Apply server.yml"
useradd -m nimadmin
install -d -m 700 -o nimadmin /home/nimadmin/.ssh
echo "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAItestkeyonly nimadmin@test" >/home/nimadmin/.ssh/authorized_keys
cat >/tmp/inventory.yml <<'EOF'
all:
  children:
    server:
      hosts:
        localhost:
          ansible_connection: local
          server_in_container: true
          server_login_user: nimadmin
          server_lan_ip: 192.0.2.10
          server_vlan_cidr: 192.0.2.0/24
          server_upstream_dns: 192.0.2.1
          ut99_server_enabled: true
          gameservers_enabled: true
          pccafe_enabled: true
          pccafe_bridge_nic: eth0
    seats:
      hosts:
        seat-01:
          seat_mac: "00:00:5e:00:53:01"
        seat-02:
EOF
run() { (cd ansible && ansible-playbook server.yml -i /tmp/inventory.yml "$@"); }
run --check >/tmp/check.log 2>&1 || { tail -n 30 /tmp/check.log; die "dry run on a fresh server failed"; }
ok "dry run"
run
ok "first run"

step "Second run changes nothing"
second=$(run)
echo "$second" | tail -n 3
echo "$second" | grep -Eq "changed=0 +unreachable=0 +failed=0" || die "second run changed something"
ok "idempotent"

step "Rendered files"
grep -q '"192.0.2.10:64738:64738/udp"' /srv/gaming-room/mumble/compose.yaml || die "Mumble port not bound to the LAN address"
grep -q 'MUMBLE_CONFIG_REGISTERNAME: "Lobby"' /srv/gaming-room/mumble/compose.yaml || die "Mumble root channel name"
grep -qx 'LANCACHE_IP=192.0.2.10' /srv/gaming-room/lancache/.env || die "Lancache IP"
grep -qx 'UPSTREAM_DNS=192.0.2.1' /srv/gaming-room/lancache/.env || die "upstream DNS"
[[ $(stat -c %u /srv/gaming-room/mumble/data) == 10000 ]] || die "Mumble data folder owner"
grep -q 'PasswordAuthentication no' /etc/ssh/sshd_config.d/10-gaming-room.conf || die "sshd hardening"
grep -qx 'HandleLidSwitch=ignore' /etc/systemd/logind.conf.d/10-gaming-room.conf || die "lid switch"
ok "compose files, settings and sshd"

step "Game share"
grep -qx '\[games\]' /etc/rsyncd.conf || die "rsync module missing"
grep -q 'read only = yes' /etc/rsyncd.conf || die "game share is not read-only"
grep -q 'hosts allow = 192.0.2.0/24' /etc/rsyncd.conf || die "game share not limited to the VLAN"
grep -q 'address = 192.0.2.10' /etc/rsyncd.conf || die "rsync not bound to the LAN address"
[[ $(stat -c %U /srv/games/mw2) == nimadmin ]] || die "game share not owned by the login user"
[[ ! -e /srv/games/mw2/iw4x.dll ]] || die "IW4x installed without MW2"
ok "read-only rsync share; IW4x waits for MW2"

step "TrackMania Nations Forever"
[[ -x /usr/local/sbin/tmnf-build ]] || die "TrackMania unpack script missing"
[[ ! -e /srv/games/tmnf ]] || die "TrackMania unpacked in the container (downloads should be off)"
command -v innoextract >/dev/null || die "innoextract missing"
ok "unpack script and tools"

step "Bought games on open-source engines"
[[ -d /srv/games/quake3 && -d /srv/games/jka ]] || die "share folders for bought games missing"
[[ $(stat -c %U /srv/games/quake3) == nimadmin ]] || die "quake3 share folder not owned by the login user"
[[ ! -e /srv/games/quake3/.engine-version ]] || die "engine added without the bought data"
# With the data copied in, the next run adds the engine. Stand-in engine
# parts are served from inside the container, laid out like the real ones
# (a flat zip with non-executable programs, and a zip with a top folder),
# so the test needs no third-party download.
install -d -o nimadmin /srv/games/quake3/baseq3
echo "stand-in for the bought pak0" >/srv/games/quake3/baseq3/pak0.pk3
mkdir -p /tmp/parts/eng /tmp/parts/pr/quake3-latest-pk3s/baseq3 /tmp/www
for f in quake3e.x64 quake3e-vulkan.x64 quake3e.ded.x64; do echo engine >"/tmp/parts/eng/$f"; done
echo pak >/tmp/parts/pr/quake3-latest-pk3s/baseq3/pak8.pk3
(cd /tmp/parts/eng && python3 -m zipfile -c /tmp/www/engine.zip quake3e.x64 quake3e-vulkan.x64 quake3e.ded.x64)
(cd /tmp/parts/pr && python3 -m zipfile -c /tmp/www/pr.zip quake3-latest-pk3s)
(cd /tmp/www && exec python3 -m http.server 8099 --bind 127.0.0.1 >/dev/null 2>&1) &
sleep 1
engine_sum=$(sha256sum /tmp/www/engine.zip | cut -d' ' -f1)
pr_sum=$(sha256sum /tmp/www/pr.zip | cut -d' ' -f1)
cat >/tmp/engine-vars.yml <<VARS
gameshare_engine_games:
  - name: quake3
    data: baseq3/pak0.pk3
    guide: docs/bought-games.md
    programs: [quake3e.x64, quake3e-vulkan.x64, quake3e.ded.x64]
    parts:
      - { name: quake3e-test, url: "http://127.0.0.1:8099/engine.zip", ext: .zip, sha256: $engine_sum }
      - { name: pointrelease-test, url: "http://127.0.0.1:8099/pr.zip", ext: .zip, sha256: $pr_sum, subdir: quake3-latest-pk3s }
VARS
run -e @/tmp/engine-vars.yml >/tmp/engine.log 2>&1 || { grep -E "fatal|FAILED|msg" /tmp/engine.log | tail -n 20; die "run with the Quake 3 data failed"; }
[[ -x /srv/games/quake3/quake3e.x64 ]] || die "Quake3e not added, or not executable"
[[ -e /srv/games/quake3/baseq3/pak8.pk3 ]] || die "Quake 3 point release not added"
grep -q '^quake3e-test=' /srv/games/quake3/.engine-version || die "engine version not recorded"
[[ ! -e /srv/games/quake3/.unpacked ]] || die "unpack marker leaked into the game folder"
again=$(run -e @/tmp/engine-vars.yml)
echo "$again" | grep -Eq "changed=0 +unreachable=0 +failed=0" || { echo "$again" | grep -B3 changed: | head -20; die "engine run not idempotent"; }
ok "engine waits for the data, then is added; idempotent"

step "Unreal Tournament"
[[ -x /usr/local/sbin/ut99-build ]] || die "UT99 build script missing"
[[ ! -e /srv/games/ut99 ]] || die "UT99 built in the container (downloads should be off)"
id ut99server >/dev/null || die "no ut99server account"
unit=/etc/systemd/system/ut99-server.service
grep -q '^User=ut99server$' "$unit" || die "UT99 server does not run as ut99server"
grep -qF 'ucc-bin server "DM-Deck16][?game=Botpack.DeathMatchPlus?mutator=Botpack.InstaGibDM" port=7777' "$unit" || die "UT99 server command"
grep -q '^ReadWritePaths=/var/lib/ut99-server$' "$unit" || die "UT99 server can write outside its home"
grep -qF 'ini=/var/lib/ut99-server/.utpg/System/UnrealTournament.ini userini=/var/lib/ut99-server/.utpg/System/User.ini' "$unit" || die "UT99 server settings not in its home"
{ command -v 7z || command -v 7zz; } >/dev/null || die "7zip missing"
command -v bzip2 >/dev/null || die "bzip2 missing"
ok "build script, tools and dedicated server unit"

step "Unreal Tournament 2004"
[[ -x /usr/local/sbin/ut2004-build ]] || die "UT2004 build script missing"
[[ ! -e /srv/games/ut2004 ]] || die "UT2004 built in the container (downloads should be off)"
command -v unshield >/dev/null || die "unshield missing"
ok "build script and tools"

step "On-demand game servers"
python3 -m unittest discover -s tests/gameservers >/tmp/gs-unit.log 2>&1 || { cat /tmp/gs-unit.log; die "control service unit tests"; }
ok "control service unit tests"
[[ ! -e /etc/systemd/system/gs-ut2004-instagib.service ]] || die "preset service installed without its game"
grep -q '"presets": \[\]' /etc/gameservers/presets.json || die "presets listed without their game"
# A stand-in UT2004 makes its presets ready.
install -d -o nimadmin /srv/games/ut2004/System
install -m 755 -o nimadmin /dev/null /srv/games/ut2004/System/ucc-bin
printf '[Engine.GameInfo]\nMaxPlayers=16\n[IpDrv.MasterServerUplink]\nDoUplink=True\n' >/srv/games/ut2004/System/Default.ini
printf '[DefaultPlayer]\nName=Player\n' >/srv/games/ut2004/System/DefUser.ini
# Same stand-in engine parts as above, so this run needs no third-party download.
run -e @/tmp/engine-vars.yml >/tmp/gs.log 2>&1 || { grep -E "fatal|FAILED|msg" /tmp/gs.log | tail -n 20; die "run with UT2004 present failed"; }
u=/etc/systemd/system/gs-ut2004-instagib.service
grep -qx 'User=gameserver' "$u" || die "preset does not run as gameserver"
grep -qx 'ExecStart="./ucc-bin" "server" "DM-Rankin?game=XGame.xDeathMatch?mutator=XGame.MutInstaGib" "port=7800" "ini=/var/lib/gameservers/ut2004-instagib/UT2004.ini" "userini=/var/lib/gameservers/ut2004-instagib/User.ini" "-nohomedir"' "$u" \
    || { grep ExecStart "$u"; die "preset command"; }
grep -qx 'ReadWritePaths=/var/lib/gameservers/ut2004-instagib' "$u" || die "preset can write outside its folder"
grep -q '^\[Install\]' "$u" && die "on-demand preset starts at boot"
[[ -e /etc/systemd/system/gs-ut2004-onslaught.service && ! -e /etc/systemd/system/gs-ut99-monsterhunt.service ]] || die "wrong set of preset services"
ini=/var/lib/gameservers/ut2004-instagib/UT2004.ini
[[ $(stat -c %U "$ini") == gameserver ]] || die "preset settings not owned by gameserver"
grep -qx 'MaxPlayers=16' "$ini" || die "preset did not start from the game's defaults"
grep -qx 'VotingHandlerType=xVoting.xVotingHandler' "$ini" || die "map voting not set"
grep -qx 'DoUplink=False' "$ini" || die "internet uplink not off"
python3 - <<'PY' || die "preset list for the control service"
import json
c = json.load(open("/etc/gameservers/presets.json"))
assert c["address"] == "192.0.2.10" and c["port"] == 8790, c
assert [p["id"] for p in c["presets"]] == ["ut2004-instagib", "ut2004-onslaught"], c["presets"]
assert c["presets"][0]["query"] == {"type": "ut2004", "port": 7801}, c["presets"][0]
PY
grep -q 'subject.user == "gameserver-api"' /etc/polkit-1/rules.d/50-gameservers.rules || die "polkit rule"
grep -q '/\^gs-\[a-z0-9-\]+\\.service\$/' /etc/polkit-1/rules.d/50-gameservers.rules || die "polkit rule not limited to gs-*.service"
grep -qx 'User=gameserver-api' /etc/systemd/system/gameservers-api.service || die "control service account"
[[ -x /usr/local/lib/gameservers/gameservers-api ]] || die "control service not installed"
again=$(run -e @/tmp/engine-vars.yml)
echo "$again" | grep -Eq "changed=0 +unreachable=0 +failed=0" || { echo "$again" | grep -B3 changed: | head -20; die "game server run not idempotent"; }
ok "presets wait for their game; services, settings, polkit and control service; idempotent"

step "Steam PC Cafe VM"
c=/srv/gaming-room/pccafe/compose.yaml
bat=/srv/gaming-room/pccafe/oem/install.bat
np=/etc/netplan/60-gaming-room-bridge.yaml
tap=/etc/systemd/system/pccafe-tap.service
[[ $(stat -c '%a %U' "$c") == "600 root" ]] || die "cafe compose file not root-only"
grep -q 'image: dockurr/windows:6.06' "$c" || die "cafe image not pinned"
grep -q 'network_mode: host' "$c" || die "cafe container not on the host network"
grep -q 'NETWORK: "N"' "$c" || die "dockur still sets up its own network"
grep -qE 'ifname=tap-cafe,script=no,downscript=no -device virtio-net-pci,id=net0,netdev=hostnet0,mac=02(:[0-9a-f]{2}){5}"' "$c" \
    || die "cafe VM NIC is not the tap with a local MAC"
if grep -qE 'macvlan|vhost|NET_ADMIN' "$c"; then die "macvlan leftovers in the cafe compose file"; fi
pw=$(cat /srv/gaming-room/pccafe/windows-password)
(( ${#pw} >= 16 )) || die "cafe Windows password too short"
grep -qF "PASSWORD: \"$pw\"" "$c" || die "cafe Windows password not used"
grep -q 'localport=27036' "$bat" || die "cafe discovery firewall rule"
grep -q 'localport=27037' "$bat" || die "cafe client firewall rule"
grep -q 'RealTimeIsUniversal /t REG_DWORD /d 1' "$bat" || die "cafe clock not on UTC"
grep -q 'tzutil /s "W. Europe Standard Time"' "$bat" || die "cafe time zone"
if grep -qE '^ *parameters:' "$np"; then die "bridge parameters block netplan try's rollback"; fi
[[ $(stat -c '%a %U' "$np") == "600 root" ]] || die "bridge netplan file not root-only"
grep -q 'interfaces: \[eth0\]' "$np" || die "NIC not in the bridge"
grep -qE 'macaddress: "([0-9a-f]{2}:){5}[0-9a-f]{2}"' "$np" || die "bridge does not keep the NIC's MAC"
grep -q 'ExecStart=-/usr/sbin/ip tuntap add dev tap-cafe mode tap user 0' "$tap" || die "tap unit"
grep -q 'master br0 up' "$tap" || die "tap not on the bridge"
ok "cafe VM compose file, bridge, tap, password and firewall script"

step "Claude on the server"
id claude >/dev/null || die "no claude account"
[[ -s /home/claude/.ssh/id_ed25519.pub ]] || die "no SSH key for claude"
visudo -cf /etc/sudoers.d/20-claude >/dev/null || die "claude sudo rule invalid"
grep -q 'claude --remote-control' /etc/systemd/system/claude-orchestrator.service || die "Remote Control service"
[[ $(git config --file /home/claude/.gitconfig user.email) == 34557052+nimsim@users.noreply.github.com ]] || die "claude git identity"
ok "account, key, sudo, service and git identity"

step "wake-seats"
bash -n /usr/local/bin/wake-seats
grep -q '\[seat-01\]="00:00:5e:00:53:01"' /usr/local/bin/wake-seats || die "seat MAC missing"
if grep -q 'seat-02' /usr/local/bin/wake-seats; then die "seat without MAC listed"; fi
ok "MAC list from the inventory"

step "Refuses a login user without SSH keys"
: >/home/nimadmin/.ssh/authorized_keys
if run >/tmp/nokeys.log 2>&1; then die "ran without SSH keys"; fi
grep -q "has no SSH keys" /tmp/nokeys.log || die "wrong refusal"
ok "refused"

if [[ -d /out ]]; then
    cp -a /srv/gaming-room/. /out/
fi
printf '\nALL SERVER CHECKS PASSED\n'
