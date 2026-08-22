# Installing Phantom on Linux

Phantom ships three ways on Linux: a Debian `.deb`, an RPM `.rpm`, and a
portable tarball. All three install the same two binaries, a systemd unit that
reapplies a spoofed MAC on boot, and the docs. Every install path needs root.

Phantom spoofs machine-id, hostname, and the MAC of each physical interface.
Setting a MAC needs `iproute2`; the packages depend on it.

Authorized use only: machines you own or are expressly authorized to test.

## Debian and Ubuntu (.deb)

```sh
sudo apt-get install ./phantom_<version>_amd64.deb
```

`apt-get` pulls in the `iproute2` dependency. Installing enables
`phantom.service` for the next boot; it does not change your identity until you
apply a profile.

## Fedora, RHEL, and openSUSE (.rpm)

```sh
sudo dnf install ./phantom-<version>.x86_64.rpm     # or: sudo zypper install ...
```

## Portable tarball (any distro)

```sh
tar -xzf phantom-<version>-x86_64-linux.tar.gz
cd phantom-<version>-x86_64-linux
sudo ./install.sh
```

The tarball installer copies the binaries to `/usr/local` and installs the
systemd unit. Remove it later with `sudo ./uninstall.sh`.

## First use

```sh
phantom license activate <key>
phantom generate <profile> --seed <seed>
sudo phantom apply <profile>
phantom validate <profile>       # confirm the spoof took
```

`apply` records the profile as active. On the next boot, `phantom.service`
reapplies it so the spoofed MAC returns (machine-id and hostname persist on
their own).

## Verify your download

```sh
sha256sum -c SHA256SUMS.txt
```

## Uninstall

Removal restores your true identity before the binaries go away.

```sh
sudo apt-get remove phantom        # Debian/Ubuntu
sudo dnf remove phantom            # Fedora/RHEL
sudo ./uninstall.sh                # tarball install
```

Your profiles and license under `/var/lib/phantom` are kept so a reinstall
picks your setup back up. Delete that directory by hand for a full wipe.
