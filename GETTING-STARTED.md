# Getting Started with Phantom

Phantom reads the hardware identifiers software uses to fingerprint your
machine, generates realistic replacement identities, and applies them where it
can reverse the change cleanly.

Phantom is for machines you own or are authorized to test. It serves
penetration testers rotating identity between assessments, QA teams validating
software across hardware profiles, privacy researchers studying device
fingerprinting, and enterprise IT teams provisioning standardized identities. It
does not target anti-cheat systems, game services, or fraud-detection
infrastructure.

## Prerequisites

**Windows:** Windows 10 22H2 or Windows 11 23H2, x86-64. Administrator account
for apply and revert. ~30 MB disk space.

**Linux:** Any x86-64 distribution with systemd. Root for apply and revert.
`iproute2` for MAC spoofing (the `.deb` and `.rpm` packages pull it in).

No internet connection is required.

## Installation

### Windows MSI

Download `PhantomSetup-v1.1.0.msi` and `SHA256SUMS.txt` from
[Releases](https://github.com/HarperZ9/phantom/releases/latest). Verify the
hash before running:

```
certutil -hashfile PhantomSetup-v1.1.0.msi SHA256
```

Compare the output against `SHA256SUMS.txt`. Double-click the MSI. The installer
is unsigned, so SmartScreen shows "Windows protected your PC." Click **More
info**, then **Run anyway**. Accept the UAC prompt and the license agreement.

Confirm with `phantom --version`. If the shell does not find the command, close
the terminal and open a new one (PATH changes take effect in new sessions only).

### Linux

```sh
# Debian / Ubuntu
sudo apt-get install ./phantom_1.1.0-1_amd64.deb

# Fedora / RHEL
sudo dnf install ./phantom-1.1.0-1.x86_64.rpm

# Portable tarball (any distro)
tar -xzf phantom-1.1.0-x86_64-linux.tar.gz
cd phantom-1.1.0-x86_64-linux
sudo ./install.sh
```

Verify tarballs with `sha256sum -c SHA256SUMS.txt` before extracting. All three
install paths enable `phantom.service` for boot. The service does nothing until
you apply a profile.

## Step 1: Audit your machine

`phantom audit` is read-only. It reports every hardware identifier software can
see.

```
> phantom audit

  Phantom Hardware Identity Audit
  ============================================================

  [SMBIOS Firmware Table]
    BoardManufacturer                   ASUSTeK COMPUTER INC.
    BoardProduct                        ROG STRIX B550-F GAMING
    BoardSerial                         MB-0283917AK4
    SystemUUID                          A4E1F9C2-38D7-11EC-8D3D-0242AC130003

  [Windows Registry]
    MachineGuid                         {a1b2c3d4-e5f6-7890-abcd-ef1234567890}
    HwProfileGuid                       {12345678-abcd-ef01-2345-6789abcdef01}
    MachineId                           {98765432-1abc-def0-1234-56789abcdef0}
    ProductId                           00331-10000-00001-AA123
    InstallDate                         1672531200
    ComputerName                        DESKTOP-A7B3C9D

  [Disk Identifiers]
    Disk0_Model                         Samsung SSD 970 EVO Plus
    Disk0_Serial                        S4EVNF0M812345K

  [Network Adapters]
    Ethernet0_MAC                       04:D4:C4:5A:BC:12
    WiFi0_MAC                           9C:B6:D0:EF:78:34

  [GPU Devices]
    GPU0_VendorID                       10DE
    GPU0_DeviceID                       2484

  [TPM Module]
    TPM_Manufacturer                    INTC (Intel)
    TPM_SpecVersion                     2.0

  Total identifiers read: 23
  These values uniquely identify this machine to any software that queries them.
```

The **Windows Registry** section lists the five identifiers Phantom can spoof at
Layer 2. SMBIOS, Disk, GPU, Display, and TPM require Layer 1 or Layer 0 (not
shipped yet). On Linux, the audit shows machine-id, hostname, and MACs instead
of registry keys.

## Step 2: Generate a profile

```
> phantom profile generate lab-1

  Profile 'lab-1' generated and saved.
  38 identity vectors across all modeled layers.
```

Phantom builds an internally consistent identity across the full hardware tree.
Samsung disk serials follow Samsung's format. Intel MACs use real Intel OUI
prefixes. To make a profile reproducible, pass a seed:

```
phantom profile generate lab-1 --seed "seattle-testrig-04"
```

Inspect with `phantom profile show lab-1`. The output lists every modeled
identifier: SMBIOS, disks, NICs, GPUs, and the OS-level keys. The Windows/Linux
section at the bottom shows what Layer 2 will write. Everything above it is
modeled for future layers.

## Step 3: Apply the profile

On Windows, open an elevated terminal (right-click > Run as administrator). On
Linux, prefix with `sudo`.

```
> phantom apply lab-1 --layers 2

  Applying profile 'lab-1' to 1 layer(s)...

  Layer 2 (Registry/Userland) - 5 identifiers applied
    + SOFTWARE\Microsoft\Cryptography\MachineGuid
    + SYSTEM\CurrentControlSet\Control\IDConfigDB\Hardware Profiles\0001
    + SOFTWARE\Microsoft\SQMClient\MachineId
    + SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProductId
    + SOFTWARE\Microsoft\Windows NT\CurrentVersion\InstallDate

  Backup written to %ProgramData%\Phantom\backup.json
```

On Linux, `apply` writes machine-id, hostname, and the MAC of each physical
interface, with backup at `/var/lib/phantom/backup.json`.

If you lack elevation, apply prints an error and exits without partial writes. A
reboot after apply is recommended: some services cache identifiers at startup.

## Step 4: Validate

```
> phantom validate lab-1

  Phantom Validation Report
  ============================================================

  Checked:       38
  Matching:      5
  Mismatched:    0
  Unavailable:   33

  Result: CONSISTENT
  All readable identifiers match the active profile.
```

The 5 matching entries are the Layer-2 identifiers. The 33 unavailable entries
need Layer 1 or Layer 0 to spoof. Expected on v1.1.0. A mismatch means a value
did not stick; check that you ran apply with elevation.

## Step 5: Revert

```
> phantom revert

  Reverting from backup...
  5 identifiers restored to original values.
  Backup cleared.
```

Run from an elevated terminal (Windows) or with `sudo` (Linux). Phantom reads
the backup from apply and writes back every original value exactly.

## Profile management

```
phantom profile list                      # list saved profiles
phantom profile export lab-1 > lab-1.json # export to JSON
phantom profile import lab-1.json         # import from JSON
phantom profile delete lab-1              # remove a profile
```

Free tier stores two profiles. Pro allows 50. Enterprise has no limit.

## Licensing

Phantom runs in Free tier immediately after install. Free is a real tier: it
applies the full Layer-2 identity set and stores two profiles.

**Request a key** on the machine you want licensed:

```
> phantom license request

  Machine Fingerprint: 9f3a...c7e2
  Requested Tier:      pro
  Phantom Version:     1.1.0
  Current Tier:        free

  Send this block to your Phantom licensing contact.
```

The enrollment block contains no personal data. Send it to your licensing
contact; they return a key bound to your fingerprint.

**Activate:**

```
phantom license activate AEA6Y-UAAAD-HFAAA-...-QOZ7R-K
```

The tool shows the Terms of Use and Privacy Notice, each requiring a `y`
confirmation. For unattended installs, pass `--accept-tou` and
`--acknowledge-privacy-notice`.

**Check status:**

```
> phantom license status

  Tier:    Pro
  Serial:  PH-2026-00042
  Expires: 2027-08-24
```

| Tier | Layers | Profiles | Background service |
|------|--------|----------|--------------------|
| Free | Layer 2 | 2 | No |
| Pro | All shipped layers | 50 | Yes |
| Enterprise | All shipped layers | Unlimited | Yes |

Keys are HMAC-signed and bound to one machine's hardware fingerprint. A key
issued for one device does nothing on another.

## Caveats

**Layer 2 only.** Phantom v1.1.0 spoofs userland identifiers: registry keys on
Windows, machine-id/hostname/MAC on Linux. Applications that read raw device
serials through kernel APIs see the real hardware. Layer 1 (kernel driver) is
compiled and partially reviewed, but unsigned and not functional end to end.
Layer 0 (UEFI/DXE) is modeled only.

**Unsigned installer.** The Windows MSI triggers SmartScreen on every install.
Verify the MSI hash against `SHA256SUMS.txt` before running. Linux packages are
also not GPG-signed; verify with `sha256sum -c`.

**ComputerName is modeled, not applied, on Windows.** Writing the registry key
alone desyncs the machine name and breaks shutdown and WMI. Hostname spoofing
works on Linux, where `sethostname()` is sufficient.

**Profile scope exceeds applied scope.** Each profile models ~38 identity
vectors. Layer 2 applies 5 on Windows and 3 on Linux. The rest exist so the
identity stays coherent as higher layers ship.

## Quick reference

```
phantom audit                             # read-only identifier scan
phantom profile generate <name>           # create identity
phantom profile generate <name> --seed s  # reproducible identity
phantom profile show <name>               # inspect a profile
phantom apply <name> --layers 2           # apply (elevated)
phantom validate <name>                   # check consistency
phantom revert                            # restore originals (elevated)
phantom status                            # current state
phantom license request                   # enrollment for a key
phantom license activate <key>            # activate
phantom --json audit                      # machine-readable output
```

Full CLI reference: [README](https://github.com/HarperZ9/phantom#readme).
Privacy: `phantom privacy-notice`. Terms: `phantom tou`.
