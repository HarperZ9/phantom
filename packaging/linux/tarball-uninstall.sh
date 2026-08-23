#!/bin/sh
# Phantom portable uninstall. Reverts the machine to its true identity,
# removes the systemd unit, and deletes the installed binaries. Needs root.
#
# Profiles, license, and the backup under /var/lib/phantom are left in
# place so a reinstall picks the setup back up. Remove that directory by
# hand for a full wipe.
set -e

BIN_DIR=/usr/local/bin
SBIN_DIR=/usr/local/sbin

if [ "$(id -u)" != "0" ]; then
    echo "This uninstaller must run as root. Re-run with sudo." >&2
    exit 1
fi

if [ -x "$SBIN_DIR/phantom-svc" ]; then
    # Restore the true identity BEFORE removing the binary that can do it.
    "$SBIN_DIR/phantom-svc" --cleanup || true
    "$SBIN_DIR/phantom-svc" --uninstall || true
fi

rm -f "$BIN_DIR/phantom" "$SBIN_DIR/phantom-svc"

echo "Phantom removed. Profiles and license kept under /var/lib/phantom."
