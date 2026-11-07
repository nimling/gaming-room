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
    git pacman-contrib util-linux python >/dev/null
ssh-keygen -A >/dev/null

# Work on a writable copy; the repo is mounted read-only.
cp -a /repo /work
cd /work

step "Lint"
mapfile -t scripts < <(grep -rl '^#!/bin/bash' ansible/roles tests)
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

step "Apply seat.yml"
ssh-keygen -q -t ed25519 -N '' -f /tmp/admin_key
key=$(cat /tmp/admin_key.pub)
cat > /tmp/vars.yml <<EOF
seat_in_container: true
seat_admin_ssh_keys: ["$key"]
seat_orchestrator_ssh_keys: ["$key"]
EOF
run_playbook() {
    (cd ansible && ansible-playbook seat.yml -i localhost, -c local -e target=localhost -e @/tmp/vars.yml)
}
run_playbook
ok "first run"

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
btrfs subvolume show /home/player >/dev/null || die "home is no longer a subvolume"
ok "home replaced"

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
