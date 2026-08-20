# Security Policy

## Supported versions

Phantom is under active development. Security fixes are applied to the
latest release only; older releases are considered end-of-life once the
next tagged release ships.

## Reporting a vulnerability

Do **not** file a public GitHub issue for a security report. Send the
details to `security@phantom.dev` and include, when possible:

- The Phantom version and platform where the issue was observed
- A reproducible test case or a description precise enough to build one
- The impact you believe it has (loss of licensing enforcement, tamper
  detection bypass, privilege escalation on the host, etc.)
- Any suggested mitigation you already know of

You will get an acknowledgement within 3 business days and a plan for
fix + disclosure within 10 business days.

## What is in scope

- The license activation and validation path (`phantom-license` crate)
- The named-pipe protocol between the CLI, service, and tray
  (`phantom-ipc` crate) — including message parsing and pipe ACLs
- The Windows service surface (`phantom-svc` crate) — including how it
  reads the config file and how it persists state
- The kernel filter driver (`phantom-driver`) — anything that could
  crash a client machine or grant kernel-level primitives to userland
- The UEFI DXE module (`phantom-dxe`) — SMBIOS table rewrites and EFI
  variable handling

## What is out of scope

- Behavior on machines where Secure Boot has been intentionally
  disabled by the operator (Layer 0 requires this by design)
- Attacks that require pre-existing SYSTEM or Administrator privileges
  (Phantom trusts its own installer and service account)
- The developer key baked into pre-release binaries (production
  binaries ship with a rotated signing key; the pre-release key is
  documented as such)
