/*
 * profile_store.c — kernel-side profile storage.
 *
 * Stores the active spoofing profile in non-paged pool. A spin lock guards the
 * pointer: Set and Clear swap it and free the old profile under the lock, and
 * readers copy the fields they need out under the same lock. That is what makes
 * the free safe: no reader ever holds a pointer into a profile after the lock is
 * released, so Set/Clear can never free memory a reader is still using.
 */

#include "profile_store.h"

static PHANTOM_KERNEL_PROFILE* g_ActiveProfile = NULL;
static KSPIN_LOCK              g_ProfileLock;

VOID PhantomProfileStoreInit(VOID)
{
    g_ActiveProfile = NULL;
    KeInitializeSpinLock(&g_ProfileLock);
}

VOID PhantomProfileStoreCleanup(VOID)
{
    PhantomProfileStoreClear();
}

NTSTATUS PhantomProfileStoreSet(
    _In_reads_bytes_(Length) PVOID Buffer,
    _In_ ULONG Length
)
{
    PHANTOM_KERNEL_PROFILE* newProfile;
    PHANTOM_KERNEL_PROFILE* oldProfile;
    KIRQL oldIrql;

    if (Length < sizeof(PHANTOM_KERNEL_PROFILE)) {
        return STATUS_BUFFER_TOO_SMALL;
    }

    /* Validate magic */
    if (((PHANTOM_KERNEL_PROFILE*)Buffer)->Magic != PHANTOM_PROFILE_MAGIC) {
        return STATUS_INVALID_PARAMETER;
    }

    if (((PHANTOM_KERNEL_PROFILE*)Buffer)->DiskCount > PHANTOM_MAX_DISKS ||
        ((PHANTOM_KERNEL_PROFILE*)Buffer)->NicCount > PHANTOM_MAX_NICS ||
        ((PHANTOM_KERNEL_PROFILE*)Buffer)->GpuCount > PHANTOM_MAX_GPUS ||
        ((PHANTOM_KERNEL_PROFILE*)Buffer)->DisplayCount > PHANTOM_MAX_DISPLAYS) {
        return STATUS_INVALID_PARAMETER;
    }

    /*
     * Validate every per-field length before it can drive a copy. These
     * lengths come from userland; an out-of-range value would otherwise let
     * a filter read past a fixed source array (disk serial/model), and in the
     * GPU PnP path would integer-overflow the allocation size and heap-write
     * past the buffer. The counts above are already bounded, so these loops
     * stay in range.
     */
    {
        PHANTOM_KERNEL_PROFILE* p = (PHANTOM_KERNEL_PROFILE*)Buffer;
        ULONG i;
        for (i = 0; i < p->DiskCount; i++) {
            if (p->Disks[i].SerialLength > PHANTOM_MAX_SERIAL_LEN ||
                p->Disks[i].ModelLength > PHANTOM_MAX_MODEL_LEN ||
                p->Disks[i].FirmwareRevLength > PHANTOM_MAX_SERIAL_LEN) {
                return STATUS_INVALID_PARAMETER;
            }
        }
        for (i = 0; i < p->GpuCount; i++) {
            if (p->Gpus[i].PnpInstanceIdLength > PHANTOM_MAX_MODEL_LEN) {
                return STATUS_INVALID_PARAMETER;
            }
        }
    }

    newProfile = (PHANTOM_KERNEL_PROFILE*)ExAllocatePool2(
        POOL_FLAG_NON_PAGED,
        sizeof(PHANTOM_KERNEL_PROFILE),
        PHANTOM_POOL_TAG
    );
    if (!newProfile) {
        return STATUS_INSUFFICIENT_RESOURCES;
    }

    RtlCopyMemory(newProfile, Buffer, sizeof(PHANTOM_KERNEL_PROFILE));

    KeAcquireSpinLock(&g_ProfileLock, &oldIrql);
    oldProfile = g_ActiveProfile;
    g_ActiveProfile = newProfile;
    KeReleaseSpinLock(&g_ProfileLock, oldIrql);

    if (oldProfile) {
        ExFreePoolWithTag(oldProfile, PHANTOM_POOL_TAG);
    }

    return STATUS_SUCCESS;
}

VOID PhantomProfileStoreClear(VOID)
{
    PHANTOM_KERNEL_PROFILE* oldProfile;
    KIRQL oldIrql;

    KeAcquireSpinLock(&g_ProfileLock, &oldIrql);
    oldProfile = g_ActiveProfile;
    g_ActiveProfile = NULL;
    KeReleaseSpinLock(&g_ProfileLock, oldIrql);

    if (oldProfile) {
        ExFreePoolWithTag(oldProfile, PHANTOM_POOL_TAG);
    }
}

BOOLEAN PhantomProfileIsActive(VOID)
{
    return (g_ActiveProfile != NULL);
}

BOOLEAN PhantomGetDiskProfile(_In_ ULONG Index, _Out_ PHANTOM_DISK_PROFILE* Out)
{
    KIRQL oldIrql;
    BOOLEAN found = FALSE;

    KeAcquireSpinLock(&g_ProfileLock, &oldIrql);
    if (g_ActiveProfile && Index < g_ActiveProfile->DiskCount) {
        RtlCopyMemory(Out, &g_ActiveProfile->Disks[Index], sizeof(*Out));
        found = TRUE;
    }
    KeReleaseSpinLock(&g_ProfileLock, oldIrql);
    return found;
}

BOOLEAN PhantomGetNicProfile(_In_ ULONG Index, _Out_ PHANTOM_NIC_PROFILE* Out)
{
    KIRQL oldIrql;
    BOOLEAN found = FALSE;

    KeAcquireSpinLock(&g_ProfileLock, &oldIrql);
    if (g_ActiveProfile && Index < g_ActiveProfile->NicCount) {
        RtlCopyMemory(Out, &g_ActiveProfile->Nics[Index], sizeof(*Out));
        found = TRUE;
    }
    KeReleaseSpinLock(&g_ProfileLock, oldIrql);
    return found;
}

BOOLEAN PhantomGetGpuProfile(_In_ ULONG Index, _Out_ PHANTOM_GPU_PROFILE* Out)
{
    KIRQL oldIrql;
    BOOLEAN found = FALSE;

    KeAcquireSpinLock(&g_ProfileLock, &oldIrql);
    if (g_ActiveProfile && Index < g_ActiveProfile->GpuCount) {
        RtlCopyMemory(Out, &g_ActiveProfile->Gpus[Index], sizeof(*Out));
        found = TRUE;
    }
    KeReleaseSpinLock(&g_ProfileLock, oldIrql);
    return found;
}

BOOLEAN PhantomGetTpmProfile(_Out_ PHANTOM_TPM_PROFILE* Out)
{
    KIRQL oldIrql;
    BOOLEAN found = FALSE;

    KeAcquireSpinLock(&g_ProfileLock, &oldIrql);
    if (g_ActiveProfile && g_ActiveProfile->HasTpm) {
        RtlCopyMemory(Out, &g_ActiveProfile->Tpm, sizeof(*Out));
        found = TRUE;
    }
    KeReleaseSpinLock(&g_ProfileLock, oldIrql);
    return found;
}

BOOLEAN PhantomGetDisplayProfile(_In_ ULONG Index, _Out_ PHANTOM_DISPLAY_PROFILE* Out)
{
    KIRQL oldIrql;
    BOOLEAN found = FALSE;

    KeAcquireSpinLock(&g_ProfileLock, &oldIrql);
    if (g_ActiveProfile && Index < g_ActiveProfile->DisplayCount) {
        RtlCopyMemory(Out, &g_ActiveProfile->Displays[Index], sizeof(*Out));
        found = TRUE;
    }
    KeReleaseSpinLock(&g_ProfileLock, oldIrql);
    return found;
}
