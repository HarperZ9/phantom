/*
 * profile_store.c — kernel-side profile storage.
 *
 * Stores the active spoofing profile in non-paged pool. Updates are atomic
 * via interlocked pointer swap so filter threads never see a torn read.
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

const PHANTOM_DISK_PROFILE* PhantomGetDiskProfile(_In_ ULONG Index)
{
    PHANTOM_KERNEL_PROFILE* p = g_ActiveProfile;
    if (!p || Index >= p->DiskCount) {
        return NULL;
    }
    return &p->Disks[Index];
}

const PHANTOM_NIC_PROFILE* PhantomGetNicProfile(_In_ ULONG Index)
{
    PHANTOM_KERNEL_PROFILE* p = g_ActiveProfile;
    if (!p || Index >= p->NicCount) {
        return NULL;
    }
    return &p->Nics[Index];
}

const PHANTOM_GPU_PROFILE* PhantomGetGpuProfile(_In_ ULONG Index)
{
    PHANTOM_KERNEL_PROFILE* p = g_ActiveProfile;
    if (!p || Index >= p->GpuCount) {
        return NULL;
    }
    return &p->Gpus[Index];
}

const PHANTOM_TPM_PROFILE* PhantomGetTpmProfile(VOID)
{
    PHANTOM_KERNEL_PROFILE* p = g_ActiveProfile;
    if (!p || !p->HasTpm) {
        return NULL;
    }
    return &p->Tpm;
}

const PHANTOM_DISPLAY_PROFILE* PhantomGetDisplayProfile(_In_ ULONG Index)
{
    PHANTOM_KERNEL_PROFILE* p = g_ActiveProfile;
    if (!p || Index >= p->DisplayCount) {
        return NULL;
    }
    return &p->Displays[Index];
}
