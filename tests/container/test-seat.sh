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
mount -o loop /tmp/home.img /home
ok "$(findmnt -no FSTYPE -T /home) on /home"

step "A stand-in game share (rsync daemon)"
mkdir -p /srv/share/mw2/players /srv/share/ut99/System64 /srv/share/ut99/Maps
touch /srv/share/mw2/iw4mp.exe /srv/share/mw2/iw4x.exe /srv/share/mw2/iw4x.dll /srv/share/mw2/players/key
install -m 755 /dev/null /srv/share/ut99/System64/ut-bin; touch '/srv/share/ut99/Maps/DM-Deck16][.unr'
printf '[URL]\nLocalMap=CityIntro.unr\n' >/srv/share/ut99/System64/Default.ini
printf '[DefaultPlayer]\nName=Player\n' >/srv/share/ut99/System64/DefUser.ini
echo "iso=test bp4=test patch=469e" >/srv/share/ut99/.ut99-version
mkdir -p /srv/share/worms/User/Teams; touch /srv/share/worms/WA.exe /srv/share/worms/User/Teams/WG.WGT
mkdir -p /srv/share/ut2004/System /srv/share/ut2004/Maps
install -m 755 /dev/null /srv/share/ut2004/System/UT2004; touch /srv/share/ut2004/Maps/DM-Rankin.ut2
printf '[URL]\nMap=NvidiaLogo.ut2\n' >/srv/share/ut2004/System/Default.ini
printf '[DefaultPlayer]\nName=Player\n' >/srv/share/ut2004/System/DefUser.ini
echo "iso=test patch=3374" >/srv/share/ut2004/.ut2004-version
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
[[ $allow == "AllowUsers admin orchestrator@192.0.2.10 orchestrator@198.51.100.20" ]] || die "wrong $allow"
[[ ! -e /etc/sudoers.d/90-seat-bootstrap ]] || die "temporary sudo rule left behind"
[[ -e /etc/sudoers.d/10-orchestrator ]] || die "orchestrator sudo rule missing"
ok "orchestrator sources and temporary sudo rule"
grep -qx 'User=player' /etc/plasmalogin.conf || die "Plasma Login Manager auto-login: $(cat /etc/plasmalogin.conf)"
grep -qx 'Relogin=false' /etc/plasmalogin.conf || die "Plasma Login Manager relogin"
ok "Plasma Login Manager auto-login"
grep -q 'server: "192.0.2.10"' /etc/arcade/arcade.yaml || die "launcher voice server: $(cat /etc/arcade/arcade.yaml)"
grep -qx '3440x1440@144' /etc/seat/display || die "seat display"
grep -q 'arcade-app' /home/.golden/player/.config/autostart/arcade.desktop || die "launcher autostart"
grep -qx 'AutoSuspendAction=0' /home/.golden/player/.config/powerdevilrc || die "auto-suspend still on"
grep -qx 'fullscreen=true' /home/.golden/player/.config/kwinrulesrc || die "launcher full-screen rule"
[[ ! -e /home/.golden/player/.config/autostart/cachyos-hello.desktop ]] || die "CachyOS welcome app still starts"
ok "launcher settings, autostart and no auto-suspend"
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
[[ -e /games/classic/worms/User/Teams/WG.WGT && -x /usr/local/bin/proton-view ]] || die "Worms not copied from the share, or proton-view missing"
# proton-view builds a writable view in the home; stub umu-run so it stops there.
mkdir -p /tmp/bin; install -d -o player /tmp/pv
install -m 755 /dev/stdin /tmp/bin/umu-run <<'STUB'
#!/bin/sh
echo "umu-run $*"
STUB
view=$(sudo -u player env PATH="/tmp/bin:$PATH" PROTONPATH=GE-Proton HOME=/tmp/pv proton-view worms WA.exe 2>&1; cat /tmp/pv/.local/share/arcade/worms.log)
[[ -f /tmp/pv/.local/share/arcade/worms-game/WA.exe && ! -L /tmp/pv/.local/share/arcade/worms-game/WA.exe ]] || die "proton-view did not copy the program"
[[ -L /tmp/pv/.local/share/arcade/worms-game/User/Teams/WG.WGT && -w /tmp/pv/.local/share/arcade/worms-game/User/Teams ]] || die "proton-view view not linked or not writable"
grep -q 'umu-run ./WA.exe' <<<"$view" || die "proton-view did not start the game with umu-run"
ok "mw2, ut99 and worms mirrored; UT99 defaults seeded; proton-view builds a writable view"
[[ -e /games/classic/ut2004/Maps/DM-Rankin.ut2 && -x /games/classic/ut2004/System/UT2004 ]] || die "UT2004 not copied from the share, or not executable"
grep -qx 'Map=NvidiaLogo.ut2' /home/.golden/player/.ut2004/System/UT2004.ini || die "UT2004 defaults not in the golden home"
grep -qx 'Name=Player' /home/.golden/player/.ut2004/System/User.ini || die "UT2004 user defaults not in the golden home"
ok "mw2, ut99 and ut2004 mirrored; UT99 and UT2004 defaults seeded"

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
ok "home replaced; Steam games kept, Steam login and Proton prefixes gone"

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
