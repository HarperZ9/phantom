<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/HarperZ9/phantom/main/docs/art/hero-dark.svg">
  <img src="https://raw.githubusercontent.com/HarperZ9/phantom/main/docs/art/hero-light.svg" alt="phantom: Audit and rotate hardware identifiers with exact backup and revert. Bundles of fine lines carry the work through 5 stations, audit, generate, apply, validate and revert, along a sweeping path into a bright core." width="100%">
</picture>

# phantom

Audit and rotate hardware identifiers with exact backup and revert.

[![version: 1.1.1](https://img.shields.io/badge/version-1.1.1-e6e1d6?style=flat-square&labelColor=1a1712)](https://github.com/HarperZ9/phantom/releases/latest)
[![CI](https://github.com/HarperZ9/phantom/actions/workflows/ci.yml/badge.svg)](https://github.com/HarperZ9/phantom/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-Phantom_Proprietary_License-e6e1d6?style=flat-square&labelColor=1a1712)](https://github.com/HarperZ9/phantom/blob/main/LICENSE)

Phantom is a command-line tool for Windows and Linux that shows which hardware identifiers software on your machine can read, and replaces the operating-system identifiers among them with values from a generated profile. It saves every original value before the first write, and `phantom revert` puts them back exactly.

On Windows it changes five registry values: MachineGuid, HwProfileGuid, the SQM MachineId, ProductId and InstallDate. On Linux it changes the machine ID, the hostname and the MAC address of each physical network interface. These changes are machine-wide. Every process and every user sees the new values, and there is no per-application mode.

Identifiers that come from firmware or hardware stay as they are. That covers SMBIOS board and system serials, disk serials, GPU device IDs, TPM details, display EDID, and on Windows the network adapter MAC. Software that reads those directly will still see the real machine. Phantom generates matching values for them in every profile, but no released version applies them. A kernel driver and a UEFI module for that work exist in the source tree as unfinished, unshipped code (see [Scope](#scope)).

Use Phantom on machines you own or are authorized to test. Its Terms of Use prohibit fraud and criminal use, and it does not target anti-cheat or fraud-detection systems.

## Who this is for

- **Privacy researchers** who want to see which identifiers an application reads and how it behaves when they change.
- **QA and device-testing teams** who need software to see a different machine identity without reimaging or keeping a device lab.
- **Penetration testers and red teams** who reset OS-level machine identity between engagements on their own test hosts.
- **Forensic and incident-response analysts** who build examination environments and want to control the OS-level identifiers the examined software sees.

![Eight stages of applying a hardware identity: audit, generate, consistent, capture, backup, write, validate, and revert. The audit reads seven identifier sources and writes nothing. Generate hashes one seed string with SHA-256 and uses the result to key a ChaCha20 stream, so the same seed rebuilds the same identity on any machine. The generated values stay internally consistent: a Samsung disk serial carries Samsung's format, and an Intel network adapter takes one of the registered Intel prefixes. Capture reads each live registry value together with the type it is stored under, because a string written where a number belongs corrupts the value. The backup is saved to disk before a single write happens. Then the identifiers are written: five registry values on Windows, three on Linux. Validate re-reads every source and compares it field by field. Revert restores each original at the type it was recorded under. Three outcomes: originals kept, nothing written, and modeled only.](docs/art/apply-lane.svg)

## What v1.1.1 does

- **Audit.** Reads seven sources (the SMBIOS firmware table, the Windows registry, disks, network adapters, GPUs, display EDID and the TPM) and prints what software can see. It writes nothing and runs without elevation.
- **Generate.** Builds a full identity profile from a seed string. Values follow real vendor formats: a Samsung disk gets a Samsung-style serial, and an Intel network adapter gets a registered Intel MAC prefix. The same seed always produces the same profile.
- **Apply.** Writes the Windows or Linux identifiers listed above, after saving a backup. A second apply keeps the first backup, so revert still returns the true originals. See [What apply changes](#what-apply-changes).
- **Validate.** Re-reads every source and compares it field by field with the profile. It checks the whole profile, so on Windows the firmware, disk, network, GPU and TPM fields will show as mismatches after an apply, because apply does not change them.
- **Revert.** Restores each original value from the backup, at the registry type it was stored under. Uninstalling also reverts first.

Changing these values can have side effects. Software that tied its own licence or enrolment to MachineGuid or the machine ID may treat the machine as new, and on Linux systemd-networkd derives its default DHCP client ID from the machine ID, so your network may hand out a different address.

## How it compares

- **MAC changers** such as Technitium MAC Address Changer on Windows or `macchanger` on Linux change one identifier. Phantom does not change the MAC on Windows, and on Linux it changes the MAC together with the machine ID and hostname, from one profile, with a single revert.
- **`systemd-machine-id-setup` and `hostnamectl`** on Linux can set the same values by hand. Phantom adds the backup, the seeded profile, validation and exact revert.
- **Sysprep** on Windows generalizes an image before it is deployed. It is a one-way step for imaging. Phantom changes a running system and can undo the change.
- **A fresh virtual machine** in Hyper-V, VirtualBox or VMware gives software a new identity at every layer, firmware and disks included. Phantom covers fewer identifiers and works on the physical machine you already use.
- **Browser anti-fingerprinting** in Tor Browser or Brave works on what web pages can see. Phantom works on what installed programs read from the OS.

## Getting started on Windows

### Try it without installing

The release also ships a portable zip, `phantom-v1.1.1-x86_64-pc-windows-msvc.zip`. Unzip it and run `phantom.exe` from a normal terminal. Auditing and working with profiles need no elevation.

```
phantom --version                       # phantom 1.1.1
phantom audit                           # read-only: what software can see
phantom profile generate my-profile --seed "any memorable string"
phantom profile list
phantom validate my-profile             # compare the live system with the profile
```

Before you apply anything, `validate` reports mismatches and exits with status 1, since the live system still carries its original values.

### Install with the MSI

Download `PhantomSetup-v1.1.1.msi` and `SHA256SUMS.txt` from [Releases](https://github.com/HarperZ9/phantom/releases) and check the hash before you run it:

```
certutil -hashfile PhantomSetup-v1.1.1.msi SHA256
```

Compare the output with the matching line in `SHA256SUMS.txt`. The installer is not code-signed yet, so SmartScreen shows "Windows protected your PC". Click **More info**, then **Run anyway**, and accept the UAC prompt and the licence agreement. The MSI installs three programs under `Program Files\Phantom`: the `phantom.exe` CLI, the `phantom-svc.exe` service that reapplies your last applied profile at boot, and the `phantom-tray.exe` status icon.

### Apply and revert

`apply` and `revert` write machine-wide registry keys, so run them from a terminal opened as administrator (right-click, Run as administrator).

```
phantom apply my-profile --layers 2     # elevated
phantom validate my-profile
phantom revert                          # elevated: restore the originals
```

## Getting started on Linux

Phantom ships as a `.deb`, an `.rpm`, and a portable tarball. All three install the CLI, the service binary, and a systemd unit. `apply` and `revert` need root, and setting a MAC needs `iproute2`.

### 1. Install

```
# Debian / Ubuntu
sudo apt-get install ./phantom_1.1.1-1_amd64.deb

# Fedora / RHEL / openSUSE
sudo dnf install ./phantom-1.1.1-1.x86_64.rpm      # or: sudo zypper install ...

# Any distro (portable tarball)
tar -xzf phantom-1.1.1-x86_64-linux.tar.gz && cd phantom-1.1.1-x86_64-linux && sudo ./install.sh
```

Installing enables `phantom.service`, which reapplies your active profile on boot. Nothing changes on your machine until you run an explicit `apply`.

### 2. Use it

```
phantom --version
phantom audit                           # read-only
phantom profile generate my-profile
sudo phantom apply my-profile           # spoof machine-id, hostname, and MAC
phantom validate my-profile
sudo phantom revert                     # restore the originals
```

A spoofed MAC does not survive a reboot on its own, so `phantom.service` reapplies it at boot. machine-id and hostname are file-based and persist on their own. This is verified on a real Linux VM through a power-cycle (see `docs/linux-vm-dogfood.md`). Full install detail is in [docs/linux-install.md](docs/linux-install.md).

### License activation (both platforms)

Phantom runs in **Free** tier immediately, which covers Layer-2 spoofing and up to two profiles. A **Pro** or **Enterprise** key raises the profile limit and unlocks the deferred layers as they ship.

```
phantom license request          # prints an enrollment block to send your licensing contact
phantom license activate <key>   # activate the key they issue back
```

Activation shows the Terms of Use and Privacy Notice. Answer `y` at each prompt (or pass `--accept-tou --acknowledge-privacy-notice` for unattended installs).

![Twelve rows covering what a generated profile holds and where each part gets its shape. The seed is one string, hashed with SHA-256 into a ChaCha20 stream, so the same seed rebuilds the same identity on any machine. Four board vendors, ASUSTeK, Gigabyte, Micro-Star and ASRock, each carrying its own serial prefix, length, character set and BIOS vendor string. Four disk vendors, Samsung, Western Digital, Seagate and Crucial. Four network vendors, Intel, Realtek, Broadcom and Qualcomm, drawn from eighteen registered OUI prefixes rather than random leading bytes. Fourteen GPU models, eight NVIDIA and six AMD, each with the real PCI device id it carries. Four TPM vendors and seven display vendors under the three letter EDID codes the standard assigns. On Windows five registry values are applied: MachineGuid, HwProfileGuid, MachineId, ProductId and InstallDate, four strings and one number. On Linux three identifiers are applied: machine id, hostname, and the MAC of each physical interface. The accented row is the computer name, which is modeled and deliberately not applied, because writing it at the registry level alone desyncs the machine name and breaks shutdown and WMI. The backup is written before any value changes, and a second apply keeps the first one. Profile limits run two, fifty, and unlimited across the three tiers.](docs/art/identity-table.svg)

## What apply changes

At Layer 2, `apply` spoofs the userland-reversible identifiers, the ones an ordinary process reads and that Phantom can change and restore exactly without touching firmware.

**Windows** (five registry identifiers):

| Identifier | Registry location |
|---|---|
| MachineGuid | `HKLM\SOFTWARE\Microsoft\Cryptography\MachineGuid` |
| HwProfileGuid | `HKLM\SYSTEM\CurrentControlSet\Control\IDConfigDB\Hardware Profiles\0001` |
| MachineId | `HKLM\SOFTWARE\Microsoft\SQMClient\MachineId` |
| ProductId | `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProductId` |
| InstallDate | `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\InstallDate` |

ComputerName is modeled but deliberately not applied on Windows: writing it at the registry level alone desyncs the machine name and breaks `shutdown`, `Restart-Computer`, and WMI, so it waits for a full rename path.

**Linux** (three identifiers):

| Identifier | Location |
|---|---|
| machine-id | `/etc/machine-id` and `/var/lib/dbus/machine-id` |
| hostname | `/etc/hostname` and the live `sethostname()` |
| MAC (per physical interface) | `ip link set dev <if> address` |

Hostname is safe to spoof at Layer 2 on Linux, unlike Windows, so `apply` changes it there.

A profile *models* a much wider identifier set (SMBIOS and board serials, disk serials, GPU, TPM, display, boot) so it stays internally consistent as higher layers land. Everything outside the tables above is modeled, not yet applied, because it lives in firmware or the kernel (Layers 0 and 1).

## Tiers and licensing

| Tier | Layers | Saved profiles |
|------|--------|----------------|
| Free | Layer 2 | 2 |
| Pro | All layers as they ship | 50 |
| Enterprise | All layers as they ship | Unlimited |

In 1.1.1 the boot-time reapply service runs on every tier, Free included.

Keys are HMAC-signed and bound to one machine's hardware fingerprint: a key issued for your machine is worthless on any other. `phantom license request` prints the fingerprint and build details your licensing contact needs; they issue a key bound to it. Full details in [docs/user/licensing.md](docs/user/licensing.md).

## CLI reference

```
# Audit (read-only)
phantom audit

# Profiles
phantom profile generate <name> [--seed "memorable-string"]
phantom profile show <name>
phantom profile list
phantom profile export <name> > profile.json
phantom profile import profile.json
phantom profile delete <name>

# Apply / validate / revert  (apply and revert need elevation on Windows, root on Linux)
phantom apply <name> --layers 2
phantom validate <name>
phantom revert
phantom status

# Licensing
phantom license request
phantom license activate <key> [--accept-tou --acknowledge-privacy-notice]
phantom license status
phantom license fingerprint
phantom license deactivate

# Configuration
phantom config show
phantom config path
phantom config set <key> <value>

# Boundary gateway (local model disclosure control)
phantom boundary content-id --session <id> --content request.json
phantom boundary seal --policy policy.unsigned.json --out policy.sealed.json
phantom --json boundary check --policy policy.sealed.json --session <id> --content request.json
phantom boundary serve --policy policy.sealed.json --listen 127.0.0.1:43117

# Legal + integrity
phantom privacy-notice
phantom tou
phantom self-check
phantom tamper-report

# Machine-readable output (stable JSON envelope): add --json to most commands
phantom --json status
phantom --json audit
```

## Configuration

Phantom resolves configuration in this order (highest wins): environment variables (`PHANTOM_*`), then a JSON config file, then compiled defaults.

| Variable | Default (Windows) | Default (Linux) | Description |
|----------|---------|---------|-------------|
| `PHANTOM_DATA_DIR` | `%ProgramData%\Phantom` | `/var/lib/phantom` | Machine-wide store for profiles, logs, config, backup, and license state |
| `PHANTOM_LOG_LEVEL` | `info` | `info` | `trace`, `debug`, `info`, `warn`, `error` |
| `PHANTOM_CONFIG` | `<data_dir>/config.json` | `<data_dir>/config.json` | Alternate config-file location |
| `PHANTOM_TELEMETRY` | `false` | `false` | Opt-in telemetry |

## Privacy and phone-home

Phantom does not phone home unless you set a callback URL. When you do, it sends a minimal, signed payload (an opaque license serial, the tier, the Phantom version, a timestamp, and tripwire counts) at most once per interval, over `curl` so the call is visible to your host tooling. No hardware fingerprint, profile content, or machine identity leaves the machine. Disable it any time with `phantom config set phone_home_enabled false`. See [docs/user/privacy.md](docs/user/privacy.md) and `phantom privacy-notice`.

`phantom boundary` is a separate opt-in local gateway for model clients you configure to use it. The first release supports exact-body approvals for non-streaming `POST /v1/chat/completions` to one numeric loopback upstream and records content-free receipts for one sealed policy session per gateway. It is a first-hop local control, not a promise about provider retention, legal ownership, arbitrary client containment, or the chosen upstream's internal side channels. Post-dispatch upstream errors are transport results, not proof that the upstream did not receive the approved body. See [docs/boundary-gateway.md](docs/boundary-gateway.md).

## Uninstall

On either platform, removal reverts every applied identifier from its backup first, so you are never left with a spoofed identity and no tool to restore it. Your profiles, license, and config remain in the data directory so a reinstall resumes where you left off; delete that directory to wipe everything.

```
# Windows: Settings > Apps, or the MSI
# Debian / Ubuntu
sudo apt-get remove phantom
# Fedora / RHEL
sudo dnf remove phantom
# Tarball install
sudo ./uninstall.sh
```

## System requirements

- **Windows** 10 22H2 or Windows 11 23H2 or newer; Administrator privileges for install, apply, and revert.
- **Linux** with systemd; root for apply and revert; `iproute2` for MAC spoofing.
- x86-64.

![Eight stages of validating an applied identity: sources, firmware, registry, devices, expect, compare, status, and report. Seven readers run, each one independent of the others. The firmware reader parses the SMBIOS table out of its raw bytes rather than asking an operating system service for a summary. The registry reader reads back the five Layer 2 values. The device readers enumerate disks, network adapters, GPUs, displays, and the TPM. Against each reading stands the profile field that identifier is supposed to carry. The two are compared, and each field lands on match, mismatch, or unavailable. The result is a diff a person can read, or the same content in the stable JSON envelope. Three outcomes: match, mismatch, and unavailable.](docs/art/validate-lane.svg)

## Scope

Phantom ships the Layer-2 path on both platforms, verified end to end (audit, generate, apply, validate, revert, reboot persistence, and clean uninstall) on fresh Windows and Linux VMs. It is honest about its edges:

- **Layer 2 only.** Layers 1 (kernel driver) and 0 (UEFI/DXE firmware) are modeled but not shipped. The Layer 1 kernel driver compiles and links in CI, and several memory-safety defects in it are fixed. It does not work end to end: its attach and detach requests return not-implemented, so it intercepts nothing, and its GPU PnP path is never dispatched. It is also unsigned. See `docs/kernel-driver-review.md` and `docs/windows-driver-signing.md`.
- **Unsigned.** The Windows MSI is not yet code-signed; SmartScreen will warn until a signing certificate is in place. The Linux packages ship with published `SHA256SUMS.txt`.
- **Authorized use.** Intended for machines you own or are expressly authorized to test.

## License

Proprietary. See [LICENSE](LICENSE).
