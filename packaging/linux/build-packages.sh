#!/usr/bin/env bash
# Build the Linux distribution artifacts for Phantom from prebuilt release
# binaries: a Debian .deb, an RPM .rpm, and a portable tarball with an
# install script. Hand-rolled with dpkg-deb and rpmbuild so no third-party
# packaging tool joins the build.
#
# Usage:
#   build-packages.sh <version> <bindir> <outdir>
#
#   <version>  release version, with or without a leading v (e.g. v1.0.0,
#              1.0.0, or a prerelease like v1.0.0-rc1)
#   <bindir>   directory holding the built phantom-cli (or phantom) and
#              phantom-svc, e.g. target/x86_64-unknown-linux-gnu/release
#   <outdir>   where the .deb, .rpm, and .tar.gz are written
#
# rpmbuild is optional: if it is not installed, the .rpm is skipped with a
# warning and the .deb and tarball still build.
set -euo pipefail

if [ "$#" -ne 3 ]; then
    echo "usage: $0 <version> <bindir> <outdir>" >&2
    exit 2
fi

VERSION_RAW="$1"
BIN_DIR="$2"
OUT_DIR="$3"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
UNIT_SRC="$REPO_ROOT/dist/systemd/phantom.service"
ARCH_DEB="amd64"
ARCH_RPM="x86_64"
MAINTAINER="Zain Dana Harper <17142659+HarperZ9@users.noreply.github.com>"

# --- resolve inputs ---------------------------------------------------------

# Accept phantom or phantom-cli for the CLI binary; the package always names
# it phantom.
CLI_BIN=""
if [ -f "$BIN_DIR/phantom" ]; then
    CLI_BIN="$BIN_DIR/phantom"
elif [ -f "$BIN_DIR/phantom-cli" ]; then
    CLI_BIN="$BIN_DIR/phantom-cli"
else
    echo "error: no phantom or phantom-cli binary in $BIN_DIR" >&2
    exit 1
fi
SVC_BIN="$BIN_DIR/phantom-svc"
if [ ! -f "$SVC_BIN" ]; then
    echo "error: no phantom-svc binary in $BIN_DIR" >&2
    exit 1
fi
if [ ! -f "$UNIT_SRC" ]; then
    echo "error: systemd unit not found at $UNIT_SRC" >&2
    exit 1
fi

# --- version math -----------------------------------------------------------

# Strip a leading v. UPSTREAM is the part before any -suffix; SUFFIX is the
# prerelease tail (rc1, beta2, ...) or empty.
RAW="${VERSION_RAW#v}"
UPSTREAM="${RAW%%-*}"
if [ "$RAW" = "$UPSTREAM" ]; then
    SUFFIX=""
else
    SUFFIX="${RAW#*-}"
fi

# Debian: a prerelease sorts before the release, so map - to ~. Append the
# Debian revision -1.
if [ -n "$SUFFIX" ]; then
    DEB_VERSION="${UPSTREAM}~${SUFFIX}-1"
else
    DEB_VERSION="${UPSTREAM}-1"
fi

# RPM: Version carries no dash. A prerelease goes in Release as 0.<suffix>
# so it sorts before the 1 of the final release.
RPM_VERSION="$UPSTREAM"
if [ -n "$SUFFIX" ]; then
    RPM_RELEASE="0.${SUFFIX}"
else
    RPM_RELEASE="1"
fi

# A stable, filename-friendly form for the tarball.
PKG_VERSION="$RAW"

mkdir -p "$OUT_DIR"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

echo "==> version: raw=$RAW deb=$DEB_VERSION rpm=$RPM_VERSION-$RPM_RELEASE"
echo "==> cli=$CLI_BIN svc=$SVC_BIN"

# --- Debian package ---------------------------------------------------------

echo "==> building .deb"
DEB_ROOT="$STAGE/deb"
mkdir -p "$DEB_ROOT/DEBIAN" \
         "$DEB_ROOT/usr/bin" \
         "$DEB_ROOT/usr/sbin" \
         "$DEB_ROOT/lib/systemd/system" \
         "$DEB_ROOT/usr/share/doc/phantom"

install -m 0755 "$CLI_BIN" "$DEB_ROOT/usr/bin/phantom"
install -m 0755 "$SVC_BIN" "$DEB_ROOT/usr/sbin/phantom-svc"
install -m 0644 "$UNIT_SRC" "$DEB_ROOT/lib/systemd/system/phantom.service"
install -m 0644 "$REPO_ROOT/LICENSE" "$DEB_ROOT/usr/share/doc/phantom/LICENSE"
install -m 0644 "$REPO_ROOT/README.md" "$DEB_ROOT/usr/share/doc/phantom/README.md"
install -m 0644 "$REPO_ROOT/CHANGELOG.md" "$DEB_ROOT/usr/share/doc/phantom/CHANGELOG.md"

install -m 0755 "$SCRIPT_DIR/deb-postinst" "$DEB_ROOT/DEBIAN/postinst"
install -m 0755 "$SCRIPT_DIR/deb-prerm" "$DEB_ROOT/DEBIAN/prerm"
install -m 0755 "$SCRIPT_DIR/deb-postrm" "$DEB_ROOT/DEBIAN/postrm"

cat > "$DEB_ROOT/DEBIAN/control" <<EOF
Package: phantom
Version: $DEB_VERSION
Section: admin
Priority: optional
Architecture: $ARCH_DEB
Depends: libc6, iproute2
Maintainer: $MAINTAINER
Homepage: https://github.com/HarperZ9/phantom
Description: Hardware identity privacy tool
 Phantom spoofs the userland hardware identifiers that software reads to
 fingerprint a machine: the systemd machine ID, the hostname, and the MAC
 of each physical network interface. Every change is backed up first and
 reverts exactly, so the machine returns to its true identity on request
 or on removal. A spoofed MAC does not survive a reboot on its own, so a
 oneshot systemd unit reapplies the active profile at boot.
 .
 Authorized use only: machines you own or are expressly authorized to test.
EOF

DEB_OUT="$OUT_DIR/phantom_${DEB_VERSION}_${ARCH_DEB}.deb"
dpkg-deb --root-owner-group --build "$DEB_ROOT" "$DEB_OUT"
echo "    wrote $DEB_OUT"

# --- RPM package ------------------------------------------------------------

if command -v rpmbuild >/dev/null 2>&1; then
    echo "==> building .rpm"
    RPMTOP="$STAGE/rpmbuild"
    mkdir -p "$RPMTOP"/{SOURCES,SPECS,BUILD,BUILDROOT,RPMS,SRPMS}

    install -m 0755 "$CLI_BIN" "$RPMTOP/SOURCES/phantom"
    install -m 0755 "$SVC_BIN" "$RPMTOP/SOURCES/phantom-svc"
    install -m 0644 "$UNIT_SRC" "$RPMTOP/SOURCES/phantom.service"
    install -m 0644 "$REPO_ROOT/LICENSE" "$RPMTOP/SOURCES/LICENSE"
    install -m 0644 "$REPO_ROOT/README.md" "$RPMTOP/SOURCES/README.md"
    install -m 0644 "$REPO_ROOT/CHANGELOG.md" "$RPMTOP/SOURCES/CHANGELOG.md"

    RPM_DATE="$(date -u +"%a %b %d %Y")"
    sed -e "s/@VERSION@/$RPM_VERSION/g" \
        -e "s/@RELEASE@/$RPM_RELEASE/g" \
        -e "s/@DATE@/$RPM_DATE/g" \
        "$SCRIPT_DIR/rpm-spec.in" > "$RPMTOP/SPECS/phantom.spec"

    rpmbuild --define "_topdir $RPMTOP" --define "dist %{nil}" -bb "$RPMTOP/SPECS/phantom.spec"
    find "$RPMTOP/RPMS" -name '*.rpm' -exec cp {} "$OUT_DIR/" \;
    echo "    wrote $(find "$OUT_DIR" -name 'phantom-*.rpm')"
else
    echo "==> rpmbuild not found; skipping .rpm (install the 'rpm' package to build it)"
fi

# --- portable tarball -------------------------------------------------------

echo "==> building tarball"
TAR_NAME="phantom-${PKG_VERSION}-x86_64-linux"
TAR_ROOT="$STAGE/tar/$TAR_NAME"
mkdir -p "$TAR_ROOT"

install -m 0755 "$CLI_BIN" "$TAR_ROOT/phantom"
install -m 0755 "$SVC_BIN" "$TAR_ROOT/phantom-svc"
install -m 0644 "$UNIT_SRC" "$TAR_ROOT/phantom.service"
install -m 0755 "$SCRIPT_DIR/tarball-install.sh" "$TAR_ROOT/install.sh"
install -m 0755 "$SCRIPT_DIR/tarball-uninstall.sh" "$TAR_ROOT/uninstall.sh"
install -m 0644 "$REPO_ROOT/LICENSE" "$TAR_ROOT/LICENSE"
install -m 0644 "$REPO_ROOT/README.md" "$TAR_ROOT/README.md"
install -m 0644 "$REPO_ROOT/CHANGELOG.md" "$TAR_ROOT/CHANGELOG.md"

TAR_OUT="$OUT_DIR/${TAR_NAME}.tar.gz"
tar -czf "$TAR_OUT" -C "$STAGE/tar" "$TAR_NAME"
echo "    wrote $TAR_OUT"

echo "==> done. artifacts in $OUT_DIR:"
ls -1 "$OUT_DIR"
