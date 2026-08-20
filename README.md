# Phantom

Hardware identity privacy tool.

Phantom generates realistic, internally-consistent hardware identity profiles and applies them across multiple system layers — giving users control over what their machine reports to software.

## Why

Every application on your machine can silently read dozens of unique hardware identifiers: motherboard serials, disk serials, MAC addresses, GPU device IDs, TPM module IDs, Windows registry GUIDs, and more. These identifiers enable:

- **Cross-application device fingerprinting** — correlating your activity across unrelated software
- **Persistent tracking across reinstalls** — hardware IDs survive OS reinstallation
- **Hardware-level profiling** — building a permanent device dossier without your knowledge

Phantom lets you decide what identity your hardware presents.

## Features

- **Consistent profile generation** — a single seed produces a deterministic, internally-consistent identity across all 22+ identifier vectors. Same seed, same identity, every time.
- **Vendor-accurate identifiers** — generated serials, MACs, and device IDs match real manufacturer formats (Samsung disk serials look like Samsung serials, Intel MACs use real Intel OUI prefixes).
- **Cross-source validation** — queries every identifier source the same way fingerprinting software does, reports any inconsistencies before you go live.
- **Hardware audit** — read-only mode that reports exactly what your machine currently exposes, without changing anything.
- **Profile management** — save, load, export, import, and share named profiles as portable JSON.
- **Layered architecture** — apply spoofing at the level you need:
  - **Layer 0** — UEFI/DXE firmware (SMBIOS in physical memory)
  - **Layer 1** — Kernel driver (disk, NIC, GPU, TPM interception)
  - **Layer 2** — Registry and userland (MachineGuid, HwProfileGuid, ComputerName, etc.)
- **Backup and revert** — original values are backed up before modification and can be restored with one command.

## Quick Start

```bash
# See what your machine currently exposes
phantom audit

# Generate a privacy profile
phantom profile generate my-profile --seed "any-memorable-string"

# Inspect it
phantom profile show my-profile

# Apply registry-level spoofing
phantom apply my-profile --layers 2

# Verify consistency
phantom validate my-profile

# Restore originals
phantom revert
```

## Identifier Coverage

| Category | Identifiers | Layer |
|----------|------------|-------|
| SMBIOS | Board serial, BIOS UUID, system serial, chassis asset tag, manufacturer, product name | 0 |
| Disk | ATA serial, model string, firmware revision | 1 |
| Storage | Storage query serial, volume serial, volume GUID | 1+2 |
| Network | Permanent MAC, current MAC, adapter GUID (per-adapter) | 1+2 |
| GPU | PCI vendor/device ID, PnP instance ID, driver key GUID | 1 |
| TPM | Module serial, manufacturer ID | 1 |
| Display | EDID serial, manufacturer code, product code | 1 |
| Windows | MachineGuid, HwProfileGuid, MachineId, ProductId, InstallDate, ComputerName | 2 |
| Boot | BCD identifier GUID, boot disk signature | 2 |

## Architecture

```
phantom-cli        Rust CLI + library — profile management, validation, reporting
phantom-ipc        Rust shared crate — named-pipe IPC protocol (message types + wire format)
phantom-svc        Rust Windows service — background orchestrator, listens on named pipe
phantom-tray       Rust system tray app — shield icon, status popup, toast notifications
phantom-installer  WiX MSI installer — service, tray, driver, auto-start, clean uninstall
phantom-driver     C kernel filter driver — Layer 1 interception
phantom-dxe        C UEFI DXE application — Layer 0 firmware patching
```

The CLI, IPC protocol, and profile engine run on any platform. The service, Layer 1, and Layer 2 apply operations are Windows-specific. Layer 0 requires UEFI with Secure Boot disabled.

### Service Architecture

```
                    ┌──────────────┐
                    │  phantom-svc │  ← Windows service (always-on)
                    │  NamedPipe   │
                    └──────┬───────┘
                           │ IPC (length-prefixed JSON)
              ┌────────────┼────────────┐
              │            │            │
     ┌────────┴───┐  ┌─────┴──────┐  ┌──┴──────────┐
     │ phantom-cli│  │phantom-tray│  │ other client │
     │ `service`  │  │ shield UI  │  │              │
     └────────────┘  └────────────┘  └──────────────┘
```

The service runs as a background process (standalone or as a Windows service), listens on `\\.\pipe\PhantomService`, and orchestrates apply/revert across all layers. Clients communicate via the phantom-ipc protocol — a 4-byte LE length prefix followed by JSON request/response messages.

## Building

### CLI

```bash
cd phantom
cargo build --release
```

This builds both `phantom-cli` and `phantom-svc`. Binaries are at `target/release/phantom-cli` and `target/release/phantom-svc` (`.exe` on Windows).

### Kernel Driver (Layer 1)

Requires the Windows Driver Kit (WDK) and Visual Studio with the WDK build tools:

```bash
cd phantom/phantom-driver
msbuild phantom.vcxproj /p:Configuration=Release /p:Platform=x64
```

The driver binary is `phantom.sys`. Install via `phantom.inf` (test signing required for unsigned drivers).

### DXE Firmware Module (Layer 0)

Requires the [EDK2](https://github.com/tianocore/edk2) build environment:

```bash
# From the edk2 root, symlink or copy the phantom-dxe directory
ln -s /path/to/phantom/phantom-dxe PhantomDxe
source edksetup.sh
build -p PhantomDxe/PhantomDxe.dsc -a X64 -t GCC5
```

The output is `PhantomDxe.efi`. Copy it to the EFI System Partition and configure your firmware to load it as a DXE driver. Requires Secure Boot disabled.

**Usage flow:**
1. From Windows, run `phantom apply <profile> --layers 0` to write the SMBIOS profile to an EFI NVRAM variable
2. Reboot — the DXE module reads the variable and rewrites SMBIOS tables in firmware memory
3. Windows boots with spoofed SMBIOS values visible to all software, including physical memory readers

### Installer

Requires [WiX Toolset v3](https://wixtoolset.org/) and all binaries built:

```bash
cd phantom/phantom-installer
build.cmd
```

The output is `out\PhantomSetup.msi`. The installer:

- Copies `phantom-cli.exe`, `phantom-svc.exe`, and `phantom-tray.exe` to `%ProgramFiles%\Phantom`
- Installs `PhantomService` as an auto-start Windows service
- Registers `phantom-tray.exe` for login auto-start
- Installs the kernel driver via `pnputil`
- Creates a Start Menu shortcut

On first launch the service auto-generates a `default` identity profile and applies it, so the user is protected before they interact with the tray app.

**Uninstall** reverts all identity layers (registry, kernel, firmware) before removing files, leaving the machine in its original state.

## Profile Format

Profiles are stored as JSON in `%APPDATA%\phantom\profiles\` (Windows) or `~/.config/phantom/profiles/` (Linux/macOS). See `profiles/` for examples.

### Service

```bash
# Run in standalone mode (foreground, for development)
phantom-svc --standalone

# Install as a Windows service
phantom-svc --install
sc start PhantomService

# Communicate via the CLI
phantom service ping
phantom service status
phantom service protect my-profile --layers 1,2
phantom service unprotect

# Uninstall the service
phantom-svc --uninstall
```

## Configuration

Phantom supports enterprise configuration via environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `PHANTOM_DATA_DIR` | `%APPDATA%\phantom` (Win) / `~/.config/phantom` (Linux) | Base data directory for profiles, logs, config, and license state |
| `PHANTOM_PIPE_NAME` | `\\.\pipe\PhantomService` | Named pipe endpoint for IPC |
| `PHANTOM_LOG_LEVEL` | `info` | Log verbosity (`trace`, `debug`, `info`, `warn`, `error`). Takes priority over `RUST_LOG` |

Enterprise deployments can set `PHANTOM_DATA_DIR` to a shared or managed path (e.g. `C:\ProgramData\Phantom`) for centralized profile and log storage.

## Project Status

- [x] Profile engine with vendor-accurate generation
- [x] Profile management (save/load/export/import)
- [x] Hardware audit (read-only identity report)
- [x] Cross-source validation (all profile fields covered)
- [x] Layer 2 registry spoofing (MachineGuid, HwProfileGuid, MachineId, ProductId, ComputerName, InstallDate)
- [x] Layer 1 kernel driver (disk/NIC/GPU/TPM/EDID filter, timing normalization + calibration)
- [x] Layer 0 DXE firmware module (SMBIOS rewrite, EFI variable profile store)
- [x] WDK build system (phantom.vcxproj)
- [x] Named-pipe IPC protocol (phantom-ipc: message types, wire format, client/server)
- [x] Background service (phantom-svc: Windows service + standalone mode, request handler)
- [x] CLI service commands (phantom service ping/status/protect/unprotect)
- [x] System tray app (phantom-tray: shield icon, status popup, toast, context menu, auto-start)
- [x] WiX MSI installer (phantom-installer: service + tray + driver install, clean uninstall)
- [x] First-run auto-generation (service creates default profile on first boot)
- [x] Config persistence (service remembers active profile across reboots)
- [x] Profile quick-switch (tray context menu with checkmark + toast confirmation)
- [x] System identifier readers (SMBIOS parser, disk/network/GPU/display/TPM registry readers)
- [x] Source key alignment (all reader keys match validation diff map)
- [x] Service quality (real identifier_count, revert warning propagation)
- [x] Security hardening (SDDL pipe ACL, CSPRNG seed generation, release profile with LTO)
- [x] Structured logging (tracing + daily file rotation, PHANTOM_LOG_LEVEL)
- [x] License system (HMAC-SHA256 keys, machine-bound activation, tier-gated layer access)
- [x] Anti-tamper (debugger detection, binary integrity checks, constant-time comparison)
- [x] Enterprise config (PHANTOM_DATA_DIR, PHANTOM_PIPE_NAME, centralized deployment)

## License

MIT
