/*
 * disk_filter.c — disk serial/model interception.
 *
 * Intercepts two IOCTL paths used to read disk identifiers:
 *
 *   1. IOCTL_ATA_PASS_THROUGH (ATA IDENTIFY DEVICE) — returns the raw ATA
 *      identity page containing serial number at words 10-19 and model
 *      number at words 27-46.
 *
 *   2. IOCTL_STORAGE_QUERY_PROPERTY (StorageDeviceProperty) — returns a
 *      STORAGE_DEVICE_DESCRIPTOR with serial number and product ID offsets.
 *
 * Both paths must return identical values or the inconsistency is detectable.
 * Profile data comes from the kernel profile store; timing normalization
 * ensures response latency matches calibrated baselines.
 */

#include "phantom.h"
#include "profile_store.h"
#include "timing.h"

#include <ntddstor.h>
#include <ntddscsi.h>

/* ATA IDENTIFY DEVICE command */
#define ATA_CMD_IDENTIFY_DEVICE  0xEC

/* Offsets into the 512-byte ATA identify buffer (in 16-bit words) */
#define ATA_IDENT_SERIAL_WORD_START  10
#define ATA_IDENT_SERIAL_WORD_COUNT  10   /* 20 bytes */
#define ATA_IDENT_MODEL_WORD_START   27
#define ATA_IDENT_MODEL_WORD_COUNT   20   /* 40 bytes */
#define ATA_IDENT_FWREV_WORD_START   23
#define ATA_IDENT_FWREV_WORD_COUNT   4    /* 8 bytes  */

BOOLEAN PhantomIsDiskIdentIoctl(ULONG IoControlCode)
{
    return (IoControlCode == IOCTL_STORAGE_QUERY_PROPERTY ||
            IoControlCode == IOCTL_ATA_PASS_THROUGH ||
            IoControlCode == IOCTL_ATA_PASS_THROUGH_DIRECT);
}

/*
 * Completion routine for disk IOCTLs — rewrites identifier fields
 * in the response buffer after the real driver has filled it.
 */
static NTSTATUS DiskIoctlCompletion(
    PDEVICE_OBJECT DeviceObject,
    PIRP Irp,
    PVOID Context
)
{
    PPHANTOM_FILTER_EXT ext = (PPHANTOM_FILTER_EXT)Context;
    PIO_STACK_LOCATION irpSp = IoGetCurrentIrpStackLocation(Irp);
    ULONG ioctl = irpSp->Parameters.DeviceIoControl.IoControlCode;
    const PHANTOM_DISK_PROFILE* profile;
    LARGE_INTEGER startTicks;

    UNREFERENCED_PARAMETER(DeviceObject);

    if (!NT_SUCCESS(Irp->IoStatus.Status)) {
        goto done;
    }

    profile = PhantomGetDiskProfile(ext->DeviceIndex);
    if (!profile) {
        goto done;
    }

    startTicks = KeQueryPerformanceCounter(NULL);

    if (ioctl == IOCTL_STORAGE_QUERY_PROPERTY) {
        RewriteStorageDescriptor(Irp, profile);
    } else if (ioctl == IOCTL_ATA_PASS_THROUGH || ioctl == IOCTL_ATA_PASS_THROUGH_DIRECT) {
        RewriteAtaIdentify(Irp, profile);
    }

    PhantomTimingApplyDelay(ioctl, startTicks);

done:
    if (Irp->PendingReturned) {
        IoMarkIrpPending(Irp);
    }
    return STATUS_SUCCESS;
}

NTSTATUS PhantomInterceptDiskIoctl(
    PPHANTOM_FILTER_EXT Ext,
    PIRP Irp,
    PIO_STACK_LOCATION IrpSp
)
{
    UNREFERENCED_PARAMETER(IrpSp);

    IoCopyCurrentIrpStackLocationToNext(Irp);
    IoSetCompletionRoutine(
        Irp,
        DiskIoctlCompletion,
        Ext,
        TRUE,   /* on success */
        FALSE,  /* on error   */
        FALSE   /* on cancel  */
    );
    return IoCallDriver(Ext->LowerDevice, Irp);
}

/*
 * Rewrite serial, model, and firmware revision in the ATA IDENTIFY buffer.
 * ATA strings are byte-swapped (high byte first in each word).
 */
static VOID RewriteAtaIdentify(PIRP Irp, const PHANTOM_DISK_PROFILE* Profile)
{
    PUCHAR buffer;
    ULONG bufferLen;
    ATA_PASS_THROUGH_EX* ata;
    PUSHORT identWords;

    buffer = (PUCHAR)Irp->AssociatedIrp.SystemBuffer;
    bufferLen = (ULONG)Irp->IoStatus.Information;

    if (!buffer || bufferLen < sizeof(ATA_PASS_THROUGH_EX) + 512) {
        return;
    }

    ata = (ATA_PASS_THROUGH_EX*)buffer;

    if (ata->DataBufferOffset == 0 ||
        ata->DataBufferOffset + 512 > bufferLen) {
        return;
    }

    identWords = (PUSHORT)(buffer + ata->DataBufferOffset);

    WriteAtaString(
        identWords + ATA_IDENT_SERIAL_WORD_START,
        ATA_IDENT_SERIAL_WORD_COUNT,
        Profile->Serial,
        Profile->SerialLength
    );

    WriteAtaString(
        identWords + ATA_IDENT_MODEL_WORD_START,
        ATA_IDENT_MODEL_WORD_COUNT,
        Profile->Model,
        Profile->ModelLength
    );

    WriteAtaString(
        identWords + ATA_IDENT_FWREV_WORD_START,
        ATA_IDENT_FWREV_WORD_COUNT,
        Profile->FirmwareRev,
        Profile->FirmwareRevLength
    );
}

/*
 * Write an ATA-format string (byte-swapped within each 16-bit word,
 * space-padded to fill the field).
 */
static VOID WriteAtaString(
    PUSHORT Dest,
    ULONG WordCount,
    const CHAR* Source,
    ULONG SourceLen
)
{
    ULONG totalBytes = WordCount * 2;
    UCHAR padded[128];
    ULONG i;

    if (totalBytes > sizeof(padded)) {
        totalBytes = sizeof(padded);
    }

    RtlFillMemory(padded, totalBytes, ' ');

    for (i = 0; i < SourceLen && i < totalBytes; i++) {
        padded[i] = (UCHAR)Source[i];
    }

    /* Byte-swap within each word */
    for (i = 0; i < totalBytes; i += 2) {
        UCHAR hi = padded[i];
        UCHAR lo = (i + 1 < totalBytes) ? padded[i + 1] : ' ';
        Dest[i / 2] = (USHORT)((lo << 8) | hi);
    }
}

/*
 * Rewrite serial and product ID in a STORAGE_DEVICE_DESCRIPTOR response.
 */
static VOID RewriteStorageDescriptor(PIRP Irp, const PHANTOM_DISK_PROFILE* Profile)
{
    PSTORAGE_DEVICE_DESCRIPTOR desc;
    PUCHAR buffer;
    ULONG bufferLen;

    buffer = (PUCHAR)Irp->AssociatedIrp.SystemBuffer;
    bufferLen = (ULONG)Irp->IoStatus.Information;

    if (!buffer || bufferLen < sizeof(STORAGE_DEVICE_DESCRIPTOR)) {
        return;
    }

    desc = (PSTORAGE_DEVICE_DESCRIPTOR)buffer;

    if (desc->SerialNumberOffset > 0 &&
        desc->SerialNumberOffset < bufferLen)
    {
        PCHAR dest = (PCHAR)(buffer + desc->SerialNumberOffset);
        ULONG maxLen = bufferLen - desc->SerialNumberOffset;
        ULONG copyLen = min(Profile->SerialLength, maxLen - 1);

        RtlCopyMemory(dest, Profile->Serial, copyLen);
        dest[copyLen] = '\0';
    }

    if (desc->ProductIdOffset > 0 &&
        desc->ProductIdOffset < bufferLen)
    {
        PCHAR dest = (PCHAR)(buffer + desc->ProductIdOffset);
        ULONG maxLen = bufferLen - desc->ProductIdOffset;
        ULONG copyLen = min(Profile->ModelLength, maxLen - 1);

        RtlCopyMemory(dest, Profile->Model, copyLen);
        dest[copyLen] = '\0';
    }

    if (desc->ProductRevisionOffset > 0 &&
        desc->ProductRevisionOffset < bufferLen)
    {
        PCHAR dest = (PCHAR)(buffer + desc->ProductRevisionOffset);
        ULONG maxLen = bufferLen - desc->ProductRevisionOffset;
        ULONG copyLen = min(Profile->FirmwareRevLength, maxLen - 1);

        RtlCopyMemory(dest, Profile->FirmwareRev, copyLen);
        dest[copyLen] = '\0';
    }
}
