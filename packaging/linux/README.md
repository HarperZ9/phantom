# Linux packaging

Builds the Linux distribution artifacts from prebuilt release binaries: a
Debian `.deb`, an RPM `.rpm`, and a portable tarball with an install script.
One script, standard distro tools (`dpkg-deb`, `rpmbuild`), no third-party
packager.

## Build

```sh
# From a release build: cargo build --release --workspace
bash packaging/linux/build-packages.sh v1.0.0 target/x86_64-unknown-linux-gnu/release out
ls out
#   phantom_1.0.0-1_amd64.deb
#   phantom-1.0.0-1.x86_64.rpm
#   phantom-1.0.0-x86_64-linux.tar.gz
```

`rpmbuild` is optional. Without it the `.rpm` is skipped and the `.deb` and
tarball still build. On Debian and Ubuntu, `apt-get install rpm` provides it.

The release workflow runs this script on a version tag and attaches all three
artifacts, plus `SHA256SUMS.txt`, to the GitHub Release.

## What the packages install

| Path | Contents |
|---|---|
| `/usr/bin/phantom` | the CLI |
| `/usr/sbin/phantom-svc` | the service binary (`--reapply` at boot) |
| `/lib/systemd/system/phantom.service` (deb), `/usr/lib/...` (rpm) | the unit |
| `/usr/share/doc/phantom/` | LICENSE, README, CHANGELOG |

`Depends`/`Requires`: `iproute2` (for setting a MAC) and the C runtime.

## Maintainer scripts

The install and removal hooks carry the reversibility guarantee:

- **On install:** enable `phantom.service` so a spoofed MAC is reapplied on the
  next boot. Nothing is started or spoofed until an explicit `phantom apply`.
- **On removal (not upgrade):** run `phantom-svc --cleanup`, which reverts every
  layer from the backup and restores the machine's true machine-id, hostname,
  and MAC BEFORE the binaries are removed, then disable the unit. This is the
  Sev-1 bar: a package removal must never leave the machine on a spoofed
  identity. An upgrade keeps the identity; the replacement package carries it
  forward.

The Debian scripts are `deb-postinst`, `deb-prerm`, `deb-postrm`. The RPM
scriptlets live in `rpm-spec.in` and mirror them in RPM's argument convention
(`$1 == 0` is a final erase, `$1 == 1` is a first install).

Profiles, license, and the backup under `/var/lib/phantom` are left in place on
removal, so a reinstall picks the setup back up. A full wipe of that directory
is a manual step.

## Verification done

- The `.deb` install and removal cycle is dogfooded on a systemd host: install
  enables the unit, removal reverts and disables it.
- The `.rpm` is built and its scriptlets, file list, and dependencies inspected.

Still owed (Phase 3 remainder): a rootful-VM CI job that exercises the real
apply path, and a power-cycle dogfood confirming a spoofed MAC returns after a
real reboot.
