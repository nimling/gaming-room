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
