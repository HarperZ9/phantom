/*
 * edid_filter.c — display EDID interception.
 *
 * Intercepts monitor EDID data returned via the display miniport driver
 * to replace manufacturer code, product code, and serial number fields
 * in the 128-byte EDID block.
 *
 * EDID layout (bytes):
 *   8-9:   Manufacturer ID (compressed 3-letter code)
 *   10-11: Product code (little-endian u16)
 *   12-15: Serial number (little-endian u32)
 *
 * Status: Structural implementation. EDID interception requires hooking
 * the display miniport's DxgkDdiQueryDeviceDescriptor callback, which
 * varies by GPU vendor driver. Full implementation deferred.
 */

#include "phantom.h"
#include "profile_store.h"

/* EDID byte offsets */
#define EDID_MANUFACTURER_OFFSET    8
#define EDID_PRODUCT_CODE_OFFSET   10
#define EDID_SERIAL_OFFSET         12

/*
 * Encode a 3-letter EDID manufacturer ID into 2 bytes.
 * Each letter is encoded as (letter - '@') in 5 bits.
 * Byte layout: [0bX AAAAA BB] [0bBBB CCCCC]
 */
static VOID EncodeEdidManufacturer(
    const CHAR ManufacturerCode[3],
    UCHAR Out[2]
)
{
    UCHAR a = (UCHAR)(ManufacturerCode[0] - '@') & 0x1F;
    UCHAR b = (UCHAR)(ManufacturerCode[1] - '@') & 0x1F;
    UCHAR c = (UCHAR)(ManufacturerCode[2] - '@') & 0x1F;

    Out[0] = (UCHAR)((a << 2) | (b >> 3));
    Out[1] = (UCHAR)(((b & 0x07) << 5) | c);
}

/*
 * Rewrite EDID fields in a raw 128-byte EDID block.
 * Called from the display miniport interception path.
 */
VOID PhantomRewriteEdidBlock(
    PUCHAR EdidBlock,
    ULONG BlockLength,
    const CHAR ManufacturerCode[3],
    USHORT ProductCode,
    ULONG SerialNumber
)
{
    UCHAR checksum;
    ULONG i;

    if (BlockLength < 128) {
        return;
    }

    EncodeEdidManufacturer(ManufacturerCode, &EdidBlock[EDID_MANUFACTURER_OFFSET]);

    EdidBlock[EDID_PRODUCT_CODE_OFFSET]     = (UCHAR)(ProductCode & 0xFF);
    EdidBlock[EDID_PRODUCT_CODE_OFFSET + 1] = (UCHAR)(ProductCode >> 8);

    EdidBlock[EDID_SERIAL_OFFSET]     = (UCHAR)(SerialNumber & 0xFF);
    EdidBlock[EDID_SERIAL_OFFSET + 1] = (UCHAR)((SerialNumber >> 8) & 0xFF);
    EdidBlock[EDID_SERIAL_OFFSET + 2] = (UCHAR)((SerialNumber >> 16) & 0xFF);
    EdidBlock[EDID_SERIAL_OFFSET + 3] = (UCHAR)((SerialNumber >> 24) & 0xFF);

    /* Recalculate EDID checksum (sum of all 128 bytes must be 0 mod 256) */
    checksum = 0;
    for (i = 0; i < 127; i++) {
        checksum += EdidBlock[i];
    }
    EdidBlock[127] = (UCHAR)(256 - checksum);
}

/*
 * Monitor EDID query IOCTL.
 * The monitor port driver returns EDID blocks via IOCTL_VIDEO_GET_CHILD_STATE
 * and related video IOCTLs. The exact IOCTL varies by GPU vendor driver, but
 * a common path uses FILE_DEVICE_VIDEO (0x23) IOCTLs.
 */
#define FILE_DEVICE_VIDEO 0x00000023
#define IOCTL_VIDEO_QUERY_DISPLAY_BRIGHTNESS \
    CTL_CODE(FILE_DEVICE_VIDEO, 0x126, METHOD_BUFFERED, FILE_ANY_ACCESS)

BOOLEAN PhantomIsEdidIdentIoctl(ULONG IoControlCode)
{
    return (IoControlCode == IOCTL_VIDEO_QUERY_DISPLAY_BRIGHTNESS);
}

static NTSTATUS EdidIoctlCompletion(
    PDEVICE_OBJECT DeviceObject,
    PIRP Irp,
    PVOID Context
)
{
    PPHANTOM_FILTER_EXT ext = (PPHANTOM_FILTER_EXT)Context;
    const PHANTOM_DISPLAY_PROFILE* profile;
    PUCHAR buffer;
    ULONG bufferLen;
    LARGE_INTEGER startTicks;

    UNREFERENCED_PARAMETER(DeviceObject);

    if (!NT_SUCCESS(Irp->IoStatus.Status)) {
        goto done;
    }

    profile = PhantomGetDisplayProfile(ext->DeviceIndex);
    if (!profile) {
        goto done;
    }

    buffer = (PUCHAR)Irp->AssociatedIrp.SystemBuffer;
    bufferLen = (ULONG)Irp->IoStatus.Information;

    if (!buffer || bufferLen < 128) {
        goto done;
    }

    startTicks = KeQueryPerformanceCounter(NULL);

    PhantomRewriteEdidBlock(
        buffer,
        bufferLen,
        profile->ManufacturerCode,
        profile->ProductCode,
        profile->SerialNumber
    );

    PhantomTimingApplyDelay(IOCTL_VIDEO_QUERY_DISPLAY_BRIGHTNESS, startTicks);

done:
    if (Irp->PendingReturned) {
        IoMarkIrpPending(Irp);
    }
    return STATUS_SUCCESS;
}

NTSTATUS PhantomInterceptEdidIoctl(
    PPHANTOM_FILTER_EXT Ext,
    PIRP Irp,
    PIO_STACK_LOCATION IrpSp
)
{
    UNREFERENCED_PARAMETER(IrpSp);

    IoCopyCurrentIrpStackLocationToNext(Irp);
    IoSetCompletionRoutine(
        Irp,
        EdidIoctlCompletion,
        Ext,
        TRUE,
        FALSE,
        FALSE
    );
    return IoCallDriver(Ext->LowerDevice, Irp);
}
