/*
 * tpm_filter.c — TPM identifier interception.
 *
 * Intercepts TPM Base Services (TBS) queries that return the TPM
 * manufacturer ID and firmware version. These identifiers are
 * increasingly used for device fingerprinting since TPM 2.0 is
 * required by Windows 11.
 *
 * TPM identifiers are queried via:
 *   - Tbsi_GetDeviceInfo (userland TBS API)
 *   - Direct TPM command submission via Tbsip_Submit_Command
 *   - WMI Win32_Tpm class
 *
 * At the kernel level, TPM commands flow through the TPM device stack
 * (tpm.sys) as IRP_MJ_DEVICE_CONTROL with specific IOCTL codes.
 *
 * Status: Structural scaffold. Full TPM interception requires
 * identifying and hooking the specific IOCTLs used by tpm.sys for
 * capability queries (TPM2_CC_GetCapability with TPM_PT_MANUFACTURER,
 * TPM_PT_FIRMWARE_VERSION_1, TPM_PT_FIRMWARE_VERSION_2).
 */

#include "phantom.h"
#include "profile_store.h"
#include "timing.h"

/* TPM2 command codes */
#define TPM2_CC_GET_CAPABILITY  0x0000017A

/* TPM2 property tags */
#define TPM_PT_MANUFACTURER       0x00000105
#define TPM_PT_FIRMWARE_VERSION_1 0x00000112
#define TPM_PT_FIRMWARE_VERSION_2 0x00000113

/*
 * Check if a TPM command buffer contains a GetCapability request
 * for manufacturer or firmware version properties.
 */
BOOLEAN PhantomIsTpmIdentityQuery(
    const UCHAR* CommandBuffer,
    ULONG CommandLength
)
{
    ULONG commandCode;
    ULONG property;

    /* TPM2 command header: tag(2) + size(4) + commandCode(4) */
    if (CommandLength < 10) {
        return FALSE;
    }

    commandCode = ((ULONG)CommandBuffer[6] << 24) |
                  ((ULONG)CommandBuffer[7] << 16) |
                  ((ULONG)CommandBuffer[8] << 8) |
                  ((ULONG)CommandBuffer[9]);

    if (commandCode != TPM2_CC_GET_CAPABILITY) {
        return FALSE;
    }

    /* GetCapability: capability(4) + property(4) + propertyCount(4) */
    if (CommandLength < 22) {
        return FALSE;
    }

    property = ((ULONG)CommandBuffer[14] << 24) |
               ((ULONG)CommandBuffer[15] << 16) |
               ((ULONG)CommandBuffer[16] << 8) |
               ((ULONG)CommandBuffer[17]);

    return (property == TPM_PT_MANUFACTURER ||
            property == TPM_PT_FIRMWARE_VERSION_1 ||
            property == TPM_PT_FIRMWARE_VERSION_2);
}

/*
 * Rewrite the manufacturer ID in a TPM2 GetCapability response buffer.
 *
 * Response layout for TPM_PT_MANUFACTURER:
 *   tag(2) + size(4) + responseCode(4) + moreData(1) +
 *   capabilityData { capability(4) + count(4) +
 *     properties[] { property(4) + value(4) } }
 *
 * The manufacturer value is a 4-byte ASCII string (e.g., "IFX\0", "INTC").
 */
VOID PhantomRewriteTpmManufacturer(
    UCHAR* ResponseBuffer,
    ULONG ResponseLength,
    const CHAR NewManufacturerId[4]
)
{
    /* Manufacturer value starts at offset 23 in a standard response */
    ULONG valueOffset = 23;

    if (ResponseLength < valueOffset + 4) {
        return;
    }

    RtlCopyMemory(&ResponseBuffer[valueOffset], NewManufacturerId, 4);
}

/*
 * TPM command submission IOCTL used by tpm.sys.
 * Commands are submitted as METHOD_BUFFERED with the raw TPM2 command
 * in the input buffer and the response in the output buffer.
 */
#define IOCTL_TPM_SUBMIT_COMMAND \
    CTL_CODE(FILE_DEVICE_UNKNOWN, 0x01, METHOD_BUFFERED, FILE_READ_ACCESS | FILE_WRITE_ACCESS)

BOOLEAN PhantomIsTpmIdentIoctl(ULONG IoControlCode)
{
    return (IoControlCode == IOCTL_TPM_SUBMIT_COMMAND);
}

static NTSTATUS TpmIoctlCompletion(
    PDEVICE_OBJECT DeviceObject,
    PIRP Irp,
    PVOID Context
)
{
    const PHANTOM_TPM_PROFILE* profile;
    PUCHAR responseBuffer;
    ULONG responseLen;
    LARGE_INTEGER startTicks;

    UNREFERENCED_PARAMETER(DeviceObject);
    UNREFERENCED_PARAMETER(Context);

    if (!NT_SUCCESS(Irp->IoStatus.Status)) {
        goto done;
    }

    profile = PhantomGetTpmProfile();
    if (!profile) {
        goto done;
    }

    responseBuffer = (PUCHAR)Irp->AssociatedIrp.SystemBuffer;
    responseLen = (ULONG)Irp->IoStatus.Information;

    if (!responseBuffer || responseLen < 27) {
        goto done;
    }

    startTicks = KeQueryPerformanceCounter(NULL);

    PhantomRewriteTpmManufacturer(
        responseBuffer, responseLen, profile->ManufacturerId);

    PhantomTimingApplyDelay(IOCTL_TPM_SUBMIT_COMMAND, startTicks);

done:
    if (Irp->PendingReturned) {
        IoMarkIrpPending(Irp);
    }
    return STATUS_SUCCESS;
}

NTSTATUS PhantomInterceptTpmIoctl(
    PPHANTOM_FILTER_EXT Ext,
    PIRP Irp,
    PIO_STACK_LOCATION IrpSp
)
{
    PUCHAR inputBuffer;
    ULONG inputLen;

    UNREFERENCED_PARAMETER(IrpSp);

    inputBuffer = (PUCHAR)Irp->AssociatedIrp.SystemBuffer;
    inputLen = IrpSp->Parameters.DeviceIoControl.InputBufferLength;

    if (!PhantomIsTpmIdentityQuery(inputBuffer, inputLen)) {
        IoSkipCurrentIrpStackLocation(Irp);
        return IoCallDriver(Ext->LowerDevice, Irp);
    }

    IoCopyCurrentIrpStackLocationToNext(Irp);
    IoSetCompletionRoutine(
        Irp,
        TpmIoctlCompletion,
        Ext,
        TRUE,
        FALSE,
        FALSE
    );
    return IoCallDriver(Ext->LowerDevice, Irp);
}
