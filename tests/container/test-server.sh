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
shellcheck bootstrap/seat.sh ansible/roles/server_gameshare/files/ut2004-build ansible/roles/server_gameshare/files/tmnf-build \
    ansible/roles/server_gameshare/files/battlenet-prefix-publish ansible/roles/session/files/seat-token-check tests/gates/server-services.sh tests/container/test-server.sh tests/container/run-server.sh
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
          gameservers_enabled: true
          pccafe_enabled: true
          pccafe_bridge_nic: eth0
    seats:
      hosts:
        seat-01:
          seat_mac: "00:00:5e:00:53:01"
          seat_lan_address: 192.0.2.21
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

step "Battle.net prefix: publish from the canary"
[[ -d /srv/games/battlenet/prefix ]] || die "share folder for the Battle.net prefix missing"
[[ -x /usr/local/sbin/battlenet-prefix-publish && -x /usr/local/sbin/seat-token-check ]] || die "publish script or token check missing"
cmp -s ansible/roles/session/files/token-paths /usr/local/share/seat/token-paths || die "server token list differs from the seats'"
grep -qx 'BATTLENET_PREFIX_SHARE=/srv/games/battlenet/prefix' /etc/default/battlenet-prefix-publish || die "publish settings"
grep -qx 'BATTLENET_PUBLISH_SSH_AS=claude' /etc/default/battlenet-prefix-publish || die "publish does not use the claude key"
if /usr/local/sbin/battlenet-prefix-publish 'seat-01;x' >/dev/null 2>&1; then die "publish took a bad seat name"; fi
# A fake prefix as Battle.net leaves it on the canary, signed out.
src=/tmp/bnet-prefix
u=$src/drive_c/users/steamuser
mkdir -p "$src/drive_c/Program Files (x86)/Battle.net" "$src/dosdevices" "$u/AppData/Roaming/Battle.net" \
    "$u/AppData/Local/Battle.net/Logs" "$u/AppData/Local/Battle.net/BrowserCaches/common/Network" "$u/Temp" \
    "$src/drive_c/ProgramData/Battle.net/Agent/Agent.9000/Logs"
echo exe >"$src/drive_c/Program Files (x86)/Battle.net/Battle.net.exe"
ln -s ../drive_c "$src/dosdevices/c:"; ln -s / "$src/dosdevices/z:"
echo '{"Client": {"SavedAccountNames": ""}}' >"$u/AppData/Roaming/Battle.net/Battle.net.config"
printf '[System\\\\CurrentControlSet\\\\Control\\\\ComputerName\\\\ComputerName]\n"ComputerName"="SEAT-01"\n' >"$src/system.reg"
printf 'WINE REGISTRY Version 2\n' >"$src/user.reg"
echo log >"$u/AppData/Local/Battle.net/Logs/battle.net.log"
echo log >"$src/drive_c/ProgramData/Battle.net/Agent/Agent.9000/Logs/Agent.log"
echo cookie >"$u/AppData/Local/Battle.net/BrowserCaches/common/Network/Cookies"
echo tmp >"$u/Temp/x.tmp"; echo dump >"$src/drive_c/crash.dmp"
echo stale >"$src/.prefix-version"
chown -R 1000:1000 "$src"; chmod 700 "$src/drive_c"
publish() { BATTLENET_PREFIX_SOURCE=$src /usr/local/sbin/battlenet-prefix-publish "$@"; }
# A login left in the prefix ("Keep me logged in" goes to Credential Manager).
printf '[Software\\\\Wine\\\\Credential Manager\\\\Generic: Battle.net] 1700000000\n' >>"$src/user.reg"
echo "earlier" >/srv/games/battlenet/prefix/.prefix-version
if out=$(publish 2>&1); then echo "$out"; die "published a prefix with a login"; fi
echo "$out"
grep -q 'token: .*/battlenet/user.reg' <<<"$out" || die "refusal does not name the login"
[[ $(cat /srv/games/battlenet/prefix/.prefix-version) == earlier ]] || die "a refused publish touched the share"
[[ -z $(find /srv/games/battlenet/prefix -mindepth 1 ! -name .prefix-version) ]] || die "a refused publish copied files to the share"
[[ -z $(find /var/tmp -maxdepth 1 -name "battlenet-prefix.*") ]] || die "refused copy left behind in /var/tmp"
ok "refuses a prefix with a login, and leaves the share and no copy behind"
printf 'WINE REGISTRY Version 2\n' >"$src/user.reg"
out=$(publish seat-01 2>&1) || { echo "$out"; die "publish of a clean prefix failed"; }
echo "$out"
p=/srv/games/battlenet/prefix
grep -Eq '^[0-9]{8}T[0-9]{6}Z from /tmp/bnet-prefix/$' "$p/.prefix-version" || die "version file: $(cat "$p/.prefix-version")"
[[ -f "$p/drive_c/Program Files (x86)/Battle.net/Battle.net.exe" && -f $p/drive_c/users/steamuser/AppData/Roaming/Battle.net/Battle.net.config ]] || die "prefix not published"
[[ $(readlink "$p/dosdevices/c:") == ../drive_c && $(readlink "$p/dosdevices/z:") == / ]] || die "drive links not kept as links"
for gone in drive_c/users/steamuser/AppData/Local/Battle.net/Logs drive_c/users/steamuser/AppData/Local/Battle.net/BrowserCaches \
    drive_c/users/steamuser/Temp drive_c/ProgramData/Battle.net/Agent/Agent.9000/Logs drive_c/crash.dmp; do
    [[ ! -e $p/$gone ]] || die "$gone was published"
done
[[ -z $(find "$p" ! -perm -o=r) && -z $(find "$p" -type d ! -perm -o=x) ]] || die "rsyncd (nobody) cannot read everything"
[[ -z $(find "$p" -perm /go=w ! -type l) && -z $(find "$p" ! -user root ! -path "$p") ]] || die "published files writable or not owned by root"
grep -q "note: system.reg names 'seat-01'" <<<"$out" || die "publish did not point out the seat's name in the registry"
first=$(cat "$p/.prefix-version")
sleep 1
rm -rf "$u/AppData/Roaming/Battle.net"
publish >/dev/null
[[ $(cat "$p/.prefix-version") != "$first" && ! -e $p/drive_c/users/steamuser/AppData/Roaming/Battle.net ]] || die "a new publish did not replace the old one"
ok "publishes without logs, caches and dumps; links kept; readable for rsyncd; new version each time"

step "Steam café library"
python3 -m unittest discover -s tests/steamcafe >/tmp/cafe-unit.log 2>&1 || { cat /tmp/cafe-unit.log; die "steam-cafe unit tests"; }
ok "steam-cafe unit tests (version compare, StateFlags, busy seats, list filtering, copies)"
id cafe-sync >/dev/null || die "no cafe-sync account"
id -nG cafe-sync | tr ' ' '\n' | grep -qx sudo && die "cafe-sync may use sudo"
[[ $(stat -c '%U %a' /var/lib/cafe-sync/.ssh/id_ed25519) == "cafe-sync 600" ]] || die "cafe-sync private key owner or mode"
cmp -s /var/lib/cafe-sync/.ssh/id_ed25519.pub /srv/games/steam-cafe/cafe-sync.pub || die "public key not published on the share"
[[ $(stat -c '%U %a' /srv/games/steam-cafe/cafe-sync.pub) == "root 644" ]] || die "published key owner or mode"
[[ $(stat -c '%U %a' /srv/games/steam-cafe) == "root 755" ]] || die "cafe-sync could replace the published key"
[[ $(stat -c '%U' /srv/games/steam-cafe/steamapps/common) == cafe-sync ]] || die "cafe-sync cannot fill the library"
python3 - <<'PY' || die "collect settings"
import json
c = json.load(open("/etc/steam-cafe/collect.json"))
assert c["seats"] == [{"name": "seat-01", "address": "192.0.2.21"}], c["seats"]
assert c["dest"] == "/srv/games/steam-cafe/steamapps", c
assert c["user"] == "cafe-sync" and c["ssh_key"] == "/var/lib/cafe-sync/.ssh/id_ed25519", c
assert c["presence_url"] == "http://192.0.2.10:8790/api/presence", c
ids = [a["appid"] for a in c["apps"]]
assert 550 in ids and 228980 in ids and len(ids) == len(set(ids)), ids
PY
s=/etc/systemd/system/steam-cafe-collect.service
grep -qx 'User=cafe-sync' "$s" || die "collect does not run as cafe-sync"
grep -qx 'ExecStart=/usr/local/sbin/steam-cafe collect --config /etc/steam-cafe/collect.json' "$s" || die "collect command"
grep -qx 'ReadWritePaths=/srv/games/steam-cafe/steamapps /var/lib/cafe-sync' "$s" || die "collect can write outside the library"
grep -qx 'OnCalendar=\*-\*-\* 02:00' /etc/systemd/system/steam-cafe-collect.timer || die "collect not at 02:00 (before the seats' 03:30 pull)"
# No seat answers in here (192.0.2.21 is a documentation address), and the
# People list is not running: the run must say so and change nothing.
out=$(timeout 120 runuser -u cafe-sync -- /usr/local/sbin/steam-cafe collect --config /etc/steam-cafe/collect.json 2>&1) \
    || { echo "$out"; die "collect failed with no seats answering"; }
echo "$out"
grep -q '^people list not reachable' <<<"$out" || die "collect did not report the missing People list"
grep -q '^seat-01: skipped, did not answer' <<<"$out" || die "collect did not report the silent seat"
grep -q '^no seat could be read (of 1); the library stays as it is' <<<"$out" || die "collect did not stop cleanly"
[[ -z $(ls -A /srv/games/steam-cafe/steamapps/common) ]] || die "collect wrote into the library"
ok "account, key on the share, settings, timer; no seats answering is handled"

step "TrackMania Nations Forever"
[[ -x /usr/local/sbin/tmnf-build ]] || die "TrackMania unpack script missing"
[[ ! -e /srv/games/tmnf ]] || die "TrackMania unpacked in the container (downloads should be off)"
command -v innoextract >/dev/null || die "innoextract missing"
ok "unpack script and tools"

step "Bought games on open-source engines"
[[ -d /srv/games/quake3 && -d /srv/games/jka && -d /srv/games/serioussam ]] || die "share folders for bought games missing"
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
[[ ! -e /etc/systemd/system/ut99-server.service ]] || die "old UT99 server unit still installed (now a preset)"
{ command -v 7z || command -v 7zz; } >/dev/null || die "7zip missing"
command -v bzip2 >/dev/null || die "bzip2 missing"
ok "build script and tools; old server unit gone"

step "Unreal Tournament 2004"
[[ -x /usr/local/sbin/ut2004-build ]] || die "UT2004 build script missing"
[[ ! -e /srv/games/ut2004 ]] || die "UT2004 built in the container (downloads should be off)"
command -v unshield >/dev/null || die "unshield missing"
ok "build script and tools"

step "On-demand game servers"
python3 -m unittest discover -s tests/gameservers >/tmp/gs-unit.log 2>&1 || { cat /tmp/gs-unit.log; die "control service unit tests"; }
ok "control service unit tests"
[[ ! -e /etc/systemd/system/gs-ut2004-instagib.service ]] || die "preset service installed without its game"
# The Quake 3 stand-in from the engine step makes its preset ready; nothing else.
grep -q '"id": "quake3-ffa"' /etc/gameservers/presets.json || die "Quake 3 preset not listed with its game present"
grep -q '"id": "ut2004-instagib"' /etc/gameservers/presets.json && die "presets listed without their game"
q=/etc/systemd/system/gs-quake3-ffa.service
grep -qx 'ExecStart="/srv/games/quake3/quake3e.ded.x64" "+set" "dedicated" "1" "+set" "net_port" "27960" "+set" "fs_homepath" "/var/lib/gameservers/quake3-ffa" "+set" "sv_hostname" "Nimtech Quake 3" "+set" "sv_maxclients" "16" "+set" "g_allowVote" "1" "+set" "g_gametype" "0" "+set" "fraglimit" "30" "+set" "timelimit" "15" "+map" "q3dm17"' "$q" \
    || { grep ExecStart "$q"; die "Quake 3 preset command"; }
grep -q '^\[Install\]' "$q" || die "always-on Quake 3 preset does not start at boot"
# A stand-in UT2004 makes its presets ready.
install -d -o nimadmin /srv/games/ut2004/System
install -m 755 -o nimadmin /dev/null /srv/games/ut2004/System/ucc-bin
printf '[Engine.GameInfo]\nMaxPlayers=16\n[IpDrv.MasterServerUplink]\nDoUplink=True\n' >/srv/games/ut2004/System/Default.ini
printf '[DefaultPlayer]\nName=Player\n' >/srv/games/ut2004/System/DefUser.ini
# And a stand-in UT99 with VoteSys, for the always-on UT99 preset.
install -d -o nimadmin /srv/games/ut99/System /srv/games/ut99/System64
echo stand-in >/srv/games/ut99/System/VoteSys_v21.u
install -m 755 -o nimadmin /dev/null /srv/games/ut99/System64/ucc-bin
printf '[Engine.GameEngine]\nServerPackages=Botpack\n[IpServer.UdpServerUplink]\nDoUplink=True\n' >/srv/games/ut99/System64/Default.ini
printf '[DefaultPlayer]\nName=Player\n' >/srv/games/ut99/System64/DefUser.ini
# Same stand-in engine parts as above, so this run needs no third-party download.
run -e @/tmp/engine-vars.yml >/tmp/gs.log 2>&1 || { grep -E "fatal|FAILED|msg" /tmp/gs.log | tail -n 20; die "run with UT2004 present failed"; }
u=/etc/systemd/system/gs-ut2004-instagib.service
grep -qx 'User=gameserver' "$u" || die "preset does not run as gameserver"
grep -qx 'ExecStart="/srv/games/ut2004/System/ucc-bin" "server" "DM-Rankin?game=XGame.xDeathMatch?mutator=XGame.MutInstaGib" "port=7800" "ini=/var/lib/gameservers/ut2004-instagib/UT2004.ini" "userini=/var/lib/gameservers/ut2004-instagib/User.ini" "-nohomedir"' "$u" \
    || { grep ExecStart "$u"; die "preset command"; }
grep -qx 'ReadWritePaths=/var/lib/gameservers/ut2004-instagib' "$u" || die "preset can write outside its folder"
grep -qx 'SuccessExitStatus=42 SIGTERM' "$u" || die "a normal UT2004 stop (exit 42) would count as failed"
grep -q '^\[Install\]' "$u" || die "always-on UT2004 preset does not start at boot"
grep -q '^\[Install\]' /etc/systemd/system/gs-ut2004-onslaught.service && die "on-demand preset starts at boot"
[[ -e /etc/systemd/system/gs-ut2004-onslaught.service && ! -e /etc/systemd/system/gs-ut99-monsterhunt.service ]] || die "wrong set of preset services"
ini=/var/lib/gameservers/ut2004-instagib/UT2004.ini
[[ $(stat -c %U "$ini") == gameserver ]] || die "preset settings not owned by gameserver"
grep -qx 'MaxPlayers=16' "$ini" || die "preset did not start from the game's defaults"
grep -qx 'VotingHandlerType=xVoting.xVotingHandler' "$ini" || die "map voting not set"
grep -qx 'DoUplink=False' "$ini" || die "internet uplink not off"
grep -qx 'Port=7800' "$ini" || die "UT2004 preset still on the default game port (UT99 holds 7777)"
grep -qx 'QueryPort=7801' "$ini" || die "UT2004 preset still on the default query port (UT99 holds 7778)"
[[ $(grep -c '^GameConfig=(GameClass=' "$ini") == 8 ]] || die "UT2004 game type voting not set"
grep -qx 'bMidGameVote=True' "$ini" || die "UT2004 mid-game voting not on"
# UT99: settings in .utpg/System, folders owned by the game server account.
v=/var/lib/gameservers/ut99-classic/.utpg/System
[[ $(stat -c %U "$v") == gameserver && $(stat -c %U "$v/..") == gameserver ]] || die "UT99 settings folders not owned by gameserver"
grep -qx 'ServerPackages=Botpack' "$v/UnrealTournament.ini" || die "the game's own ServerPackages were lost"
grep -qx 'ServerPackages=VoteSys_v21' "$v/UnrealTournament.ini" || die "VoteSys not added to ServerPackages"
grep -qx 'DoUplink=False' "$v/UnrealTournament.ini" || die "UT99 internet uplink not off"
grep -qx 'DefaultPreset=InstaGib/DeathMatch' "$v/VoteSys.ini" || die "VoteSys default preset"
grep -qx 'bEnableCustomDataTransport=True' "$v/VoteSys.ini" || die "VoteSys data link off"
grep -qx 'DataPort=7787' "$v/VoteSys.ini" || die "VoteSys data port not set"
grep -qx 'Game=MonsterHunt.MonsterHunt' "$v/VoteSysPresets.ini" || die "VoteSys presets"
[[ $(grep -c '^PresetName=' "$v/VoteSysPresets.ini") == 9 ]] || die "VoteSys preset count"
grep -qF '"server" "DM-Deck16][?game=Botpack.DeathMatchPlus?mutator=VoteSys_v21.MutVoteSys,Botpack.InstaGibDM" "port=7777" "ini=/var/lib/gameservers/ut99-classic/.utpg/System/UnrealTournament.ini"' /etc/systemd/system/gs-ut99-classic.service || die "UT99 preset command"
python3 - <<'PY' || die "preset list for the control service"
import json
c = json.load(open("/etc/gameservers/presets.json"))
assert c["address"] == "192.0.2.10" and c["port"] == 8790, c
ids = [p["id"] for p in c["presets"]]
assert ids == ["quake3-ffa", "ut2004-instagib", "ut2004-onslaught", "ut99-classic"], ids
by = {p["id"]: p for p in c["presets"]}
assert by["ut2004-instagib"]["query"] == {"type": "ut2004", "port": 7801}, by["ut2004-instagib"]
assert by["ut2004-instagib"]["always_on"] and by["ut99-classic"]["always_on"] and not by["ut2004-onslaught"].get("always_on"), by
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
grep -q 'use-dns: false' "$np" || die "server would use its own cache DNS from DHCP"
grep -q 'addresses: \[192.0.2.1\]' "$np" || die "server does not keep the router as its DNS"
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

step "Battle.net prefix: publish over SSH, as from the real canary"
# This container stands in for the canary too: orchestrator with sudo and
# the claude key, and a golden home with the signed-out prefix.
useradd -m orchestrator; usermod -p '*' orchestrator; useradd -U player
echo 'orchestrator ALL=(ALL:ALL) NOPASSWD: ALL' >/etc/sudoers.d/10-orchestrator
install -d -m 700 -o orchestrator /home/orchestrator/.ssh
install -m 600 -o orchestrator /home/claude/.ssh/id_ed25519.pub /home/orchestrator/.ssh/authorized_keys
gp=/home/.golden/player/.local/share/arcade/prefixes/battlenet
install -d -m 700 /home/.golden; mkdir -p "$gp"; cp -a "$src/." "$gp/"; chown -R player:player /home/.golden/player
/usr/sbin/sshd
out=$(/usr/local/sbin/battlenet-prefix-publish localhost 2>&1) || { echo "$out"; die "publish over SSH failed"; }
echo "$out"
grep -Eq '^[0-9]{8}T[0-9]{6}Z from localhost$' /srv/games/battlenet/prefix/.prefix-version || die "version file after SSH publish"
[[ $(readlink /srv/games/battlenet/prefix/dosdevices/z:) == / ]] || die "drive links not kept over SSH"
cmp -s /srv/games/battlenet/prefix/.prefix-version "$gp/.prefix-version" || die "canary not given the published version"
[[ $(stat -c %U:%G "$gp/.prefix-version") == player:player ]] || die "canary's version file not owned by player"
ok "copies from the canary as orchestrator with the claude key; canary gets the same version"

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
