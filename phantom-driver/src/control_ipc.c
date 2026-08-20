/*
 * control_ipc.c — userland <-> driver IPC via the control device.
 *
 * The CLI communicates with the driver through \Device\PhantomSpoof
 * using DeviceIoControl. Supported operations:
 *
 *   IOCTL_PHANTOM_SET_PROFILE    — load a profile into the kernel store
 *   IOCTL_PHANTOM_CLEAR_PROFILE  — deactivate spoofing
 *   IOCTL_PHANTOM_GET_STATUS     — query driver state
 *   IOCTL_PHANTOM_ATTACH_FILTER  — attach filter to a device stack
 *   IOCTL_PHANTOM_DETACH_FILTER  — remove a filter from a device stack
 *   IOCTL_PHANTOM_CALIBRATE      — run timing calibration pass
 */

#include "phantom.h"
#include "profile_store.h"
#include "timing.h"

/* Counters for status reporting */
static volatile LONG g_AttachedDiskCount = 0;
static volatile LONG g_AttachedNicCount  = 0;
static volatile LONG g_AttachedGpuCount  = 0;
static volatile LONG g_AttachedTpmCount  = 0;
static volatile LONG g_AttachedEdidCount = 0;
static volatile LONG g_InterceptedCount  = 0;

NTSTATUS PhantomHandleControlIoctl(
    PDEVICE_OBJECT DeviceObject,
    PIRP Irp
)
{
    PIO_STACK_LOCATION irpSp = IoGetCurrentIrpStackLocation(Irp);
    ULONG ioctl = irpSp->Parameters.DeviceIoControl.IoControlCode;
    PVOID inputBuffer = Irp->AssociatedIrp.SystemBuffer;
    ULONG inputLen = irpSp->Parameters.DeviceIoControl.InputBufferLength;
    ULONG outputLen = irpSp->Parameters.DeviceIoControl.OutputBufferLength;
    NTSTATUS status = STATUS_SUCCESS;
    ULONG bytesReturned = 0;

    UNREFERENCED_PARAMETER(DeviceObject);

    switch (ioctl) {

    case IOCTL_PHANTOM_SET_PROFILE:
        if (!inputBuffer || inputLen < sizeof(PHANTOM_KERNEL_PROFILE)) {
            status = STATUS_BUFFER_TOO_SMALL;
            break;
        }
        status = PhantomProfileStoreSet(inputBuffer, inputLen);
        break;

    case IOCTL_PHANTOM_CLEAR_PROFILE:
        PhantomProfileStoreClear();
        status = STATUS_SUCCESS;
        break;

    case IOCTL_PHANTOM_GET_STATUS:
        if (outputLen < sizeof(PHANTOM_STATUS)) {
            status = STATUS_BUFFER_TOO_SMALL;
            break;
        }
        {
            PHANTOM_STATUS* st = (PHANTOM_STATUS*)Irp->AssociatedIrp.SystemBuffer;
            st->Version = 1;
            st->ProfileActive = PhantomProfileIsActive();
            st->AttachedDiskCount = (ULONG)g_AttachedDiskCount;
            st->AttachedNicCount  = (ULONG)g_AttachedNicCount;
            st->AttachedGpuCount  = (ULONG)g_AttachedGpuCount;
            st->AttachedTpmCount  = (ULONG)g_AttachedTpmCount;
            st->AttachedEdidCount = (ULONG)g_AttachedEdidCount;
            st->InterceptedIoctlCount = (ULONG)g_InterceptedCount;
            st->TimingCalibratedCount = 0;
            bytesReturned = sizeof(PHANTOM_STATUS);
        }
        break;

    case IOCTL_PHANTOM_ATTACH_FILTER:
        /*
         * Input: PHANTOM_ATTACH_REQUEST { FilterType, DevicePath }
         * Attaches a filter device to the specified device stack.
         *
         * Full implementation requires:
         *   1. Open the target device by name
         *   2. Create a filter device object
         *   3. IoAttachDeviceToDeviceStack
         *   4. Set up the device extension with filter type and index
         *
         * This is the integration point for dynamic filter attachment
         * from the CLI.
         */
        status = STATUS_NOT_IMPLEMENTED;
        break;

    case IOCTL_PHANTOM_DETACH_FILTER:
        status = STATUS_NOT_IMPLEMENTED;
        break;

    case IOCTL_PHANTOM_CALIBRATE:
        /*
         * Triggers timing calibration by sending real IOCTLs through
         * attached filter stacks and recording response times.
         * Input: ULONG[2] = { FilterType, IoControlCode }
         */
        if (!inputBuffer || inputLen < 2 * sizeof(ULONG)) {
            status = STATUS_BUFFER_TOO_SMALL;
        } else {
            ULONG filterType = ((ULONG*)inputBuffer)[0];
            ULONG targetIoctl = ((ULONG*)inputBuffer)[1];
            PDEVICE_OBJECT dev;
            BOOLEAN found = FALSE;

            for (dev = DeviceObject->DriverObject->DeviceObject; dev; dev = dev->NextDevice) {
                if (dev->DeviceExtension) {
                    PPHANTOM_FILTER_EXT fext = (PPHANTOM_FILTER_EXT)dev->DeviceExtension;
                    if ((ULONG)fext->FilterType == filterType) {
                        status = PhantomTimingCalibrate(fext, targetIoctl);
                        found = TRUE;
                        break;
                    }
                }
            }
            if (!found) {
                status = STATUS_DEVICE_NOT_CONNECTED;
            }
        }
        break;

    default:
        status = STATUS_INVALID_DEVICE_REQUEST;
        break;
    }

    Irp->IoStatus.Status = status;
    Irp->IoStatus.Information = bytesReturned;
    IoCompleteRequest(Irp, IO_NO_INCREMENT);
    return status;
}

/*
 * Detach and delete all filter device objects during unload.
 */
VOID PhantomDetachAllFilters(_In_ PDRIVER_OBJECT DriverObject)
{
    PDEVICE_OBJECT device = DriverObject->DeviceObject;

    while (device) {
        PDEVICE_OBJECT next = device->NextDevice;
        PPHANTOM_FILTER_EXT ext;

        /* Skip the control device (no extension) */
        if (device->DeviceExtension) {
            ext = (PPHANTOM_FILTER_EXT)device->DeviceExtension;
            if (ext->LowerDevice) {
                IoDetachDevice(ext->LowerDevice);
            }
            IoDeleteDevice(device);
        }

        device = next;
    }
}
