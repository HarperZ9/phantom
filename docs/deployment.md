# Phantom Deployment Guide

## Single-Machine Install

1. Build release binaries:
   ```bash
   cd phantom
   cargo build --release
   ```
2. Copy `target/release/phantom-cli.exe` and `target/release/phantom-svc.exe` to a directory on `PATH`.
3. Install the service:
   ```bash
   phantom-svc --install
   sc start PhantomService
   ```
4. Generate a profile and apply:
   ```bash
   phantom profile generate default --seed "my-seed"
   phantom apply default --layers 2
   ```

Alternatively, use the MSI installer which handles all of the above.

## Enterprise Deployment

### Centralized Data Directory

Set `PHANTOM_DATA_DIR` as a system-wide environment variable to control where Phantom stores profiles, logs, license state, and config:

```
PHANTOM_DATA_DIR=C:\ProgramData\Phantom
```

Directory structure under `PHANTOM_DATA_DIR`:

```
<PHANTOM_DATA_DIR>/
  profiles/       saved identity profiles (.json)
  logs/           daily-rotated service logs (phantom-svc.log.YYYY-MM-DD)
  .config.json    service state (active profile, auto-apply flag)
  license.json    license activation state
```

### Custom Pipe Name

For environments running multiple Phantom instances or where the default pipe name conflicts:

```
PHANTOM_PIPE_NAME=\\.\pipe\PhantomService-TeamA
```

Both the service and CLI read this variable, so set it machine-wide.

### Log Level

Control service log verbosity:

```
PHANTOM_LOG_LEVEL=debug
```

Accepted values: `trace`, `debug`, `info` (default), `warn`, `error`. This takes priority over `RUST_LOG`.

### Group Policy / MDM

For managed deployments, set the three environment variables via Group Policy (Computer Configuration > Preferences > Environment) or your MDM tool. The MSI installer accepts `PHANTOM_DATA_DIR` as a property:

```bash
msiexec /i PhantomSetup.msi PHANTOM_DATA_DIR="C:\ProgramData\Phantom" /qn
```

### Licensing

Phantom uses a tiered license model:

| Tier | Layers | Max Profiles | Binding |
|------|--------|-------------|---------|
| Free | Layer 2 only | 2 | None |
| Pro | All (0, 1, 2) | 50 | Machine |
| Enterprise | All (0, 1, 2) | Unlimited | Machine |

Activate a license:
```bash
phantom license activate <KEY>
```

Check status:
```bash
phantom license status
```

License keys are HMAC-SHA256 signed and bound to the machine's hardware fingerprint. View the fingerprint with:
```bash
phantom license fingerprint
```

### Silent Deployment Checklist

1. Set `PHANTOM_DATA_DIR` via GPO/MDM (optional — defaults to the
   machine-wide `%ProgramData%\Phantom`)
2. Deploy MSI with `/qn` (silent)
3. Distribute license keys to each machine (pre-activated via `phantom license activate`)
4. Pre-stage a profile JSON in `<PHANTOM_DATA_DIR>/profiles/<name>.json`
5. Apply it explicitly from an elevated context:
   `phantom apply <name> --layers 2`

The service never spoofs on its own. It applies only what an operator
explicitly applies, and once applied it re-applies that profile across
reboots. A fresh install with no prior apply leaves the machine
unprotected.

## Uninstall

```bash
phantom-svc --cleanup    # reverts layers, removes tray auto-start, clears config
phantom-svc --uninstall  # removes the Windows service
```

Or uninstall via the MSI, which runs cleanup automatically.

## Troubleshooting

**Service won't start**: Check logs in `<PHANTOM_DATA_DIR>/logs/`. Common causes: pipe name conflict, insufficient permissions.

**License activation fails**: Ensure the key is valid and matches this machine's fingerprint (`phantom license fingerprint`). Keys are one machine, one activation.

**Pipe connection refused**: Verify the service is running (`sc query PhantomService`). If using a custom pipe name, ensure both service and CLI have the same `PHANTOM_PIPE_NAME`.
