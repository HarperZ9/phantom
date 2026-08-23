#!/bin/sh
# Phantom portable install. For distros without a .deb or .rpm, or for a
# quick manual install. Needs root: it writes /usr/local, sets a MAC
# (CAP_NET_ADMIN), and installs a systemd unit.
#
# It installs the two binaries to /usr/local and then calls
# `phantom-svc --install`, which writes the systemd unit with the real
# installed path and enables the boot-time reapply.
set -e

BIN_DIR=/usr/local/bin
SBIN_DIR=/usr/local/sbin
SRC_DIR="$(cd "$(dirname "$0")" && pwd)"

if [ "$(id -u)" != "0" ]; then
    echo "This installer must run as root. Re-run with sudo." >&2
    exit 1
fi

if [ ! -f "$SRC_DIR/phantom" ] || [ ! -f "$SRC_DIR/phantom-svc" ]; then
    echo "phantom and phantom-svc must sit next to this script." >&2
    exit 1
fi

install -D -m 0755 "$SRC_DIR/phantom" "$BIN_DIR/phantom"
install -D -m 0755 "$SRC_DIR/phantom-svc" "$SBIN_DIR/phantom-svc"

echo "Installed:"
echo "  $BIN_DIR/phantom"
echo "  $SBIN_DIR/phantom-svc"
echo

# Writes the unit pointing at the installed phantom-svc, then enables it.
"$SBIN_DIR/phantom-svc" --install

echo
echo "Next: 'phantom license activate <key>', then 'phantom apply <profile>'."
echo "Setting a MAC needs iproute2; install it if 'ip' is missing."
