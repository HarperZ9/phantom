# Your first profile

A **profile** is a saved, named identity Phantom applies to your
machine. Generate one, apply it, verify, and (when you want your
real identity back) revert. That's the whole loop.

## Prerequisites

- Phantom installed and [activated](activate.md) at Pro or higher.
- Administrator terminal on the machine.

## 1. Audit what you have now

Before changing anything, take a snapshot of your current identity
so you know what "real" looks like:

```
phantom audit
```

Output includes your `MachineGuid`, your primary NIC MACs, your
SMBIOS strings, and a summary fingerprint. Copy the fingerprint
somewhere safe — it is your recovery reference if a revert ever
misfires.

## 2. Generate a profile

```
phantom profile generate my-profile
```

This creates a new, random identity — a fresh MachineGuid, a
plausible OEM brand string, believable NIC vendor prefixes, etc. —
and saves it under `%ProgramData%\Phantom\profiles\my-profile.json`.

`generate` does **not** apply the profile yet. You can list what
you have, inspect it, or generate several and pick the one you
like:

```
phantom profile list
phantom profile show my-profile
```

## 3. Apply it

```
phantom apply my-profile --layers 2
```

`--layers 2` is the only layer that ships in v1.0 — user-mode
registry spoofing. It changes what Windows tells any process that
asks for `MachineGuid`, DHCP client id, SMBIOS, and related
identifiers. It does **not** modify actual hardware; nothing on your
disk is at risk.

The command runs synchronously and reports what changed:

```
Applied 'my-profile' at layer 2 (registry).
  HKLM\SOFTWARE\Microsoft\Cryptography\MachineGuid ← <new-guid>
  HKLM\SOFTWARE\Microsoft\SQMClient\MachineId ← <new-id>
  [3 more]

Backup written to %ProgramData%\Phantom\backup.json
Reboot recommended for changes to fully propagate.
```

## 4. Verify

```
phantom validate
```

`validate` re-reads every key Phantom claimed to change and confirms
the value on disk matches the profile. Green means every change is
in place.

Or run `phantom audit` again and compare against your snapshot
from step 1 — the identifiers should have moved.

## 5. Reboot (recommended)

Some Windows services cache identifiers at startup. A reboot after
`phantom apply` ensures every application sees the new identity.
Phantom's service restarts automatically at boot and re-asserts the
profile if anything drifts.

## 6. Revert when you want your real identity back

```
phantom revert
```

Phantom reads the backup file it wrote during apply and restores
every value byte-for-byte. Reboot again for good measure.

## Common patterns

**Switch between two lab profiles.** Generate two profiles, then
`phantom apply lab-a --layers 2` or `lab-b`. Each apply
first reverts the previous profile from backup, then applies the new
one — the two profiles don't corrupt each other.

**Regenerate a lab profile.** `phantom profile generate rolling`
overwrites the same slot with a new random identity. Validate backup,
apply, and revert manually on a disposable test image before adding
any scheduled workflow.

**Inspect what an existing profile actually contains.**
`phantom profile show my-profile --json` dumps the whole record;
pipe to `jq` if you want to grep by field.

## What you cannot do (yet)

- **Layer 1** — kernel filter driver that intercepts identifier
  reads at the syscall boundary. Blocks applications that read raw
  device serials Phantom cannot mask from user mode. Deferred; it is
  not installed by the current MSI.
- **Layer 0** — UEFI/DXE hooks for pre-boot identity. Niche;
  deferred and not included in the current MSI.

No externally representative application-coverage percentage has
been established for rc1.

## Troubleshooting

Nothing changed after `apply`. → Was the terminal elevated? Layer 2
writes to `HKLM`, which requires administrator.

`revert` says "no backup". → No profile is currently applied. Run
`phantom status` to see what Phantom thinks is active.

A specific application still sees my real identity. → It probably
reads a Layer 1 identifier (raw SATA serial, physical NIC EEPROM).
File the app name in an issue with the identifier it reads; Layer 1
is on the roadmap.

## Next steps

- Read `phantom --help` for the full command surface.
- Run `phantom privacy-notice` any time to reread what phones home.
- [Uninstall](uninstall.md) describes the intended Layer-2 cleanup
  path. Verify it on a disposable Windows test image before relying
  on it.
