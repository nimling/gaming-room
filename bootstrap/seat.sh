#!/bin/bash
# One-time step at a new seat, so Ansible can take over from the server.
# Run it once at the seat, from the seat's normal (sudo) account:
#
#   curl -fsSL https://raw.githubusercontent.com/nimling/gaming-room/main/bootstrap/seat.sh | sudo bash -s -- nimsim seat-01
#
# Arguments: the GitHub user whose public SSH keys may log in, and the
# seat's name. It installs and starts OpenSSH, adds those keys for the
# account that ran sudo, sets the host name, and prints the address.
# It changes nothing else; the seat playbook does the rest.
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

address=$(ip -4 -brief address show scope global | awk '{print $3}' | cut -d/ -f1 | head -n 1)
echo
echo "Done. Send this to Claude:"
echo "  seat:    $seat_name"
echo "  user:    $user"
echo "  address: ${address:-unknown}"
