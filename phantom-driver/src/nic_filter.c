/*
 * nic_filter.c — network adapter MAC address interception.
 *
 * Intercepts NDIS OID requests for MAC addresses:
 *
 *   OID_802_3_PERMANENT_ADDRESS — hardware-burned MAC (cannot be changed
 *   via normal OS settings; queried separately by fingerprinting software)
 *
 *   OID_802_3_CURRENT_ADDRESS — active MAC address (can be changed via
 *   OS settings, but fingerprinting software compares it against permanent)
 *
 * Both MUST return the same spoofed value — a mismatch between permanent
 * and current is a strong spoofing detection signal.
 *
 * Implementation: IRP filter on the NIC device stack, intercepting
 * IOCTL_NDIS_QUERY_GLOBAL_STATS which wraps OID queries from userland.
 */

#include "phantom.h"
#include "profile_store.h"
#include "timing.h"

/* NDIS OIDs for MAC address queries */
#define OID_802_3_PERMANENT_ADDRESS  0x01010101
#define OID_802_3_CURRENT_ADDRESS    0x01010102
#define OID_GEN_PHYSICAL_MEDIUM      0x00010202

/* IOCTL used by userland to query NDIS OIDs */
#define IOCTL_NDIS_QUERY_GLOBAL_STATS  \
    CTL_CODE(FILE_DEVICE_PHYSICAL_NETCARD, 0, METHOD_OUT_DIRECT, FILE_ANY_ACCESS)

BOOLEAN PhantomIsNicIdentIoctl(ULONG IoControlCode)
{
    return (IoControlCode == IOCTL_NDIS_QUERY_GLOBAL_STATS);
}

/*
 * Check if the OID in the input buffer is a MAC address query.
 */
static BOOLEAN IsMacAddressOid(PIRP Irp, ULONG InputLen)
{
    ULONG* oidPtr;

    /* The OID is a ULONG at the start of the input buffer; reject anything
     * too small to hold it before dereferencing. */
    if (!Irp->AssociatedIrp.SystemBuffer || InputLen < sizeof(ULONG)) {
        return FALSE;
    }

    oidPtr = (ULONG*)Irp->AssociatedIrp.SystemBuffer;

    return (*oidPtr == OID_802_3_PERMANENT_ADDRESS ||
            *oidPtr == OID_802_3_CURRENT_ADDRESS);
}

static NTSTATUS NicIoctlCompletion(
    PDEVICE_OBJECT DeviceObject,
    PIRP Irp,
    PVOID Context
)
{
    PPHANTOM_FILTER_EXT ext = (PPHANTOM_FILTER_EXT)Context;
    const PHANTOM_NIC_PROFILE* profile;
    PUCHAR macBuffer;
    ULONG* oidPtr;
    LARGE_INTEGER startTicks;

    UNREFERENCED_PARAMETER(DeviceObject);

    if (!NT_SUCCESS(Irp->IoStatus.Status)) {
        goto done;
    }

    profile = PhantomGetNicProfile(ext->DeviceIndex);
    if (!profile) {
        goto done;
    }

    oidPtr = (ULONG*)Irp->AssociatedIrp.SystemBuffer;
    if (!oidPtr) {
        goto done;
    }

    /*
     * The MAC address is returned in the MDL (METHOD_OUT_DIRECT).
     * Map the output buffer and overwrite the 6-byte MAC.
     */
    if (Irp->MdlAddress) {
        macBuffer = (PUCHAR)MmGetSystemAddressForMdlSafe(
            Irp->MdlAddress,
            NormalPagePriority
        );
    } else {
        macBuffer = NULL;
    }

    if (!macBuffer || Irp->IoStatus.Information < 6) {
        goto done;
    }

    startTicks = KeQueryPerformanceCounter(NULL);

    if (*oidPtr == OID_802_3_PERMANENT_ADDRESS) {
        RtlCopyMemory(macBuffer, profile->PermanentMac, 6);
    } else if (*oidPtr == OID_802_3_CURRENT_ADDRESS) {
        RtlCopyMemory(macBuffer, profile->CurrentMac, 6);
    }

    PhantomTimingApplyDelay(IOCTL_NDIS_QUERY_GLOBAL_STATS, startTicks);

done:
    if (Irp->PendingReturned) {
        IoMarkIrpPending(Irp);
    }
    return STATUS_SUCCESS;
}

NTSTATUS PhantomInterceptNicIoctl(
    PPHANTOM_FILTER_EXT Ext,
    PIRP Irp,
    PIO_STACK_LOCATION IrpSp
)
{
    if (!IsMacAddressOid(Irp, IrpSp->Parameters.DeviceIoControl.InputBufferLength)) {
        /* Not a MAC OID (or too small to be one) — pass through */
        IoSkipCurrentIrpStackLocation(Irp);
        return IoCallDriver(Ext->LowerDevice, Irp);
    }

    IoCopyCurrentIrpStackLocationToNext(Irp);
    IoSetCompletionRoutine(
        Irp,
        NicIoctlCompletion,
        Ext,
        TRUE,
        FALSE,
        FALSE
    );
    return IoCallDriver(Ext->LowerDevice, Irp);
}
