# Phantom

Hardware identity privacy for authorized Windows systems.

Every application on your machine can read dozens of unique hardware identifiers: motherboard serials, disk serials, MAC addresses, GPU device IDs, TPM module IDs, and Windows registry GUIDs. Software uses these to fingerprint your device, track it across reinstalls, and build a permanent hardware dossier without your knowledge or consent.

Phantom audits what your machine reveals, generates internally consistent identity profiles, and applies them to the Windows registry identifiers software reads most often. It backs up every original value before it writes, and restores them exactly on revert or uninstall.

Phantom is for machines you own or are expressly authorized to test. It is not a tool for evading fraud controls or misrepresenting a device you do not control.

## What v1.0.0 does

- **Audit.** Read and report every hardware identifier software can see on this machine, across SMBIOS, disk, network, GPU, TPM, display, Windows registry, and boot categories. Nothing is modified.
- **Generate.** Build a realistic, internally consistent identity profile from a seed. Samsung disk serials match Samsung's format; Intel MACs use real Intel OUI prefixes. One seed reproduces the same identity every time.
- **Apply (Layer 2, registry).** Spoof the five Windows registry identifiers that carry the most fingerprinting weight (see [What apply changes](#what-apply-changes)).
- **Validate.** Confirm every identifier source reports consistently after an apply.
- **Revert.** Restore the exact original values, from a backup written before the first change.

Layer 1 (kernel driver) and Layer 0 (UEFI/DXE firmware) are modeled but not shipped in v1.0.0. See [Scope](#scope).

## Getting started

### 1. Download and verify

Download `PhantomSetup-v1.0.0.msi` from [Releases](https://github.com/HarperZ9/phantom/releases), along with `SHA256SUMS.txt`.

Verify the download before running it:

```
certutil -hashfile PhantomSetup-v1.0.0.msi SHA256
```

Compare the output to the matching line in `SHA256SUMS.txt`.

### 2. Install

Double-click the MSI. The installer is not yet code-signed, so Windows SmartScreen shows "Windows protected your PC". Click **More info**, then **Run anyway**. Accept the UAC prompt and the license agreement. Installation takes under 30 seconds.

Three components install to `C:\Program Files\Phantom\`:

| Component | Purpose |
|-----------|---------|
| `phantom.exe` | CLI for audit, profiles, apply/revert, licensing, and configuration |
| `phantom-svc.exe` | Background service (`PhantomService`) that re-applies your active profile across reboots |
| `phantom-tray.exe` | System-tray status indicator, launched at login |

### 3. Confirm the install

```
phantom --version
sc query PhantomService
```

`--version` reports `phantom 1.0.0`; the service state reads `RUNNING`.

### 4. Audit what your machine reveals

```
phantom audit
```

This prints every readable hardware identifier, grouped by category. It changes nothing, so run it first to see your starting exposure.

### 5. Activate a license (optional)

Phantom runs in **Free** tier immediately, which covers Layer-2 registry spoofing and up to two profiles. A **Pro** or **Enterprise** key raises the profile limit and unlocks the deferred layers as they ship. See [Tiers and licensing](#tiers-and-licensing).

```
phantom license request          # prints an enrollment block to send your licensing contact
phantom license activate <key>   # activate the key they issue back
```

Activation shows the Terms of Use and Privacy Notice. Answer `y` at each prompt (or pass `--accept-tou --acknowledge-privacy-notice` for unattended installs). Check status any time with `phantom license status`.

### 6. Generate and apply a profile

Applying writes machine-wide registry keys, so run these from an **elevated** terminal (right-click > Run as administrator):

```
phantom profile generate my-profile
phantom apply my-profile --layers 2
```

`apply` prints each registry value it changed and backs up the originals first.

### 7. Validate and revert

```
phantom validate my-profile   # every applied identifier should read consistently
phantom revert                # restore the exact original values
```

## What apply changes

At Layer 2, v1.0.0 spoofs these five Windows registry identifiers:

| Identifier | Registry location |
|---|---|
| MachineGuid | `HKLM\SOFTWARE\Microsoft\Cryptography\MachineGuid` |
| HwProfileGuid | `HKLM\SYSTEM\CurrentControlSet\Control\IDConfigDB\Hardware Profiles\0001` |
| MachineId | `HKLM\SOFTWARE\Microsoft\SQMClient\MachineId` |
| ProductId | `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProductId` |
| InstallDate | `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\InstallDate` |

A profile *models* a much wider identifier set (below) so it stays internally consistent as higher layers land. Everything outside the table above is modeled, not yet applied. ComputerName is modeled but deliberately not applied: writing it at the registry level alone desyncs the machine name and breaks `shutdown`, `Restart-Computer`, and WMI, so it waits for a full rename path in a later release.

| Category | Identifiers | Applied in v1.0.0 |
|----------|------------|---|
| Windows registry | MachineGuid, HwProfileGuid, MachineId, ProductId, InstallDate | Yes (Layer 2) |
| Windows registry | ComputerName | Modeled; deferred (needs a full rename) |
| SMBIOS | Board serial, BIOS UUID, system serial, chassis asset tag, manufacturer, product name | Modeled (Layer 0) |
| Disk | ATA serial, model, firmware revision, volume serial, volume GUID | Modeled (Layers 0/1) |
| Network | Permanent MAC, current MAC, adapter GUID | Modeled (Layers 0/1) |
| GPU | PCI vendor/device ID, PnP instance ID, driver key GUID | Modeled (Layer 1) |
| TPM | Module serial, manufacturer ID | Modeled (Layer 1) |
| Display | EDID serial, manufacturer code, product code | Modeled (Layer 1) |
| Boot | BCD identifier GUID, boot disk signature | Modeled (Layer 0) |

## Tiers and licensing

| Tier | Layers | Profiles | Background service |
|------|--------|----------|--------------------|
| Free | Layer 2 (registry) | 2 | No |
| Pro | All layers as they ship | 50 | Yes |
| Enterprise | All layers as they ship | Unlimited | Yes |

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

# Apply / validate / revert  (apply and revert require an elevated shell)
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

# Background service
phantom service ping
phantom service status

# Configuration
phantom config show
phantom config path
phantom config init
phantom config set <key> <value>

# Legal + integrity
phantom privacy-notice
phantom tou
phantom self-check
phantom tamper-report

# Machine-readable output (stable JSON envelope): add --json to most commands
phantom --json status
phantom --json audit
phantom --json license status
```

## Configuration

Phantom resolves configuration in this order (highest wins): environment variables (`PHANTOM_*`), then a JSON config file, then compiled defaults.

| Variable | Default | Description |
|----------|---------|-------------|
| `PHANTOM_DATA_DIR` | `%ProgramData%\Phantom` | Machine-wide store for profiles, logs, config, backup, and license state, shared by the CLI and the service |
| `PHANTOM_PIPE_NAME` | `\\.\pipe\PhantomService` | Named-pipe endpoint for service IPC |
| `PHANTOM_LOG_LEVEL` | `info` | `trace`, `debug`, `info`, `warn`, `error` |
| `PHANTOM_CONFIG` | `<data_dir>/config.json` | Alternate config-file location |
| `PHANTOM_TELEMETRY` | `false` | Opt-in telemetry |

`phantom config set` accepts `data_dir`, `pipe_name`, `log_level`, `license_key`, `telemetry_enabled`, `phone_home_url`, `phone_home_enabled`, and `phone_home_interval_secs`. The config file is plain JSON and safe to deploy through configuration management.

## Privacy and phone-home

Phantom does not phone home unless you set a callback URL. When you do, it sends a minimal, signed payload (an opaque license serial, the tier, the Phantom version, a timestamp, and tripwire counts) at most once per interval, over `curl` so the call is visible to your host tooling. No hardware fingerprint, profile content, or machine identity leaves the machine. Disable it any time with `phantom config set phone_home_enabled false`. See [docs/user/privacy.md](docs/user/privacy.md) and `phantom privacy-notice`.

## Uninstall

Uninstall through **Settings > Apps** or the MSI. Before the files are removed, Phantom reverts every applied identifier from its backup, so you are never left with a spoofed identity and no tool to restore it. Your profiles, license, and config remain under `%ProgramData%\Phantom` so a reinstall resumes where you left off; delete that folder to wipe everything. See [docs/user/uninstall.md](docs/user/uninstall.md).

## System requirements

- Windows 10 22H2 or Windows 11 23H2 or newer
- Administrator privileges for install, apply, and revert
- x86-64

## Scope

v1.0.0 ships the Layer-2 registry path, verified end-to-end (audit, generate, apply, validate, revert, reboot persistence, phone-home, and clean uninstall) on a fresh Windows VM. It is honest about its edges:

- **Layer 2 only.** Layers 1 (kernel driver) and 0 (UEFI/DXE firmware) are modeled but not shipped.
- **Unsigned.** The MSI is not yet code-signed; SmartScreen will warn until a signing certificate is in place.
- **Authorized use.** Intended for machines you own or are expressly authorized to test.

## License

Proprietary. See [LICENSE](LICENSE).
