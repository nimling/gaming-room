#!/bin/bash
# One-time step at a new seat, so Ansible can take over from the server.
# Run it once at the seat, from the seat's normal (sudo) account:
#
#   curl -fsSL https://raw.githubusercontent.com/nimling/gaming-room/main/bootstrap/seat.sh | sudo bash -s -- nimsim seat-01
#
# Arguments: the GitHub user whose public SSH keys may log in, and the
# seat's name. It installs and starts OpenSSH, adds those keys for the
# account that ran sudo, sets the host name, and lets that account use
# sudo without a password until the first Ansible run deletes the rule
# (/etc/sudoers.d/90-seat-bootstrap, listed in seat_temporary_sudoers).
# It prints what Claude needs. The seat playbook does the rest.
set -euo pipefail

die() { echo "bootstrap: $*" >&2; exit 1; }

github_user=${1:-}
seat_name=${2:-}
[[ $github_user =~ ^[A-Za-z0-9-]{1,39}$ ]] || die "usage: seat.sh <github-user> <seat-name, like seat-01>"
[[ $seat_name =~ ^seat-[0-9]{2}$ ]] || die "seat name must look like seat-01"
[[ $EUID -eq 0 ]] || die "run it with sudo"
user=${SUDO_USER:-}
[[ -n $user && $user != root ]] || die "run it with sudo from your normal account, not as root"
command -v pacman >/dev/null || die "this is for CachyOS seats"

echo "Installing OpenSSH"
pacman -S --needed --noconfirm openssh >/dev/null

echo "Fetching the public SSH keys of github.com/$github_user"
keys=$(curl -fsSL "https://github.com/$github_user.keys") || die "could not fetch keys"
grep -Eq '^(ssh-(ed25519|rsa)|ecdsa-sha2-nistp[0-9]+|sk-ssh-ed25519@openssh.com) ' <<<"$keys" \
    || die "github.com/$github_user has no public SSH keys"

home=$(getent passwd "$user" | cut -d: -f6)
group=$(id -gn "$user")
install -d -m 700 -o "$user" -g "$group" "$home/.ssh"
auth="$home/.ssh/authorized_keys"
touch "$auth"
added=0
while IFS= read -r key; do
    [[ -z $key ]] && continue
    if ! grep -qxF "$key" "$auth"; then
        echo "$key" >>"$auth"
        added=$((added + 1))
    fi
done <<<"$keys"
chown "$user:$group" "$auth"
chmod 600 "$auth"
echo "Added $added key(s) for $user"

hostnamectl set-hostname "$seat_name"
systemctl enable --now sshd >/dev/null 2>&1
# CachyOS turns ufw on by default and it drops SSH. Open SSH to the seat's
# own network only (the gaming VLAN, where the server is). Phase 1 then makes
# sshd key-only and limits who may log in.
lan=$(ip -4 route show scope link | awk '!/linkdown/ {print $1; exit}')
if [[ -n $lan ]] && command -v ufw >/dev/null 2>&1 && ufw status | grep -q 'Status: active'; then
    ufw allow from "$lan" to any port 22 proto tcp >/dev/null && echo "Opened SSH in the firewall for $lan"
fi

# Temporary: the first Ansible run uses this account with sudo, then deletes
# this file as its very last task.
rule=/etc/sudoers.d/90-seat-bootstrap
printf '# Temporary, from bootstrap/seat.sh. The first Ansible run deletes it.\n%s ALL=(ALL) NOPASSWD: ALL\n' "$user" >"$rule.tmp"
chmod 0440 "$rule.tmp"
visudo -cf "$rule.tmp" >/dev/null || die "the sudo rule did not validate"
mv "$rule.tmp" "$rule"

route_dev=$(ip -4 route show default | awk '{print $5; exit}')
address=$(ip -4 -brief address show dev "${route_dev:-lo}" | awk '{print $3}' | cut -d/ -f1 | head -n 1)
mac=$(cat "/sys/class/net/${route_dev:-lo}/address" 2>/dev/null || true)
fstype=$(findmnt -no FSTYPE /)
echo
echo "Done. Send this to Claude:"
echo "  seat:    $seat_name"
echo "  user:    $user"
echo "  address: ${address:-unknown}"
echo "  mac:     ${mac:-unknown}"
echo "  disk:    $fstype"
