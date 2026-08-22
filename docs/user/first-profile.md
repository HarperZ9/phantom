# Your first profile

A **profile** is a saved, named identity Phantom applies to your
machine. Generate one, apply it, verify, and (when you want your real
identity back) revert. That is the whole loop.

## Prerequisites

- Phantom installed (see [install.md](install.md)). Free tier is enough
  to apply a Layer-2 profile.
- An Administrator terminal on the machine.

## 1. Audit what you have now

Before changing anything, snapshot your current identity so you know
what "real" looks like:

```
phantom audit
```

Output includes your `MachineGuid`, your primary NIC MACs, your SMBIOS
strings, and a summary fingerprint. Copy the fingerprint somewhere safe;
it is your recovery reference if a revert ever misfires.

## 2. Generate a profile

```
phantom profile generate my-profile
```

This creates a new, random, internally consistent identity (a fresh
MachineGuid, a plausible OEM brand string, believable NIC vendor
prefixes) and saves it under
`%ProgramData%\Phantom\profiles\my-profile.json`.

`generate` does **not** apply the profile yet. You can list what you
have, inspect it, or generate several and pick one:

```
phantom profile list
phantom profile show my-profile
```

## 3. Apply it

```
phantom apply my-profile --layers 2
```

Layer 2 is the layer that ships in v1.0.0. It rewrites the five Windows
registry identifiers most software reads to fingerprint a machine:
MachineGuid, HwProfileGuid, MachineId, ProductId, and InstallDate. It
does **not** touch real hardware, and nothing on your disk is at risk.
The rest of the profile (SMBIOS, disk, network, and so on) is modeled
for the deferred layers, not written yet.

The command runs synchronously and reports what changed:

```
Applying profile 'my-profile' to 1 layer(s)...

Layer 2 (Registry/Userland) - 5 identifiers applied
  + SOFTWARE\Microsoft\Cryptography\MachineGuid
  + SOFTWARE\Microsoft\SQMClient\MachineId
  [3 more]

Backup written to %ProgramData%\Phantom\backup.json
```

Run it from an elevated terminal: Layer 2 writes to `HKLM`, which
requires administrator. If you are not elevated, `apply` tells you so.

## 4. Verify

```
phantom validate my-profile
```

`validate` re-reads every applied identifier and confirms the value on
disk matches the profile. The Layer-2 keys read consistent; identifiers
from the deferred layers show as not-yet-applied, which is expected.

Or run `phantom audit` again and compare against your step-1 snapshot;
the registry identifiers should have moved.

## 5. Reboot (recommended)

Some Windows services cache identifiers at startup. A reboot after
`phantom apply` ensures every application sees the new identity. On a
Pro or Enterprise license, Phantom's service restarts at boot and
re-asserts the profile.

## 6. Revert when you want your real identity back

```
phantom revert
```

Phantom reads the backup written during apply and restores every value
exactly. Once every value is restored, it clears the backup, so `phantom
status` reads original state again.

## Common patterns

**Switch between two lab profiles.** Generate two profiles, then
`phantom apply lab-a --layers 2` or `lab-b`. Each apply reverts the
previous profile from backup first, then applies the new one, so the two
never corrupt each other.

**Regenerate a profile.** `phantom profile generate rolling` overwrites
the same slot with a new random identity. Validate the apply and revert
on a disposable test image before wiring it into any scheduled workflow.

**Inspect a profile.** `phantom --json profile show my-profile` dumps
the whole record; pipe to `jq` to grep by field.

## What you cannot do yet

- **Layer 1**, a kernel filter driver that intercepts identifier reads
  at the syscall boundary, blocks applications that read raw device
  serials Phantom cannot mask from user mode. Deferred; not installed by
  the current MSI.
- **Layer 0**, UEFI/DXE hooks for pre-boot identity. Deferred; not
  included in the current MSI.

## Troubleshooting

Nothing changed after `apply`. Was the terminal elevated? Layer 2 writes
to `HKLM`, which requires administrator.

`revert` says "no backup". No profile is currently applied. Run `phantom
status` to see what Phantom thinks is active.

A specific application still sees your real identity. It probably reads a
Layer-1 identifier (raw SATA serial, physical NIC EEPROM) that Layer 2
cannot mask. File the application name and the identifier it reads in an
issue; Layer 1 is on the roadmap.

## Next steps

- Read `phantom --help` for the full command surface.
- Run `phantom privacy-notice` any time to reread what phones home.
- [Uninstall](uninstall.md) reverts every applied identifier before
  removing the files.
