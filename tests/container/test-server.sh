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
apt-get install -y -qq ansible ansible-lint yamllint shellcheck openssh-server sudo python3 iproute2 >/dev/null
mkdir -p /run/sshd

cp -a /repo /work
cd /work

step "Lint"
shellcheck bootstrap/seat.sh tests/gates/server-services.sh tests/container/test-server.sh tests/container/run-server.sh
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
    seats:
      hosts:
        seat-01:
          seat_mac: "00:00:5e:00:53:01"
        seat-02:
EOF
run() { (cd ansible && ansible-playbook server.yml -i /tmp/inventory.yml); }
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
