# Phantom

Prerelease Windows Layer-2 registry identity tooling for authorized lab evaluation.

Every application on your machine can read dozens of unique hardware identifiers: motherboard serials, disk serials, MAC addresses, GPU device IDs, TPM module IDs, and Windows registry GUIDs. Software uses these to fingerprint your device, track you across reinstalls, and build a permanent hardware dossier without your knowledge or consent.

Phantom v1.0.0-rc1 audits identifier exposure, generates internally consistent profiles, and can apply the supported registry fields in Layer 2 on organization-owned or expressly authorized Windows test systems. The current MSI does not install a kernel driver or UEFI/DXE component. Layer 1 and Layer 0 remain deferred roadmap work.

The current release is not cleared for production or for an external enterprise pilot. CI, clean-VM lifecycle receipts, rollback and uninstall verification, operational support controls, and the service IPC privilege boundary must pass before that status changes.

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

Phantom works in Free tier immediately after installation. A license key can unlock the rc1 Layer-2 feature and profile limits represented by the selected tier; it does not enable Layer 1 or Layer 0:

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

Layer 1 kernel-driver and Layer 0 UEFI/DXE modes are not included in the current MSI and must not be represented as available.

### 7. Validate

Confirm that every identifier source reports consistently:

```bash
phantom validate my-profile
```

All fields should show green. Any inconsistency between sources is flagged.

### 8. Revert

Restore your machine to its original hardware identity at any time:

```bash
phantom revert
```

Original values are backed up before every apply and restored exactly.

## Current rc1 scope

| Surface | rc1 status |
|---|---|
| Layer 2 registry mode | Prerelease lab evaluation only |
| Layer 1 kernel driver | Deferred; not installed by the current MSI |
| Layer 0 UEFI/DXE | Deferred; not included in the current MSI |
| Enterprise deployment | Not cleared; CI, lifecycle, support, and IPC privilege-boundary gates remain |

The codebase contains Free, Pro, and Enterprise license tiers for Layer-2 features and profile limits. Their presence does not make the rc1 build production-ready or authorize claims that Layer 1 or Layer 0 is available.

Do not use the current prerelease as a production privacy control. Treat it as lab tooling until the published readiness gates have current pass receipts.

## Identifier model and current apply scope

Phantom profiles model identifier vectors across several hardware categories. In rc1, only the Layer-2 registry apply path is in the supported lab scope. Layer-1 and Layer-0 rows below describe deferred model coverage, not current apply capability.

| Category | Identifiers | Layer | rc1 apply status |
|----------|------------|-------|---|
| SMBIOS | Board serial, BIOS UUID, system serial, chassis asset tag, manufacturer, product name | 0 | Deferred |
| Disk | ATA serial, model string, firmware revision, storage query serial, volume serial, volume GUID | 1, 2 | Layer-2 fields only |
| Network | Permanent MAC, current MAC, adapter GUID (per-adapter) | 1, 2 | Layer-2 fields only |
| GPU | PCI vendor/device ID, PnP instance ID, driver key GUID | 1 | Deferred |
| TPM | Module serial, manufacturer ID | 1 | Deferred |
| Display | EDID serial, manufacturer code, product code | 1 | Deferred |
| Windows | MachineGuid, HwProfileGuid, MachineId, ProductId, InstallDate, ComputerName | 2 | Prerelease lab scope |
| Boot | BCD identifier GUID, boot disk signature | 2 | Prerelease lab scope; verify per runbook |

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
phantom validate <name>
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

Uninstall through Settings > Apps or via the MSI. For rc1, validate Layer-2 backup, revert, and uninstall behavior on a disposable Windows test image before relying on it. The current MSI has no kernel-driver or firmware layer to remove.

## System Requirements

- Windows 10 22H2 or Windows 11 23H2+
- Administrator privileges for service installation and registry operations

## License

Proprietary. See [LICENSE](LICENSE) for terms.
