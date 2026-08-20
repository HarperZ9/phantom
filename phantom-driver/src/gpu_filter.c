/*
 * gpu_filter.c — GPU PnP instance ID interception.
 *
 * Intercepts IRP_MN_QUERY_ID on the GPU device stack to replace the
 * Plug-and-Play device instance ID. The PnP ID encodes vendor, device,
 * and subsystem IDs in a format like:
 *
 *   PCI\VEN_10DE&DEV_2684&SUBSYS_38801028&REV_A1\4&2F5C9B7&0&0008
 *
 * This is a PnP IRP (not IOCTL), so we intercept IRP_MJ_PNP with
 * IRP_MN_QUERY_ID and BusQueryInstanceID / BusQueryDeviceID.
 */

#include "phantom.h"
#include "profile_store.h"
#include "timing.h"

#define IRP_MN_QUERY_ID  0x13

/* BusQueryDeviceID = 0, BusQueryHardwareIDs = 1, BusQueryInstanceID = 4 */
typedef enum {
    PhantomBusQueryDeviceID     = 0,
    PhantomBusQueryHardwareIDs  = 1,
    PhantomBusQueryInstanceID   = 4,
} PHANTOM_BUS_QUERY_TYPE;

BOOLEAN PhantomIsGpuIdentIoctl(ULONG IoControlCode)
{
    /*
     * GPU identity is queried via PnP IRPs, not IOCTLs.
     * This function is called from the IOCTL dispatch path, so it
     * always returns FALSE — GPU interception is handled separately
     * in the PnP dispatch path (see PhantomDispatchPnp below).
     *
     * The function exists to satisfy the filter dispatch interface.
     */
    UNREFERENCED_PARAMETER(IoControlCode);
    return FALSE;
}

NTSTATUS PhantomInterceptGpuIoctl(
    PPHANTOM_FILTER_EXT Ext,
    PIRP Irp,
    PIO_STACK_LOCATION IrpSp
)
{
    /* GPU uses PnP path, not IOCTL. This is a no-op fallback. */
    UNREFERENCED_PARAMETER(IrpSp);
    IoSkipCurrentIrpStackLocation(Irp);
    return IoCallDriver(Ext->LowerDevice, Irp);
}

/*
 * PnP query ID completion routine — replaces the returned ID string
 * with the profile's spoofed PnP instance ID.
 */
static NTSTATUS GpuPnpQueryIdCompletion(
    PDEVICE_OBJECT DeviceObject,
    PIRP Irp,
    PVOID Context
)
{
    PPHANTOM_FILTER_EXT ext = (PPHANTOM_FILTER_EXT)Context;
    PIO_STACK_LOCATION irpSp = IoGetCurrentIrpStackLocation(Irp);
    const PHANTOM_GPU_PROFILE* profile;
    PWCHAR originalId;
    PWCHAR newId;
    ULONG newIdBytes;

    UNREFERENCED_PARAMETER(DeviceObject);

    if (!NT_SUCCESS(Irp->IoStatus.Status)) {
        goto done;
    }

    profile = PhantomGetGpuProfile(ext->DeviceIndex);
    if (!profile || profile->PnpInstanceIdLength == 0) {
        goto done;
    }

    originalId = (PWCHAR)Irp->IoStatus.Information;

    /* Allocate a new wide string for the spoofed ID */
    newIdBytes = (profile->PnpInstanceIdLength + 1) * sizeof(WCHAR);
    newId = (PWCHAR)ExAllocatePool2(
        POOL_FLAG_PAGED,
        newIdBytes,
        PHANTOM_POOL_TAG
    );
    if (!newId) {
        goto done;
    }

    /* Convert ANSI profile string to wide char */
    for (ULONG i = 0; i < profile->PnpInstanceIdLength; i++) {
        newId[i] = (WCHAR)profile->PnpInstanceId[i];
    }
    newId[profile->PnpInstanceIdLength] = L'\0';

    /* Free the original and replace */
    if (originalId) {
        ExFreePool(originalId);
    }
    Irp->IoStatus.Information = (ULONG_PTR)newId;

done:
    if (Irp->PendingReturned) {
        IoMarkIrpPending(Irp);
    }
    return STATUS_SUCCESS;
}

/*
 * Called from the PnP dispatch path for GPU filter devices.
 * Intercepts IRP_MN_QUERY_ID for device ID and instance ID queries.
 */
NTSTATUS PhantomInterceptGpuPnpQueryId(
    PPHANTOM_FILTER_EXT Ext,
    PIRP Irp,
    PIO_STACK_LOCATION IrpSp
)
{
    ULONG idType;

    if (IrpSp->MinorFunction != IRP_MN_QUERY_ID) {
        IoSkipCurrentIrpStackLocation(Irp);
        return IoCallDriver(Ext->LowerDevice, Irp);
    }

    idType = IrpSp->Parameters.QueryId.IdType;

    if (idType != PhantomBusQueryDeviceID &&
        idType != PhantomBusQueryInstanceID)
    {
        IoSkipCurrentIrpStackLocation(Irp);
        return IoCallDriver(Ext->LowerDevice, Irp);
    }

    if (!PhantomProfileIsActive()) {
        IoSkipCurrentIrpStackLocation(Irp);
        return IoCallDriver(Ext->LowerDevice, Irp);
    }

    IoCopyCurrentIrpStackLocationToNext(Irp);
    IoSetCompletionRoutine(
        Irp,
        GpuPnpQueryIdCompletion,
        Ext,
        TRUE,
        FALSE,
        FALSE
    );
    return IoCallDriver(Ext->LowerDevice, Irp);
}
