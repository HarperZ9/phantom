# Phantom

Hardware identity privacy for Windows.

Every application on your machine can read dozens of unique hardware identifiers: motherboard serials, disk serials, MAC addresses, GPU device IDs, TPM module IDs, and Windows registry GUIDs. Software uses these to fingerprint your device, track you across reinstalls, and build a permanent hardware dossier without your knowledge or consent.

Phantom lets you control what identity your machine presents. It generates realistic, vendor-accurate hardware profiles and applies them across multiple system layers, from the Windows registry up through kernel drivers and into UEFI firmware.

## Getting Started

### 1. Download

Download the latest MSI installer from [Releases](https://github.com/HarperZ9/phantom/releases).

Verify the download against the `SHA256SUMS.txt` file included in the release:

```bash
certutil -hashfile PhantomSetup-v1.0.0.msi SHA256
```

Compare the output to the corresponding line in `SHA256SUMS.txt`.

### 2. Install

Double-click the MSI. Accept the UAC prompt and the license agreement. Installation takes under 30 seconds.

The installer places three components into `C:\Program Files\Phantom\`:

| Component | Purpose |
|-----------|---------|
| `phantom.exe` | CLI for profile management, auditing, and configuration |
| `phantom-svc.exe` | Background service that orchestrates identity layers |
| `phantom-tray.exe` | System tray app with status indicator and profile switching |

The background service starts automatically. The tray app launches on login.

### 3. Verify installation

Open a terminal and run:

```bash
phantom --version
```

Expected output: `phantom 1.0.0` (or the version you installed).

Check that the service is running:

```bash
sc query PhantomService
```

The state should read `RUNNING`.

### 4. Audit your current exposure

Before changing anything, see what your machine currently reveals:

```bash
phantom audit
```

This prints every hardware identifier that software can read, organized by category (SMBIOS, disk, network, GPU, TPM, display, Windows registry). Nothing is modified.

### 5. Activate a license

Phantom works in Free tier immediately after installation. To unlock all layers and features, activate a license key:

```bash
phantom license activate <your-key>
```

You'll be prompted to accept the Terms of Use and Privacy Notice. Type `agree` at each prompt.

Check your license status at any time:

```bash
phantom license status
```

### 6. Generate and apply a profile

Create a named identity profile:

```bash
phantom profile generate my-profile
```

Apply it at Layer 2 (registry-level, available on all tiers):

```bash
phantom apply my-profile --layers 2
```

With a Pro or Enterprise license, apply across all layers:

```bash
phantom apply my-profile --layers 0,1,2
```

### 7. Validate

Confirm that every identifier source reports consistently:

```bash
phantom validate
```

All fields should show green. Any inconsistency between sources is flagged.

### 8. Revert

Restore your machine to its original hardware identity at any time:

```bash
phantom revert
```

Original values are backed up before every apply and restored exactly.

## License Tiers

| | Free | Pro | Enterprise |
|---|---|---|---|
| Layer 2 (registry) | Yes | Yes | Yes |
| Layer 1 (kernel driver) | | Yes | Yes |
| Layer 0 (UEFI firmware) | | Yes | Yes |
| Background service | | Yes | Yes |
| Saved profiles | 2 | 50 | Unlimited |
| Profile quick-switch (tray) | | Yes | Yes |
| Phone-home opt-out | | Yes | Yes |
| Machine-bound activation | | Yes | Yes |

**Free** works out of the box with no license key. It covers Windows registry identifiers (MachineGuid, HwProfileGuid, MachineId, ProductId, ComputerName, InstallDate) and gives you two saved profiles.

**Pro** unlocks all three identity layers, the background service for persistent protection across reboots, and up to 50 profiles.

**Enterprise** removes all limits. Volume licensing and centralized deployment via SCCM, GPO, or Ansible are supported through the `PHANTOM_DATA_DIR` environment variable and a managed `config.json`.

To purchase a license, contact [licensing@phantom.dev](mailto:licensing@phantom.dev).

## Identifier Coverage

Phantom covers 22+ identifier vectors across six hardware categories:

| Category | Identifiers | Layer |
|----------|------------|-------|
| SMBIOS | Board serial, BIOS UUID, system serial, chassis asset tag, manufacturer, product name | 0 |
| Disk | ATA serial, model string, firmware revision, storage query serial, volume serial, volume GUID | 1, 2 |
| Network | Permanent MAC, current MAC, adapter GUID (per-adapter) | 1, 2 |
| GPU | PCI vendor/device ID, PnP instance ID, driver key GUID | 1 |
| TPM | Module serial, manufacturer ID | 1 |
| Display | EDID serial, manufacturer code, product code | 1 |
| Windows | MachineGuid, HwProfileGuid, MachineId, ProductId, InstallDate, ComputerName | 2 |
| Boot | BCD identifier GUID, boot disk signature | 2 |

Generated identifiers are vendor-accurate. Samsung disk serials match Samsung's format. Intel MACs use real Intel OUI prefixes. A single seed produces a deterministic, internally consistent identity across every vector.

## CLI Reference

```bash
# Identity audit (read-only)
phantom audit

# Profile management
phantom profile generate <name>
phantom profile generate <name> --seed "memorable-string"
phantom profile show <name>
phantom profile list
phantom profile export <name> > profile.json
phantom profile import profile.json

# Apply and revert
phantom apply <name> --layers 2
phantom apply <name> --layers 0,1,2
phantom validate
phantom revert

# License
phantom license request
phantom license activate <key>
phantom license status

# Background service
phantom service ping
phantom service status
phantom service protect <name> --layers 1,2
phantom service unprotect

# Configuration
phantom config init
phantom config show
phantom config set <key> <value>
phantom config path

# Machine-readable output (stable JSON envelope)
phantom --json status
phantom --json audit
phantom --json license status
phantom --json profile list
```

## Configuration

Phantom resolves configuration in this order (highest wins):

1. Environment variables (`PHANTOM_*`)
2. JSON config file at `$PHANTOM_CONFIG` or `<data_dir>/config.json`
3. Compiled defaults

| Variable | Default | Description |
|----------|---------|-------------|
| `PHANTOM_DATA_DIR` | `%APPDATA%\phantom` | Base directory for profiles, logs, config, and license state |
| `PHANTOM_PIPE_NAME` | `\\.\pipe\PhantomService` | Named pipe endpoint for service IPC |
| `PHANTOM_LOG_LEVEL` | `info` | Log verbosity: `trace`, `debug`, `info`, `warn`, `error` |
| `PHANTOM_CONFIG` | `<data_dir>/config.json` | Alternate config file location |
| `PHANTOM_TELEMETRY` | `false` | Opt-in telemetry: `on`/`off`/`1`/`0` |

The config file is plain JSON and safe to deploy through configuration management systems. Enterprise deployments can drop a `config.json` into a managed `PHANTOM_DATA_DIR` for centralized rollout.

## Uninstall

Uninstall through Settings > Apps or via the MSI. The uninstaller reverts all identity layers (registry, kernel, firmware) before removing files, leaving your machine in its original state.

## System Requirements

- Windows 10 22H2 or Windows 11 23H2+
- Administrator privileges (for service installation and registry/driver operations)
- Layer 0 requires UEFI with Secure Boot disabled

## License

Proprietary. See [LICENSE](LICENSE) for terms.
