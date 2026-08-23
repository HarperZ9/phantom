# Windows Layer 0/1 signing path

What it takes to sign and ship the Layer 1 kernel driver (`phantom-driver`) and
the Layer 0 UEFI DXE module (`phantom-dxe`) on Windows. Both are modeled but not
shipped in v1.0; this is the path from here to a loadable, signed binary, with
the current facts and the honest gates.

Signing is deferred by decision, pacing spend to demand. This document exists so
the path is concrete and ready, not vague, when demand arrives.

## The order of operations

Signing is the last step, not the first. For Layer 1 the sequence is:

1. Make the driver functional. It is not yet: `IOCTL_PHANTOM_ATTACH_FILTER` /
   `DETACH_FILTER` return `STATUS_NOT_IMPLEMENTED`, so nothing attaches, and there
   is an open use-after-free on the active profile. See `kernel-driver-review.md`.
2. Build it with the WDK. Done as of the `driver build (WDK)` CI job; compiling
   at Level4 with warnings-as-errors is the first real verification of the C.
3. Validate it under Driver Verifier on a test-signing machine (kernel bugs
   bugcheck the box, so this bar is above any unit test).
4. Sign it. This is the step below.

Signing an unbuilt or non-functional driver is premature. The build is the
prerequisite that is now in place; the functional work and the validation come
before a real submission.

## Layer 1: kernel-mode driver signing

Since Windows 10 version 1607, a new kernel-mode driver loads only if Microsoft
has signed it. You cannot fully sign a production kernel driver with your own
certificate. The only path to a driver that loads on a normal machine is
Microsoft's signing service.

### Attestation signing (the lighter path)

For a driver that does not need the full hardware-lab certification, attestation
signing is the route:

1. Hold an Extended Validation (EV) code-signing certificate. This is the hard
   gate: a CA issues an EV certificate only to an organization with a verified
   physical address that has existed for about three years. A sole developer with
   no registered entity does not qualify directly; this needs a business entity.
2. Register in Microsoft Partner Center (the Windows Hardware Dev Center
   program). The EV certificate establishes the account.
3. Build the driver and package it as a CAB.
4. Sign the CAB with the EV certificate.
5. Submit the CAB to the Partner Center hardware dashboard. Microsoft
   attestation-signs the driver inside and returns it.
6. Ship the Microsoft-signed driver.

Attestation signing does not run hardware compatibility tests; it is a Microsoft
counter-signature that lets the driver load. It does not grant Windows Update
distribution.

### WHQL certification (the heavier path)

Full Windows Hardware Quality Labs certification adds HLK test passes and enables
Windows Update distribution. It is more work and is not needed to load a driver a
user installs directly. Attestation is the right first target.

### Cost and time

- EV code-signing certificate: on the order of a few hundred US dollars per year,
  issued to an organization, delivered on a hardware token or a cloud HSM. Lead
  time is days to weeks for the organization validation.
- Partner Center registration: a one-time fee, established with the EV
  certificate.
- The Microsoft attestation turnaround on a submitted CAB is typically fast
  (minutes to hours), once the account and certificate are in place.

### Until it is signed

An unsigned driver loads only on a machine with test-signing enabled
(`bcdedit /set testsigning on`, then a reboot, which shows a desktop watermark).
This is fine for development and the Driver Verifier validation, and it is how
the driver will be exercised before a real submission. It is not something an end
user should be asked to do.

## Layer 0: UEFI DXE module signing

The UEFI DXE module (`phantom-dxe`) is loaded by firmware, so it answers to
Secure Boot, not to the kernel driver-signing policy. A UEFI module runs only if
it is signed by a certificate in the firmware's `db`, or its hash is in `db`.

For a third party that does not control the firmware's keys, the path is
Microsoft's UEFI signing service, through the same Partner Center hardware
dashboard, which signs third-party UEFI components with the Microsoft UEFI CA. A
boot-services DXE driver (subsystem `EFI_BOOT_SERVICE_DRIVER`) is signed with the
Microsoft Option ROM UEFI CA.

This path is stricter than driver attestation. Microsoft reviews UEFI
submissions closely, because a signed UEFI module runs before the OS and inside
the Secure Boot trust boundary. Expect a real review, not an automated
counter-signature.

One moving part to track: the Microsoft Secure Boot and UEFI CA certificates are
in a 2026 expiration and rollover. Any UEFI signing work has to target the
current CA generation (the UEFI CA 2023 line), and the trust anchors on shipping
machines are changing during this window. Confirm the current CA and process at
submission time rather than trusting a cached answer.

Because of the stricter review and the Secure Boot trust implications, Layer 0
signing is the later, harder half of this path. Layer 1 attestation is the first
target.

## CI wiring

The `driver build (WDK)` CI job compiles the driver today. The signing step is
not wired yet because there is nothing to sign until an EV certificate and a
Partner Center account exist. When they do, the shape mirrors the app installer's
`phantom-installer/sign.cmd`, which signs with the vendor certificate and no-ops
when the certificate secret is absent, so unsigned dev and PR builds still
succeed. The driver signing step will:

1. Build the driver (already wired).
2. Package the driver and its INF into a CAB.
3. Sign the CAB with the EV certificate from a CI secret.
4. Submit to Partner Center (the Hardware API) and retrieve the signed driver.

Steps 3 and 4 activate only when the certificate and account secrets are set,
exactly as the app signing does today.

## Summary of gates

| Gate | Layer 1 (driver) | Layer 0 (UEFI DXE) |
|---|---|---|
| Functional code | attach/detach + UAF fix owed | EFI-var + SMBIOS rewrite present, unverified |
| Build | done (WDK CI) | needs EDK II build (not yet wired) |
| Certificate | EV, organization only | EV, plus Microsoft UEFI submission |
| Signer | Microsoft attestation (Partner Center) | Microsoft UEFI CA (Partner Center) |
| Distribution | direct install; WHQL adds Windows Update | firmware trust, strict review |

The nearest concrete step is not a certificate. It is finishing and validating
the driver, which the build now makes possible.
