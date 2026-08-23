# phantom-driver (Layer 1) security review

Review of the kernel filter driver (`phantom-driver/src`, ~1811 lines of C):
`driver.c`, `control_ipc.c`, `profile_store.c`, and the disk / NIC / GPU / TPM /
EDID filters.

Update: the driver now compiles and links in CI with the WDK (the `driver build
(WDK)` job, Level4 warnings-as-errors). That build already caught real defects
that only a compiler finds (a broken project file, functions used before
declaration, a missing include, dead code), all fixed. So the fixes below are no
longer source-only reasoning; they compile. What CI still cannot do is run the
driver: kernel bugs bugcheck the machine, so every change here MUST still be
validated under a kernel debugger (Driver Verifier on, a BSOD-tolerant VM) before
it is trusted. That runtime validation is deferred with the rest of Layer 1.

## Fixed in this change

Low-risk, self-contained boundary fixes applied here. Each closes a concrete
memory-safety bug and adds no new kernel API or concurrency.

- **Unvalidated per-field lengths (Sev-2, root of a Sev-1).** `PhantomProfileStoreSet`
  validated the array counts but not the per-field lengths (`SerialLength`,
  `ModelLength`, `FirmwareRevLength`, `PnpInstanceIdLength`), which arrive from
  userland. An out-of-range length let a filter read past a fixed source array,
  and in the GPU path drove an allocation-size integer overflow (see below).
  Now every length is validated against its array size at the IOCTL boundary; a
  malformed profile is rejected with `STATUS_INVALID_PARAMETER`.
  - This one fix also neutralizes the **GPU heap overflow (Sev-1)** in
    `GpuPnpQueryIdCompletion`: it sizes an allocation as
    `(PnpInstanceIdLength + 1) * sizeof(WCHAR)`. With `PnpInstanceIdLength`
    unvalidated, a large value overflowed the size to a small allocation, then
    the copy loop wrote `PnpInstanceIdLength` wide chars past it. Bounding the
    length at the boundary removes the overflow.
- **ATA `DataBufferOffset` integer overflow (Sev-2).** `RewriteAtaIdentify`
  checked `DataBufferOffset + 512 > bufferLen`, which wraps for an offset near
  `ULONG_MAX` and would then point `identWords` at a wild address for an OOB
  write. Replaced with the overflow-safe `DataBufferOffset > bufferLen - 512`
  (the prior guard already forces `bufferLen >= sizeof(ATA_PASS_THROUGH_EX) + 512`,
  so the subtraction cannot underflow).
- **NIC OID read without a length check (Sev-2).** `IsMacAddressOid` dereferenced
  the first `ULONG` of the input buffer after checking only that the buffer was
  non-null. It now also requires `InputBufferLength >= sizeof(ULONG)`, threaded
  from the dispatch stack location.

## Findings

The Sev-1 use-after-free is now fixed (see below). The control-device ACL remains
open: it is a coordinated source-plus-build-file change, and it needs the same
Driver Verifier validation the rest of Layer 1 is waiting on.

### Use-after-free on the active profile (Sev-1): FIXED

The getters (`PhantomGetDiskProfile` and the rest) read `g_ActiveProfile` without
the lock and returned a pointer *into* the profile. A filter completion routine
then dereferenced that pointer while, concurrently, `PhantomProfileStoreSet` or
`PhantomProfileStoreClear` swapped the pointer under the lock and
`ExFreePoolWithTag`d the old profile. The reader was left holding a pointer into
freed non-paged pool: pool corruption, then a bugcheck, under the exact load the
driver is built for (spoofing active while a profile is updated or cleared). The
interlocked swap protected against a torn pointer read, not against freeing
memory a reader was still using.

**Fixed** with the copy-out design. Each getter now takes `g_ProfileLock`, copies
the requested record into a caller-provided struct, and returns `BOOLEAN` for
presence. The five filters read their own stack copy, so `Set`/`Clear` can free
the old profile safely: no reader holds a pointer into it once the lock is
released. `Set` and `Clear` are unchanged; they already free after the swap,
which is now safe. The change compiles clean in the WDK CI build. It still needs
Driver Verifier validation on a kernel test machine before it is trusted at
runtime, which is deferred with the rest of Layer 1.

### Control device has no restrictive ACL or caller check (Sev-2, open)

`DriverEntry` creates `\Device\PhantomSpoof` with `IoCreateDevice` and
`FILE_DEVICE_SECURE_OPEN`, but no explicit security descriptor, and
`PhantomHandleControlIoctl` performs no caller privilege check. Whether an
unprivileged process can open the device and send `IOCTL_PHANTOM_SET_PROFILE` /
`CLEAR_PROFILE` then depends on the default DACL. A driver that rewrites hardware
identity in the kernel should restrict its control interface to SYSTEM and
Administrators explicitly.

**Fix design.** Create the control device with `WdmlibIoCreateDeviceSecure`
(`<wdmsec.h>`, link `wdmsec.lib`) and an SDDL such as
`SDDL_DEVOBJ_SYS_ALL_ADMIN_ALL`, or set the security in `phantom.inf`. This is a
coordinated source plus build-file change; it is left for the WDK build.

## Completeness gaps (why Layer 1 is not shippable yet)

The C is substantial, but the driver does not currently function end to end:

- **`IOCTL_PHANTOM_ATTACH_FILTER` / `DETACH_FILTER` return `STATUS_NOT_IMPLEMENTED`.**
  Nothing ever attaches a filter to a device stack, so every interception path is
  dead. This is the single largest gap.
- **GPU PnP interception is never dispatched.** `PhantomInterceptGpuPnpQueryId`
  exists, but `driver.c` routes `IRP_MJ_PNP` straight to passthrough, so it is
  never called.
- **EDID and TPM intercept placeholder IOCTLs.** `edid_filter.c` hooks a display
  brightness IOCTL and its own comment defers the real DXGK path; `tpm_filter.c`
  guesses the `tpm.sys` submit IOCTL. Both need the real codes.
- **No PnP `IRP_MN_REMOVE_DEVICE` handling.** A filter attached imperatively is
  not torn down when its target device is removed, so the filter device object
  dangles.
- **Teardown and calibrate races.** Unload deletes filter device objects without
  draining in-flight IRPs, and `IOCTL_PHANTOM_CALIBRATE` walks the driver device
  list without synchronization against attach/detach.

## What shipping Layer 1 requires

Done: the use-after-free is fixed, and the driver compiles and links with the WDK
in CI. Still owed, in order: restrict the control device; implement filter
attach/detach and wire GPU PnP into dispatch; replace the placeholder EDID/TPM
IOCTLs with the real ones and add PnP removal handling; then sign the driver (an
EV certificate plus Microsoft attestation signing through Partner Center, the
deferred path in `windows-driver-signing.md`), and validate under a kernel
debugger with Driver Verifier before any dogfood. Until the driver
is signed it loads only on a test-signing machine.
