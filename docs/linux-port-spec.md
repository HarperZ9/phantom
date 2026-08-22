# Phantom for Linux: port specification

Status: Phases 1 and 2 shipped; Phase 3 packaging shipped, its CI job and VM
power-cycle dogfood pending. Target: a Linux userland identity layer that matches
Phantom's Windows Layer 2 in function and in its reversibility guarantee. macOS is
explicitly out of scope (see [Non-goals](#non-goals)).

## 1. Why this is a clean port, not a rewrite

Phantom already splits its apply layer by platform: `apply/registry.rs`,
`apply/firmware.rs`, and `apply/driver_ipc.rs` are each `#[cfg(windows)]` with a
`#[cfg(not(windows))]` stub that returns "requires Windows." The profile schema
(`profile/schema.rs`) is a platform-neutral superset that already models every
identifier a Linux port needs:

| Profile field | Linux meaning |
|---|---|
| `os.machine_id` | `/etc/machine-id` (systemd machine ID) |
| `os.computer_name` | hostname |
| `network_adapters[].current_mac` | the live MAC of each adapter |
| `network_adapters[].permanent_mac` | the hardware (permanent) MAC, read-only |
| `smbios.*`, `disk.*` | DMI/SMBIOS and disk serials (deferred tier) |

So the port reuses profile **generation**, the **CLI**, the **license** layer,
and the **backup discipline** unchanged. What is new is one apply **backend** and
the Linux **audit/validate** sources. The seed-to-identity engine does not move.

## 2. The Linux Layer-2 apply set

Layer 2 on Linux spoofs the userland-reversible identifiers, the ones an ordinary
process reads and that Phantom can change and restore exactly without touching
firmware:

| Identifier | Location | Backend | Persists a reboot on its own? |
|---|---|---|---|
| machine-id | `/etc/machine-id` and `/var/lib/dbus/machine-id` | file write | yes |
| hostname | `/etc/hostname` and the live `sethostname()` | file + syscall | yes |
| MAC (per adapter) | `ip link set dev <if> address <mac>` | netlink/`ip` | **no** |

`machine-id` is the direct analog of Windows `MachineGuid`: a stable per-install
identifier many libraries read for fingerprinting. hostname is the analog of
`ComputerName`, and on Linux it is safe to change at Layer 2 (unlike Windows,
where a registry-only rename desyncs WMI, which is why `ComputerName` is deferred
there). MACs are modeled on both platforms already.

Everything else the profile models (DMI/SMBIOS board and system serials, disk
serials, TPM, GPU) is **read-only in userland** on Linux: `/sys/class/dmi/id/*`
is populated by the kernel from firmware and cannot be written from userland.
Those stay in the deferred firmware/kernel tier, exactly as on Windows.

## 3. The reversibility guarantee holds identically

The core promise is unchanged: back up every original before the first write,
and restore it exactly on revert or uninstall. The backup lives machine-wide at
`/var/lib/phantom/backup.json` (root-owned, the analog of
`%ProgramData%\Phantom`), reusing the same `RegistryBackup` shape with a
Linux-flavored `value_type` tag (`file`, `hostname`, `mac`) instead of
`sz`/`dword`. The write-ahead ordering and the "preserve the true original across
a re-apply" rule from the recent backup-integrity fix apply directly, and matter
more here because of the MAC case below.

## 4. The MAC case makes the service load-bearing

On Windows, a Layer-2 registry value persists across reboot on its own, so the
service re-applying it is redundant. On Linux, machine-id and hostname persist
the same way, but **a spoofed MAC does not**: the NIC resets to its hardware MAC
on boot. So the systemd service's reapply-on-boot is genuinely needed to keep a
spoofed MAC across reboots.

That reapply reads the current (spoofed, or reset-to-hardware) MAC on boot. The
recently shipped `merge_preserving_originals` fix is what makes this safe: the
service reapplies without overwriting the true original MAC captured on the first
apply, so revert and uninstall still restore the hardware MAC. The Linux port
depends on that fix; it would be unsafe without it.

## 5. Module and architecture

- `apply/identity_linux.rs`, gated `#[cfg(target_os = "linux")]`, implementing the
  same `apply_registry_layer` / `revert_registry_layer` contract the Windows
  backend exposes. The public apply API (`apply::apply_profile`, `apply::status`,
  `apply::revert_all`) does not change; it dispatches to the platform backend by
  `cfg`, the way it already dispatches the firmware and driver stubs.
- Rename the layer's user-facing label from "Registry/Userland" to "Userland
  identity" so Layer 2 reads correctly on both platforms. Internal only; no CLI
  surface changes.
- Linux audit/validate sources under `validator/sources_linux.rs`: read
  `/etc/machine-id`, `hostname`, `/sys/class/net/<if>/address` (live MAC) and
  the permanent MAC via `ethtool -P` or `/sys/class/net/<if>/addr_assign_type`,
  and `/sys/class/dmi/id/*` and `lsblk`/`hdparm` for the deferred identifiers,
  so `audit` and `validate` report the full picture even where apply cannot yet
  reach.
- Service: a `phantom.service` systemd unit replacing the Windows service. Its
  load-bearing job is reapply-on-boot: a `Type=oneshot` unit that runs
  `phantom-svc --reapply` before networking configures the interfaces, so a
  spoofed MAC is restored after a reboot. The live IPC socket is deferred: on
  Linux the CLI applies and reverts directly as root, so a running daemon is a
  convenience, not a correctness requirement. When it lands, the named pipe
  becomes a Unix domain socket at `/run/phantom.sock`; `phantom-ipc` already
  abstracts the transport behind a signed message protocol, so the wire format
  is unchanged.

## 6. Permissions

Writing machine-id and hostname needs write access to `/etc`; setting a MAC needs
`CAP_NET_ADMIN`. In practice this means root, the analog of the Windows
elevation requirement. `apply`, `revert`, and the service run as root; `audit`,
`generate`, and read-only commands do not.

## 7. Packaging and distribution

- Build target `x86_64-unknown-linux-musl` for a static, zero-dependency binary,
  matching the "one file to run" story.
- Ship a `.deb` and `.rpm` plus a plain tarball with an install script; the
  systemd unit installs to `/etc/systemd/system/`.
- No code-signing or SmartScreen equivalent applies on Linux. The trust story is
  the same honest one Phantom already ships: a published `SHA256SUMS.txt`, and
  optionally a GPG-signed package for distro repositories later.

## 8. Testing and dogfood

- Unit tests for the pure logic (profile mapping, backup merge, value-type
  round-trip) run cross-platform in the existing CI, which already builds and
  tests on `ubuntu-latest`.
- Add a Linux job that exercises the real apply path in a rootful container or
  VM (the Windows CI cannot).
- A Linux VM dogfood mirroring the Windows 12-section runbook: audit, generate,
  apply, validate, reboot persistence (machine-id/hostname by file, MAC by
  service), revert restores exactly, and package removal returns machine-id,
  hostname, and MAC to their originals. Same Sev-1 bar: uninstall must leave the
  machine on its true identity.

## 9. Phasing

1. **machine-id + hostname + MAC. Shipped.** File-based identifiers first
   (persistent, highest fingerprinting value), which proved the Linux backend and
   the backup path end to end, then the MAC. `apply`, `revert`, `audit`, and
   `validate` all read Linux.
2. **The systemd service. Shipped.** The reapply-on-boot path so a spoofed MAC
   survives a reboot, leaning on the backup-preservation fix. `phantom apply`
   records the active profile and `phantom revert` clears it, giving the boot
   reapply its source of truth. The live IPC socket is deferred (see section 5).
3. **Packaging + Linux dogfood + CI job.** Packaging shipped: a `.deb`, an
   `.rpm`, and a portable tarball with an install script, built by
   `packaging/linux/build-packages.sh` and attached to the release. The `.deb`
   install and removal cycle is dogfooded on a systemd host, including the
   revert-on-remove Sev-1 bar. Still owed: a rootful-VM CI job that exercises the
   real apply path, and a power-cycle dogfood confirming the MAC returns after a
   real reboot.

Each phase is independently useful and independently testable.

## 10. Non-goals

- **macOS is parked.** The identifiers software fingerprints on a Mac
  (`IOPlatformUUID`, the serial number) come from IOKit/EFI and are effectively
  read-only; changing them requires disabling SIP plus a kext, which is
  deprecated and fragile on Apple Silicon. Only MAC and hostname are userland
  spoofable, which is too thin to carry the product's promise. Revisit only if a
  concrete, SIP-respecting path appears.
- **The firmware and kernel tier stays deferred** on Linux as on Windows. DMI,
  disk, TPM, and GPU identifiers need a kernel module or firmware changes (Layer
  0/1) and are modeled, not applied.
- **Authorized use only.** Same boundary as the Windows product: machines you
  own or are expressly authorized to test, not for evading fraud controls.
