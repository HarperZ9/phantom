/*
 * phantom-driver: Kernel IRP filter driver for hardware identity privacy.
 *
 * Layer 1 — attaches as an upper filter to disk, NIC, and GPU device stacks
 * via IoAttachDeviceToDeviceStack. Intercepts IRP_MJ_DEVICE_CONTROL to
 * replace hardware identifiers in IOCTL responses with profile values.
 *
 * Architecture:
 *   - Control device (\Device\PhantomSpoof) for CLI <-> driver IPC
 *   - Filter device objects attached per target device stack
 *   - Profile data received from userland via IOCTL, stored in non-paged pool
 *   - Timing normalization engine delays responses to match calibrated baselines
 */

#include "phantom.h"
#include "profile_store.h"
#include "timing.h"

#include <wdmsec.h>   /* WdmlibIoCreateDeviceSecure + the SDDL macros */

DRIVER_INITIALIZE DriverEntry;
DRIVER_UNLOAD     PhantomUnload;

static PDEVICE_OBJECT g_ControlDevice = NULL;

static NTSTATUS PhantomDispatchControl(PDEVICE_OBJECT DeviceObject, PIRP Irp);
static NTSTATUS PhantomDispatchPassthrough(PDEVICE_OBJECT DeviceObject, PIRP Irp);
static NTSTATUS PhantomFilterDeviceControl(PDEVICE_OBJECT DeviceObject, PIRP Irp);

NTSTATUS DriverEntry(
    _In_ PDRIVER_OBJECT  DriverObject,
    _In_ PUNICODE_STRING RegistryPath
)
{
    NTSTATUS status;
    UNICODE_STRING devName = RTL_CONSTANT_STRING(PHANTOM_DEVICE_NAME);
    UNICODE_STRING symLink = RTL_CONSTANT_STRING(PHANTOM_SYMLINK);

    /*
     * Restrict the control device to SYSTEM and Administrators. This driver
     * rewrites hardware identity in the kernel, so an unprivileged process must
     * not be able to open \Device\PhantomSpoof and send SET_PROFILE /
     * CLEAR_PROFILE. SDDL_DEVOBJ_SYS_ALL_ADMIN_ALL grants GENERIC_ALL to SYSTEM
     * and the Administrators group and nothing to anyone else; the CLI runs
     * elevated and the service runs as LocalSystem, so both still open it.
     */
    DECLARE_CONST_UNICODE_STRING(controlSddl, SDDL_DEVOBJ_SYS_ALL_ADMIN_ALL);

    UNREFERENCED_PARAMETER(RegistryPath);

    PhantomProfileStoreInit();
    PhantomTimingInit();

    status = WdmlibIoCreateDeviceSecure(
        DriverObject,
        0,
        &devName,
        FILE_DEVICE_UNKNOWN,
        FILE_DEVICE_SECURE_OPEN,
        FALSE,
        &controlSddl,
        NULL,
        &g_ControlDevice
    );
    if (!NT_SUCCESS(status)) {
        return status;
    }

    status = IoCreateSymbolicLink(&symLink, &devName);
    if (!NT_SUCCESS(status)) {
        IoDeleteDevice(g_ControlDevice);
        return status;
    }

    for (ULONG i = 0; i <= IRP_MJ_MAXIMUM_FUNCTION; i++) {
        DriverObject->MajorFunction[i] = PhantomDispatchPassthrough;
    }

    DriverObject->MajorFunction[IRP_MJ_CREATE] = PhantomDispatchPassthrough;
    DriverObject->MajorFunction[IRP_MJ_CLOSE]  = PhantomDispatchPassthrough;
    DriverObject->MajorFunction[IRP_MJ_DEVICE_CONTROL] = PhantomDispatchControl;
    DriverObject->DriverUnload = PhantomUnload;

    return STATUS_SUCCESS;
}

VOID PhantomUnload(
    _In_ PDRIVER_OBJECT DriverObject
)
{
    UNICODE_STRING symLink = RTL_CONSTANT_STRING(PHANTOM_SYMLINK);

    PhantomDetachAllFilters(DriverObject);
    PhantomProfileStoreCleanup();

    IoDeleteSymbolicLink(&symLink);
    if (g_ControlDevice) {
        IoDeleteDevice(g_ControlDevice);
    }
}

/*
 * Route IRP_MJ_DEVICE_CONTROL: control device IOCTLs go to IPC handler,
 * filter device IOCTLs go to the filter interception path.
 */
static NTSTATUS PhantomDispatchControl(
    PDEVICE_OBJECT DeviceObject,
    PIRP Irp
)
{
    if (DeviceObject == g_ControlDevice) {
        return PhantomHandleControlIoctl(DeviceObject, Irp);
    }

    return PhantomFilterDeviceControl(DeviceObject, Irp);
}

static NTSTATUS PhantomDispatchPassthrough(
    PDEVICE_OBJECT DeviceObject,
    PIRP Irp
)
{
    PPHANTOM_FILTER_EXT ext;

    if (DeviceObject == g_ControlDevice) {
        Irp->IoStatus.Status = STATUS_SUCCESS;
        Irp->IoStatus.Information = 0;
        IoCompleteRequest(Irp, IO_NO_INCREMENT);
        return STATUS_SUCCESS;
    }

    ext = (PPHANTOM_FILTER_EXT)DeviceObject->DeviceExtension;
    IoSkipCurrentIrpStackLocation(Irp);
    return IoCallDriver(ext->LowerDevice, Irp);
}

/*
 * Filter path for IRP_MJ_DEVICE_CONTROL on attached device stacks.
 * Inspects the IOCTL code; if it targets an identifier we spoof,
 * sets a completion routine to rewrite the response buffer.
 */
static NTSTATUS PhantomFilterDeviceControl(
    PDEVICE_OBJECT DeviceObject,
    PIRP Irp
)
{
    PPHANTOM_FILTER_EXT ext = (PPHANTOM_FILTER_EXT)DeviceObject->DeviceExtension;
    PIO_STACK_LOCATION irpSp = IoGetCurrentIrpStackLocation(Irp);
    ULONG ioctl = irpSp->Parameters.DeviceIoControl.IoControlCode;

    if (!PhantomProfileIsActive()) {
        IoSkipCurrentIrpStackLocation(Irp);
        return IoCallDriver(ext->LowerDevice, Irp);
    }

    switch (ext->FilterType) {
    case PHANTOM_FILTER_DISK:
        if (PhantomIsDiskIdentIoctl(ioctl)) {
            return PhantomInterceptDiskIoctl(ext, Irp, irpSp);
        }
        break;

    case PHANTOM_FILTER_NIC:
        if (PhantomIsNicIdentIoctl(ioctl)) {
            return PhantomInterceptNicIoctl(ext, Irp, irpSp);
        }
        break;

    case PHANTOM_FILTER_GPU:
        if (PhantomIsGpuIdentIoctl(ioctl)) {
            return PhantomInterceptGpuIoctl(ext, Irp, irpSp);
        }
        break;

    case PHANTOM_FILTER_TPM:
        if (PhantomIsTpmIdentIoctl(ioctl)) {
            return PhantomInterceptTpmIoctl(ext, Irp, irpSp);
        }
        break;

    case PHANTOM_FILTER_EDID:
        if (PhantomIsEdidIdentIoctl(ioctl)) {
            return PhantomInterceptEdidIoctl(ext, Irp, irpSp);
        }
        break;

    default:
        break;
    }

    IoSkipCurrentIrpStackLocation(Irp);
    return IoCallDriver(ext->LowerDevice, Irp);
}
