#!/usr/bin/env bash
# Rootful integration test for the Linux userland apply path.
#
# Runs the real `phantom apply`, `validate`, and `revert` against machine-id and
# hostname, isolated in a private mount + UTS + network namespace so the host's
# identity is never touched: the identity files are bind-mounted with throwaway
# copies, the hostname change lives in the UTS namespace, and the network
# namespace has only loopback. This exercises the real write, backup, validate,
# and restore path that the unit tests cannot.
#
# The network namespace is not optional. Without it, apply runs in the host's
# network namespace and finds the host's real NIC (which has a
# /sys/class/net/*/device link), then spoofs its MAC and drops the host off the
# network. A fresh network namespace has only loopback, so apply finds no
# physical interface and the MAC path is a no-op. MAC spoofing itself is verified
# in the VM dogfood, where a reboot can prove the reapply.
#
# Usage:
#   sudo bash scripts/linux-apply-integration.sh <phantom-binary>
# It re-execs itself under `unshare` if it is not already namespaced.
set -euo pipefail

PHANTOM_BIN="${1:?usage: $0 <phantom-binary>}"
SELF="$(readlink -f "$0")"
PHANTOM_BIN="$(readlink -f "$PHANTOM_BIN")"

# Enter a private mount + UTS namespace once, then continue. The marker stops an
# infinite re-exec loop.
if [ "${PHANTOM_IT_NS:-}" != "1" ]; then
    exec env PHANTOM_IT_NS=1 unshare --mount --uts --net --fork bash "$SELF" "$PHANTOM_BIN"
fi

# Keep our bind mounts from propagating back to the host.
mount --make-rprivate / 2>/dev/null || true

# /sys must reflect THIS network namespace. Without a fresh sysfs mount,
# /sys/class/net still lists the host's interfaces, and apply would try to spoof
# a NIC that is not in this namespace. After remounting, only loopback is here,
# so apply finds no physical interface and the MAC path is a clean no-op.
mount -t sysfs sysfs /sys 2>/dev/null || true
ip link set lo up 2>/dev/null || true

work="$(mktemp -d)"
cleanup() {
    umount /etc/machine-id 2>/dev/null || true
    umount /var/lib/dbus/machine-id 2>/dev/null || true
    umount /etc/hostname 2>/dev/null || true
    rm -rf "$work" 2>/dev/null || true
}
trap cleanup EXIT

# Bind a throwaway copy over each identity file that exists, so `apply` writes
# land on our copy and the host file is untouched. apply only writes a
# machine-id path that already exists, so create /etc/machine-id if missing.
[ -e /etc/machine-id ] || : > /etc/machine-id
printf 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n' > "$work/machine-id"
mount --bind "$work/machine-id" /etc/machine-id

# /var/lib/dbus/machine-id is usually a symlink to /etc/machine-id; isolating
# /etc/machine-id already covers it. Bind it only when it is a separate file.
if [ -e /var/lib/dbus/machine-id ] && [ ! -L /var/lib/dbus/machine-id ]; then
    printf 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n' > "$work/dbus-machine-id"
    mount --bind "$work/dbus-machine-id" /var/lib/dbus/machine-id
fi

[ -e /etc/hostname ] || : > /etc/hostname
printf 'phantom-it-orig\n' > "$work/hostname"
mount --bind "$work/hostname" /etc/hostname

export PHANTOM_DATA_DIR="$work/data"

orig_mid="$(cat /etc/machine-id)"
orig_host="$(cat /etc/hostname)"
echo "baseline:      machine-id=$orig_mid hostname=$orig_host"

"$PHANTOM_BIN" profile generate ci-it --seed phantom-apply-it >/dev/null
echo "== apply =="
"$PHANTOM_BIN" apply ci-it --layers 2

new_mid="$(cat /etc/machine-id)"
new_host="$(cat /etc/hostname)"
live_host="$(hostname)"
echo "after apply:   machine-id=$new_mid hostname=$new_host live=$live_host"

[ "$new_mid" != "$orig_mid" ] || { echo "FAIL: machine-id did not change"; exit 1; }
[ "$new_host" != "$orig_host" ] || { echo "FAIL: hostname file did not change"; exit 1; }
[ "$live_host" = "$new_host" ] || { echo "FAIL: live hostname ($live_host) != file ($new_host)"; exit 1; }

echo "== validate (expect consistent, exit 0) =="
"$PHANTOM_BIN" validate ci-it

echo "== revert =="
"$PHANTOM_BIN" revert

rev_mid="$(cat /etc/machine-id)"
rev_host="$(cat /etc/hostname)"
live_after="$(hostname)"
echo "after revert:  machine-id=$rev_mid hostname=$rev_host live=$live_after"

[ "$rev_mid" = "$orig_mid" ] || { echo "FAIL: machine-id not restored"; exit 1; }
[ "$rev_host" = "$orig_host" ] || { echo "FAIL: hostname file not restored"; exit 1; }
[ "$live_after" = "$orig_host" ] || { echo "FAIL: live hostname not restored"; exit 1; }

echo "LINUX APPLY INTEGRATION PASSED"
