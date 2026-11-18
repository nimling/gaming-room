#!/bin/bash
# Runs inside the test container (see run.sh). Lints, applies seat.yml twice,
# then checks the reset, the token check, golden capture and the gates.
set -euo pipefail

step() { printf '\n=== %s ===\n' "$*"; }
ok() { echo "ok    $*"; }
die() { echo "FAIL  $*" >&2; exit 1; }

step "Install tools"
pacman -Syu --noconfirm --needed \
    ansible ansible-lint shellcheck yamllint \
    btrfs-progs openssh sudo polkit sddm swayidle libnotify pciutils \
    git pacman-contrib util-linux python rsync >/dev/null
ssh-keygen -A >/dev/null

# Work on a writable copy; the repo is mounted read-only.
cp -a /repo /work
cd /work

step "Lint"
mapfile -t scripts < <(grep -rl --exclude='*.j2' '^#!/bin/bash' ansible/roles tests bootstrap)
shellcheck "${scripts[@]}"
ok "shellcheck"
yamllint ansible games
ok "yamllint"
(cd ansible && ansible-lint --offline -c ../.ansible-lint seat.yml update.yml)
ok "ansible-lint"

step "Real btrfs /home on a loop device"
truncate -s 1G /tmp/home.img
mkfs.btrfs -q /tmp/home.img
# The container's /dev is a copy made at start. When the host had no free
# loop device then, the one the kernel adds now has no node here: make it.
loopdev=$(losetup -f)
[[ -b $loopdev ]] || mknod "$loopdev" b 7 "${loopdev#/dev/loop}"
losetup "$loopdev" /tmp/home.img
mount "$loopdev" /home
ok "$(findmnt -no FSTYPE -T /home) on /home"

step "A stand-in game share (rsync daemon)"
mkdir -p /srv/share/mw2/players /srv/share/ut99/System64 /srv/share/ut99/Maps
touch /srv/share/mw2/iw4mp.exe /srv/share/mw2/iw4x.exe /srv/share/mw2/iw4x.dll /srv/share/mw2/players/key
install -m 755 /dev/null /srv/share/ut99/System64/ut-bin; touch '/srv/share/ut99/Maps/DM-Deck16][.unr'
printf '[URL]\nLocalMap=CityIntro.unr\n' >/srv/share/ut99/System64/Default.ini
printf '[DefaultPlayer]\nName=Player\n' >/srv/share/ut99/System64/DefUser.ini
echo "iso=test bp4=test patch=469e" >/srv/share/ut99/.ut99-version
mkdir -p /srv/share/worms/User/Teams; touch /srv/share/worms/WA.exe /srv/share/worms/User/Teams/WG.WGT
mkdir -p /srv/share/serioussam/SamTSE/Bin; touch /srv/share/serioussam/SamTSE/SE1_00.gro; echo "serioussam-test=0" >/srv/share/serioussam/.engine-version
# shellcheck disable=SC2016 # written into the stand-in program, expanded when it runs
printf '#!/bin/sh\necho "ran from $(cd "$(dirname "$0")"; pwd -P) with $*"\n' >/srv/share/serioussam/SamTSE/Bin/SeriousSam; chmod 755 /srv/share/serioussam/SamTSE/Bin/SeriousSam
mkdir -p /srv/share/tmnf/Packs; touch /srv/share/tmnf/TmForever.exe /srv/share/tmnf/Packs/Stadium.pak; echo "setup=test" >/srv/share/tmnf/.tmnf-version
mkdir -p /srv/share/quake3/baseq3 /srv/share/jka/base
touch /srv/share/quake3/baseq3/pak0.pk3; install -m 755 /dev/null /srv/share/quake3/quake3e.x64
echo "quake3e-test=0" >/srv/share/quake3/.engine-version
touch /srv/share/jka/base/assets0.pk3   # no engine yet: the seat must wait
mkdir -p /srv/share/ut2004/System /srv/share/ut2004/Maps
install -m 755 /dev/null /srv/share/ut2004/System/UT2004; touch /srv/share/ut2004/Maps/DM-Rankin.ut2
printf '[URL]\nMap=NvidiaLogo.ut2\n' >/srv/share/ut2004/System/Default.ini
printf '[DefaultPlayer]\nName=Player\n' >/srv/share/ut2004/System/DefUser.ini
echo "iso=test patch=3374" >/srv/share/ut2004/.ut2004-version
# Steam café library: the server's public key and one finished app (on the
# list, with a space in its folder name), plus one app that is not listed.
ssh-keygen -q -t ed25519 -N '' -C cafe-sync@test -f /tmp/cafe_key
cafe=/srv/share/steam-cafe/steamapps
mkdir -p "$cafe/common/Fake Game/bin" "$cafe/common/Fake Game/data" "$cafe/common/Other Game"
cp /tmp/cafe_key.pub /srv/share/steam-cafe/cafe-sync.pub
appmanifest() { # appid buildid installdir
    printf '"AppState"\n{\n\t"appid"\t\t"%s"\n\t"Universe"\t\t"1"\n\t"name"\t\t"%s"\n\t"StateFlags"\t\t"4"\n\t"installdir"\t\t"%s"\n\t"SizeOnDisk"\t\t"1000"\n\t"buildid"\t\t"%s"\n\t"LastOwner"\t\t"0"\n}\n' "$1" "$3" "$3" "$2"
}
appmanifest 4000001 200 "Fake Game" >"$cafe/appmanifest_4000001.acf"
printf '#!/bin/sh\necho fake\n' >"$cafe/common/Fake Game/bin/fakegame"; chmod 755 "$cafe/common/Fake Game/bin/fakegame"
echo "level data, build 200" >"$cafe/common/Fake Game/data/level.pak"
appmanifest 4000002 9 "Other Game" >"$cafe/appmanifest_4000002.acf"
touch "$cafe/common/Other Game/other.bin"
# A Steam game installed on this seat that is not on the list.
mkdir -p "/games/steam/steamapps/common/Local Only"
appmanifest 4000003 5 "Local Only" >/games/steam/steamapps/appmanifest_4000003.acf
echo keep >"/games/steam/steamapps/common/Local Only/keep.txt"
# A WoW install as Battle.net leaves it, including the copier's own WTF,
# which must never reach a seat.
mkdir -p /srv/share/wow/Data/data /srv/share/wow/_forever_/WTF/Account/SOMEONE
printf 'Branch!STRING:0|Version!STRING:0|Product!STRING:0\neu|1.0.0.1|wow_forever\n' >/srv/share/wow/.build.info
echo "game data" >/srv/share/wow/Data/data/data.000
touch /srv/share/wow/_forever_/Wow.exe
echo 'SET accountName "SOMEONE@EXAMPLE.COM"' >/srv/share/wow/_forever_/WTF/Config.wtf
printf 'uid = nobody\ngid = nobody\nuse chroot = no\n[games]\n    path = /srv/share\n    read only = yes\n' >/tmp/rsyncd.conf
rsync --daemon --config=/tmp/rsyncd.conf
ok "rsync://127.0.0.1/games"

step "Apply seat.yml"
ssh-keygen -q -t ed25519 -N '' -f /tmp/admin_key
key=$(cat /tmp/admin_key.pub)
cat > /tmp/vars.yml <<EOF
seat_in_container: true
seat_admin_ssh_keys: ["$key"]
seat_orchestrator_ssh_keys: ["$key"]
seat_orchestrator_source: ["192.0.2.10", "198.51.100.20"]
arcade_voice_server: "192.0.2.10"
seat_display: "3440x1440@144"
seat_gameshare: rsync://127.0.0.1/games
seat_steam_cafe_server: "127.0.0.1"
steam_cafe_apps:
  - { appid: 4000001, name: Fake Game }
  - { appid: 4000004, name: Not On The Server }
EOF
# What bootstrap/seat.sh leaves behind; the default seat_temporary_sudoers removes it.
echo 'installer ALL=(ALL) NOPASSWD: ALL' >/etc/sudoers.d/90-seat-bootstrap
printf '[Autologin]\nSession=plasma\n' >/etc/plasmalogin.conf
mkdir -p /etc/skel/.config/autostart && printf '[Desktop Entry]\nName=Hello\n' >/etc/skel/.config/autostart/cachyos-hello.desktop
run_playbook() {
    (cd ansible && ansible-playbook seat.yml -i localhost, -c local -e target=localhost -e @/tmp/vars.yml)
}
run_playbook
ok "first run"
allow=$(grep '^AllowUsers' /etc/ssh/sshd_config.d/10-gaming-room.conf)
[[ $allow == "AllowUsers admin orchestrator@192.0.2.10 orchestrator@198.51.100.20 cafe-sync@127.0.0.1" ]] || die "wrong $allow"
[[ ! -e /etc/sudoers.d/90-seat-bootstrap ]] || die "temporary sudo rule left behind"
[[ -e /etc/sudoers.d/10-orchestrator ]] || die "orchestrator sudo rule missing"
ok "orchestrator sources and temporary sudo rule"
grep -qx 'User=player' /etc/plasmalogin.conf || die "Plasma Login Manager auto-login: $(cat /etc/plasmalogin.conf)"
grep -qx 'Relogin=false' /etc/plasmalogin.conf || die "Plasma Login Manager relogin"
ok "Plasma Login Manager auto-login"
grep -q 'server: "192.0.2.10"' /etc/arcade/arcade.yaml || die "launcher voice server: $(cat /etc/arcade/arcade.yaml)"
grep -qx 'gameservers: "http://192.0.2.10:8790"' /etc/arcade/arcade.yaml || die "launcher game server list: $(cat /etc/arcade/arcade.yaml)"
grep -qx '3440x1440@144' /etc/seat/display || die "seat display"
grep -q 'arcade-app' /home/.golden/player/.config/autostart/arcade.desktop || die "launcher autostart"
grep -qx 'AutoSuspendAction=0' /home/.golden/player/.config/powerdevilrc || die "auto-suspend still on"
grep -qx 'fullscreen=true' /home/.golden/player/.config/kwinrulesrc || die "launcher full-screen rule"
[[ ! -e /home/.golden/player/.config/autostart/cachyos-hello.desktop ]] || die "CachyOS welcome app still starts"
ok "launcher settings, autostart and no auto-suspend"

step "WoW copied once from the share, personal folders linked into the home"
[[ -f /games/battlenet/wow/Data/data/data.000 && -f /games/battlenet/wow/.build.info ]] || die "WoW not copied"
[[ $(stat -c %U:%G /games/battlenet/wow/_forever_/Wow.exe) == player:games ]] || die "player cannot patch WoW"
[[ $(readlink /games/battlenet/wow/_forever_/WTF) == /home/player/.local/share/arcade/wow/_forever_/WTF ]] || die "WTF not linked: $(ls -la /games/battlenet/wow/_forever_)"
[[ ! -e /home/player/.local/share/arcade/wow/_forever_/WTF/Config.wtf ]] || die "the copier's WTF reached the seat"
[[ -x /usr/local/bin/battlenet-seat ]] || die "battlenet-seat missing"
# A patch by Battle.net must survive the next run.
echo "patched" >/games/battlenet/wow/Data/data/data.000
run_playbook >/dev/null
[[ $(cat /games/battlenet/wow/Data/data/data.000) == patched ]] || die "a later run undid Battle.net's patch"
ok "copied once, owned by the player, WTF linked, patches kept"

step "Idle screens go dark and stay dark"
pd=/home/.golden/player/.config/powerdevilrc
grep -qx 'TurnOffDisplayWhenIdle=true' "$pd" || die "screen does not turn off when idle"
grep -qx 'idle_inhibit: true' /etc/arcade/arcade.yaml || die "launcher does not keep the screen on during games"
grep -qx 'TurnOffDisplayIdleTimeoutSec=300' "$pd" || die "screen does not turn off after 5 minutes"
grep -qx 'DimDisplayIdleTimeoutSec=240' "$pd" || die "screen does not dim before it turns off"
# seat-idle-action with a stub systemctl: an unused session is left alone,
# a used one is reset.
mkdir -p /tmp/idle-bin /tmp/idle-home; chown player /tmp/idle-home
printf '#!/bin/sh\necho "$*" >>/tmp/idle-calls\n' >/tmp/idle-bin/systemctl; chmod 755 /tmp/idle-bin/systemctl
idle() { sudo -u player env PATH="/tmp/idle-bin:$PATH" HOME=/tmp/idle-home /usr/local/libexec/seat-idle-action; }
rm -f /tmp/idle-calls; idle
[[ ! -e /tmp/idle-calls ]] || die "idle reset of a session nobody used (would wake the screen)"
sudo -u player mkdir -p /tmp/idle-home/.cache/arcade; sudo -u player touch /tmp/idle-home/.cache/arcade/session-used
idle
grep -q 'start --no-block seat-reset.service' /tmp/idle-calls || die "used session not reset after idle"
ok "screen off after 5 min; idle reset only after someone used the seat"
for prog in openttd teeworlds xonotic-sdl; do command -v "$prog" >/dev/null || die "$prog not installed"; done
ok "free LAN games installed"
[[ $(readlink /home/.golden/player/.local/share/Steam/steamapps) == /games/steam/steamapps ]] || die "Steam games would land in the home"
[[ $(stat -c '%U %a' /games/steam/steamapps) == "player 2775" ]] || die "player cannot install into /games/steam/steamapps"
ok "Steam installs games into /games"
command -v headsetcontrol >/dev/null || die "HeadsetControl missing"
grep -q 'ExecStart=-/usr/bin/headsetcontrol -s 0' /etc/systemd/system/headset-sidetone.service || die "sidetone service"
grep -q 'ATTRS{idProduct}=="0a9b|0a9c|0a6d"' /etc/udev/rules.d/70-headset-sidetone.rules || die "sidetone udev rule"
ok "G432 sidetone off at boot and plug-in"
grep -qx 'proton: /games/runtime/GE-Proton[0-9-]*-x86_64' /etc/arcade/arcade.yaml || die "launcher Proton path: $(cat /etc/arcade/arcade.yaml)"
grep -qx 'UMU_FOLDERS_PATH=/games/runtime/umu' /etc/environment.d/60-arcade-proton.conf || die "umu folders"
grep -qx 'PRESSURE_VESSEL_FILESYSTEMS_RO=/games' /etc/environment.d/60-arcade-proton.conf || die "runtime container cannot see /games"
[[ $(stat -c %U /games/runtime/umu) == player ]] || die "player cannot write the umu folder"
ok "Proton pinned in /games/runtime, umu outside the home"

step "Games from the share"
[[ -e /games/classic/mw2/iw4x.dll && -x /usr/local/bin/iw4x-seat ]] || die "MW2 not copied from the share"
[[ ! -e /games/classic/mw2/players ]] || die "MW2 players/ came from the share"
[[ -e '/games/classic/ut99/Maps/DM-Deck16][.unr' && -x /games/classic/ut99/System64/ut-bin ]] || die "UT99 not copied from the share, or not executable"
[[ ! -x /games/classic/mw2/iw4x.exe ]] || die "MW2 files became executable"
grep -qx 'LocalMap=CityIntro.unr' /home/.golden/player/.utpg/System/UnrealTournament.ini || die "UT99 defaults not in the golden home"
grep -qx 'Name=Player' /home/.golden/player/.utpg/System/User.ini || die "UT99 user defaults not in the golden home"
[[ -e /games/classic/worms/User/Teams/WG.WGT && -x /usr/local/bin/game-view && ! -e /usr/local/bin/proton-view ]] || die "Worms not copied from the share, or game-view missing"
# game-view builds a writable view in the home; stub umu-run so it stops there.
mkdir -p /tmp/bin; install -d -o player /tmp/pv
install -m 755 /dev/stdin /tmp/bin/umu-run <<'STUB'
#!/bin/sh
echo "umu-run $*"
STUB
view=$(sudo -u player env PATH="/tmp/bin:$PATH" PROTONPATH=GE-Proton HOME=/tmp/pv game-view worms WA.exe 2>&1; cat /tmp/pv/.local/share/arcade/worms.log)
[[ -f /tmp/pv/.local/share/arcade/worms-game/WA.exe && ! -L /tmp/pv/.local/share/arcade/worms-game/WA.exe ]] || die "game-view did not copy the program"
[[ -L /tmp/pv/.local/share/arcade/worms-game/User/Teams/WG.WGT && -w /tmp/pv/.local/share/arcade/worms-game/User/Teams ]] || die "game-view view not linked or not writable"
grep -q 'umu-run ./WA.exe' <<<"$view" || die "game-view did not start the Windows game with umu-run"
# Registry settings from the launcher go into the prefix once, before the game.
printf 'REGEDIT4\n' | sudo -u player tee /tmp/pv/.local/share/arcade/worms.reg >/dev/null
view=$(sudo -u player env PATH="/tmp/bin:$PATH" PROTONPATH=GE-Proton HOME=/tmp/pv game-view worms WA.exe 2>&1; cat /tmp/pv/.local/share/arcade/worms.log)
grep -q 'umu-run regedit /S .game-view.reg' <<<"$view" || die "game-view did not import the registry settings"
[[ -f /tmp/pv/.local/share/arcade/prefixes/worms/game-view.reg && ! -e /tmp/pv/.local/share/arcade/worms-game/.game-view.reg ]] || die "game-view did not remember the import"
sudo -u player env PATH="/tmp/bin:$PATH" PROTONPATH=GE-Proton HOME=/tmp/pv game-view worms WA.exe >/dev/null 2>&1
[[ $(grep -c 'umu-run regedit' /tmp/pv/.local/share/arcade/worms.log) == 1 ]] || die "game-view imported the same registry settings twice"
# A Linux game: the program deep in the folder is a real copy and runs directly.
sudo -u player env HOME=/tmp/pv game-view serioussam SamTSE/Bin/SeriousSam +test 2>&1 || die "game-view could not start a Linux game"
[[ -f /tmp/pv/.local/share/arcade/serioussam-game/SamTSE/Bin/SeriousSam && ! -L /tmp/pv/.local/share/arcade/serioussam-game/SamTSE/Bin/SeriousSam ]] || die "game-view linked the Linux program instead of copying it"
grep -q 'ran from /tmp/pv/.local/share/arcade/serioussam-game/SamTSE/Bin with +test' /tmp/pv/.local/share/arcade/serioussam.log || die "Linux game did not run from the view"
ok "mw2, ut99, worms and serioussam mirrored; UT99 defaults seeded; game-view builds writable views"
[[ -e /games/classic/tmnf/Packs/Stadium.pak && ! -x /games/classic/tmnf/TmForever.exe ]] || die "TrackMania not copied from the share, or files executable"
ok "mw2, ut99 and tmnf mirrored; UT99 defaults seeded"
[[ -x /games/classic/quake3/quake3e.x64 && -e /games/classic/quake3/baseq3/pak0.pk3 ]] || die "Quake 3 not copied from the share, or not executable"
[[ ! -e /games/classic/jka ]] || die "Jedi Academy copied before the server added the engine"
ok "mw2, ut99 and quake3 mirrored; UT99 defaults seeded; jka waits"
[[ -e /games/classic/ut2004/Maps/DM-Rankin.ut2 && -x /games/classic/ut2004/System/UT2004 ]] || die "UT2004 not copied from the share, or not executable"
grep -qx 'Map=NvidiaLogo.ut2' /home/.golden/player/.ut2004/System/UT2004.ini || die "UT2004 defaults not in the golden home"
grep -qx 'Name=Player' /home/.golden/player/.ut2004/System/User.ini || die "UT2004 user defaults not in the golden home"
ok "mw2, ut99 and ut2004 mirrored; UT99 and UT2004 defaults seeded"

step "Steam café library: the seat copies listed apps from the share"
lib=/games/steam/steamapps
[[ -x "$lib/common/Fake Game/bin/fakegame" ]] || die "café game not copied, or not executable"
grep -qx 'level data, build 200' "$lib/common/Fake Game/data/level.pak" || die "café game data not copied"
grep -q '"buildid"[[:space:]]*"200"' "$lib/appmanifest_4000001.acf" || die "café appmanifest not written"
[[ $(stat -c '%U:%G' "$lib/appmanifest_4000001.acf" "$lib/common/Fake Game/data/level.pak" | sort -u) == player:games ]] || die "café files not owned by player:games"
[[ ! -e $lib/appmanifest_4000002.acf && ! -e "$lib/common/Other Game" ]] || die "an app that is not on the list was copied"
grep -qx keep "$lib/common/Local Only/keep.txt" && [[ -e $lib/appmanifest_4000003.acf ]] || die "a local app that is not on the list was touched"
[[ -x /usr/local/sbin/steam-cafe && -x /usr/local/libexec/cafe-sync-serve && -x /usr/bin/rrsync ]] || die "café scripts or rrsync missing"
ak=/var/lib/cafe-sync/.ssh/authorized_keys
[[ $(cat "$ak") == "from=\"127.0.0.1\",restrict,command=\"/usr/local/libexec/cafe-sync-serve\" $(cat /tmp/cafe_key.pub)" ]] || die "cafe-sync key not restricted: $(cat "$ak")"
id -nG cafe-sync | tr ' ' '\n' | grep -qx games && die "cafe-sync is in the games group (could write the library)"
# For the second run: a lower build on the server, with other data, must
# change nothing on the seat.
appmanifest 4000001 150 "Fake Game" >"$cafe/appmanifest_4000001.acf"
echo "older data, build 150 (different size)" >"$cafe/common/Fake Game/data/level.pak"
ok "listed app copied (game files, then appmanifest); unlisted apps untouched; key limited to the server"

step "Self-update"
grep -q 'ansible-pull --clean ' /etc/systemd/system/seat-pull.service || die "seat-pull can get stuck on its shallow clone (no --clean)"
ok "seat-pull resets its checkout to the branch"

step "Mumble is pre-filled in the golden home"
mumble_conf=/home/.golden/player/.config/Mumble/Mumble/mumble_settings.json
mumble_cert=/home/.golden/player/Documents/MumbleAutomaticCertificateBackup.p12
python -m json.tool "$mumble_conf" >/dev/null || die "Mumble settings are not valid JSON"
grep -q '"audio_wizard_has_been_shown": true' "$mumble_conf" || die "Mumble audio wizard not marked done"
grep -q '"transmit_mode": "PTT"' "$mumble_conf" || die "Mumble push-to-talk"
grep -q '"frames_per_packet": 1' "$mumble_conf" || die "Mumble 10 ms frames"
grep -q '"enable_evdev": true' "$mumble_conf" || die "Mumble evdev push-to-talk"
grep -q '"AAAAAgAAAAEb"' "$mumble_conf" || die "Mumble push-to-talk on the mouse side button"
id -nG player | tr ' ' '\n' | grep -qx input || die "player not in the input group (push-to-talk)"
[[ $(stat -c '%U %a' "$mumble_cert") == "player 600" ]] || die "Mumble seat certificate owner or mode"
openssl pkcs12 -in "$mumble_cert" -passin pass: -nokeys 2>/dev/null | grep -q 'BEGIN CERTIFICATE' || die "Mumble seat certificate unreadable"
[[ $(stat -c '%U %a' /etc/seat/mumble/key.pem) == "root 600" ]] || die "Mumble seat key owner or mode"
grep -qx 'wmclass=Mumble' /home/.golden/player/.config/kwinrulesrc || die "Mumble start-minimised rule"
grep -qxF "XDG_DOCUMENTS_DIR=\"\$HOME/Documents\"" /home/.golden/player/.config/user-dirs.dirs || die "XDG Documents folder not fixed"
[[ -d /home/.golden/player/Documents ]] || die "Documents folder missing in the golden home"
ok "settings, seat certificate and window rule"

step "Second run changes nothing"
second=$(run_playbook)
echo "$second" | tail -n 4
echo "$second" | grep -Eq "changed=0 +unreachable=0 +failed=0" || die "second run changed something"
ok "idempotent"
grep -qx 'level data, build 200' "$lib/common/Fake Game/data/level.pak" || die "a lower server build replaced the seat's copy"
grep -q '"buildid"[[:space:]]*"200"' "$lib/appmanifest_4000001.acf" || die "a lower server build replaced the appmanifest"
ok "a lower build on the server changes nothing"

step "Steam café library: newer builds, busy seats, and the server's read-only access"
pull_cafe() {
    /usr/local/sbin/steam-cafe pull --share rsync://127.0.0.1/games/steam-cafe/steamapps --library "$lib" \
        --apps 4000001,4000004 --owner player --group games --session-marker /home/player/.cache/arcade/session-used
}
appmanifest 4000001 300 "Fake Game" >"$cafe/appmanifest_4000001.acf"
rm "$cafe/common/Fake Game/data/level.pak"; echo "build 300" >"$cafe/common/Fake Game/data/new.pak"
# A process named steam stands in for a running Steam client.
cp /usr/bin/sleep /tmp/steam; /tmp/steam 300 & fake_steam=$!
out=$(pull_cafe); echo "$out"
grep -q '^skipped: Steam is running' <<<"$out" || die "copied while Steam runs"
[[ -e "$lib/common/Fake Game/data/level.pak" ]] || die "files changed while Steam runs"
kill "$fake_steam"; wait "$fake_steam" 2>/dev/null || true
out=$(pull_cafe); echo "$out"
grep -q '^4000001: copied build 300 (was 200)' <<<"$out" || die "newer build not copied"
[[ -e "$lib/common/Fake Game/data/new.pak" && ! -e "$lib/common/Fake Game/data/level.pak" ]] || die "newer build not mirrored (--delete)"
grep -q '"buildid"[[:space:]]*"300"' "$lib/appmanifest_4000001.acf" || die "appmanifest not updated"
grep -qx keep "$lib/common/Local Only/keep.txt" || die "unlisted local app touched"
ok "newer build copied and mirrored; nothing copied while Steam runs"

# The server's side, with this container as the seat: sshd, cafe-sync's
# forced command and rrsync, and the real collect script over SSH.
"$(command -v sshd)" -E /tmp/sshd.log
ssh_cafe() {
    ssh -i /tmp/cafe_key -o BatchMode=yes -o StrictHostKeyChecking=accept-new \
        -o UserKnownHostsFile=/tmp/cafe_known_hosts cafe-sync@127.0.0.1 "$@"
}
cafe_over_ssh() {
    [[ $(ssh_cafe status) == "steam=stopped game=stopped" ]] || die "cafe-sync status: $(ssh_cafe status 2>&1)"
    /tmp/steam 300 & fake_steam=$!
    [[ $(ssh_cafe status) == "steam=running game=stopped" ]] || die "cafe-sync status misses a running Steam"
    kill "$fake_steam"; wait "$fake_steam" 2>/dev/null || true
    if ssh_cafe cat /etc/passwd >/dev/null 2>&1; then die "cafe-sync ran a command of its own"; fi
    if rsync -e "ssh -i /tmp/cafe_key -o BatchMode=yes -o UserKnownHostsFile=/tmp/cafe_known_hosts" \
        /etc/hostname cafe-sync@127.0.0.1:x >/dev/null 2>&1; then die "cafe-sync could write to the Steam library"; fi
    [[ ! -e $lib/x ]] || die "cafe-sync wrote into the Steam library"
    if rsync -e "ssh -i /tmp/cafe_key -o BatchMode=yes -o UserKnownHostsFile=/tmp/cafe_known_hosts" \
        cafe-sync@127.0.0.1:../../../etc/hostname /tmp/escaped >/dev/null 2>&1; then die "cafe-sync read outside the Steam library"; fi
    printf '%s\n' '{"apps": [{"appid": 4000001, "name": "Fake Game"}, {"appid": 4000003, "name": "Local Only"}],
     "seats": [{"name": "seat-test", "address": "127.0.0.1"}],
     "dest": "/tmp/cafe-server/steamapps", "user": "cafe-sync",
     "ssh_key": "/tmp/cafe_key", "known_hosts": "/tmp/cafe_known_hosts",
     "presence_url": "", "reserve_gib": 0}' >/tmp/cafe-collect.json
    out=$(/usr/local/sbin/steam-cafe collect --config /tmp/cafe-collect.json) || { echo "$out"; die "collect over SSH failed"; }
    echo "$out"
    grep -q '^4000001 Fake Game: copied build 300 from seat-test (new on the server)' <<<"$out" || die "collect did not copy over SSH"
    grep -q '^4000003 Local Only: copied build 5 from seat-test' <<<"$out" || die "collect did not copy the second app"
    [[ -x "/tmp/cafe-server/steamapps/common/Fake Game/bin/fakegame" && -e /tmp/cafe-server/steamapps/appmanifest_4000001.acf ]] || die "collected copy incomplete"
    out=$(/usr/local/sbin/steam-cafe collect --config /tmp/cafe-collect.json); echo "$out"
    grep -q '^4000001 Fake Game: up to date (build 300)' <<<"$out" || die "collect copied an unchanged app again"
    ok "status line, no shell, no writes, no escape; collect copies over SSH and then stays put"
}
# Under amd64 emulation (Apple Silicon) QEMU has no seccomp, so sshd drops
# every connection. Native runners (CI) do the SSH part.
if ! ssh_cafe status >/dev/null 2>&1 && grep -q 'PR_SET_SECCOMP' /tmp/sshd.log; then
    echo "skip  SSH part: sshd's seccomp sandbox cannot run under emulation here"
else
    cafe_over_ssh
fi

step "Player home is a fresh btrfs snapshot"
btrfs subvolume show /home/player >/dev/null || die "/home/player is not a subvolume"
btrfs subvolume show /home/.golden/player >/dev/null || die "golden home is not a subvolume"
[[ $(stat -c %U /home/player) == player ]] || die "home not owned by player"
ok "subvolumes in place"

step "Simulate a session with logins"
runuser -u player -- mkdir -p /home/player/.local/share/Steam/config /home/player/.config/legendary
runuser -u player -- bash -c 'printf "\"users\"\n{\n\"7656\" { \"AccountName\" \"someone\" }\n}\n" > ~/.local/share/Steam/config/loginusers.vdf'
runuser -u player -- bash -c 'echo "{}" > ~/.config/legendary/user.json'
runuser -u player -- touch /home/player/saved-game.txt
mkdir -p /games/steam/steamapps/compatdata/12345
chown -R player:games /games/steam/steamapps
if /usr/local/sbin/seat-token-check; then die "token check missed the fake logins"; fi
ok "token check finds the fake logins"

# WoW and Battle.net in the same session.
runuser -u player -- mkdir -p /home/player/.local/share/arcade/wow/_forever_/WTF /home/player/.local/share/arcade/prefixes/battlenet
runuser -u player -- bash -c 'echo "SET accountName \"SOMEONE@EXAMPLE.COM\"" > ~/.local/share/arcade/wow/_forever_/WTF/Config.wtf'
runuser -u player -- bash -c 'printf "[Software\\\\Wine\\\\Credential Manager\\\\Generic: Battle.net] 1700000000\n" > ~/.local/share/arcade/prefixes/battlenet/user.reg'
[[ -f /games/battlenet/wow/_forever_/WTF/Config.wtf ]] || die "WoW's WTF does not lead into the home"
# Battle.net or WoW replaced a link with a real folder.
rm /games/battlenet/wow/_forever_/Logs
runuser -u player -- mkdir /games/battlenet/wow/_forever_/Logs
runuser -u player -- touch /games/battlenet/wow/_forever_/Logs/chat.log
out=$(/usr/local/sbin/seat-token-check || true)
grep -q 'WTF/Config.wtf' <<<"$out" || die "token check missed the WoW account name: $out"
grep -q 'battlenet/user.reg' <<<"$out" || die "token check missed the Battle.net credential: $out"
if /usr/local/sbin/seat-token-check; then die "token check missed the fake logins"; fi
ok "token check finds the WoW and Battle.net logins"

step "Golden capture refuses a home with logins"
if /usr/local/sbin/seat-golden-capture; then die "capture accepted a home with logins"; fi
ok "capture refused"

step "End session"
/usr/local/sbin/seat-reset session
[[ ! -e /home/player/saved-game.txt ]] || die "player files survived"
[[ ! -e /games/steam/steamapps/compatdata/12345 ]] || die "Proton prefix survived the reset"
[[ ! -e /home/player/.local/share/Steam/config/loginusers.vdf ]] || die "Steam login survived the reset"
[[ $(readlink /home/player/.local/share/Steam/steamapps) == /games/steam/steamapps ]] || die "Steam library link missing after reset"
# A game installed in this session, through the link, lands in /games and
# survives the next End session.
runuser -u player -- mkdir -p /home/player/.local/share/Steam/steamapps/common/SomeGame
runuser -u player -- touch /home/player/.local/share/Steam/steamapps/appmanifest_12345.acf
[[ -e /games/steam/steamapps/appmanifest_12345.acf ]] || die "Steam game did not land in /games"
/usr/local/sbin/seat-reset session
[[ -d /games/steam/steamapps/common/SomeGame && -e /games/steam/steamapps/appmanifest_12345.acf ]] || die "Steam game deleted by the reset"
btrfs subvolume show /home/player >/dev/null || die "home is no longer a subvolume"
[[ ! -e /home/player/.local/share/arcade/prefixes/battlenet/user.reg ]] || die "Battle.net login survived the reset"
[[ ! -e /games/battlenet/wow/_forever_/WTF/Config.wtf ]] || die "WoW account name survived the reset"
[[ -L /games/battlenet/wow/_forever_/Logs && ! -e /games/battlenet/wow/_forever_/Logs/chat.log ]] || die "real WoW Logs folder survived the reset"
[[ -f /games/battlenet/wow/Data/data/data.000 ]] || die "WoW deleted by the reset"
ok "home replaced; Steam games kept, Steam login and Proton prefixes gone"
ok "Battle.net login and WoW personal folders gone; WoW kept"

step "Gate: token leak"
bash tests/gates/token-leak.sh

step "Gate: player lockdown"
bash tests/gates/player-lockdown.sh

step "Golden capture accepts a clean home"
runuser -u player -- touch /home/player/new-default.txt
/usr/local/sbin/seat-golden-capture
/usr/local/sbin/seat-reset session
[[ -e /home/player/new-default.txt ]] || die "captured file missing after reset"
ok "capture and reset round-trip"

step "Tier detection"
/usr/local/sbin/seat-tier
grep -qx '[ABC]' /etc/seat/tier || die "no tier written"
ok "tier $(cat /etc/seat/tier)"
# Intel Arc: VRAM only from Vulkan. Recorded on seat-03 (A770 next to a Ryzen iGPU).
SEAT_TIER_VULKANINFO=tests/container/fixtures/vulkaninfo-arc-a770.txt /usr/local/sbin/seat-tier >/dev/null
grep -q '"gpu_vendor":"intel"' /etc/seat/hardware.json || die "Arc not detected: $(cat /etc/seat/hardware.json)"
grep -q '"vram_mib":16288' /etc/seat/hardware.json || die "Arc VRAM wrong: $(cat /etc/seat/hardware.json)"
ok "Intel Arc VRAM from Vulkan, iGPU ignored"
/usr/local/sbin/seat-tier >/dev/null

step "Update freeze"
echo "2999-01-01" > /etc/seat/update-freeze-until
/usr/local/sbin/seat-update | grep -q "frozen until" || die "freeze ignored"
rm /etc/seat/update-freeze-until
ok "freeze respected"

step "Reset refuses without a golden home"
mv /home/.golden/player /home/.golden/player.bak
if /usr/local/sbin/seat-reset session; then die "reset ran without a golden home"; fi
[[ -d /home/player ]] || die "home was deleted without a golden home"
mv /home/.golden/player.bak /home/.golden/player
ok "home kept"

printf '\nALL CHECKS PASSED\n'
