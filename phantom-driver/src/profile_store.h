/*
 * profile_store.h — kernel-side storage for the active hardware identity profile.
 *
 * Profile data is received from userland as a packed binary blob via IOCTL,
 * stored in non-paged pool, and accessed lock-free via interlocked pointer swap.
 */

#pragma once

#include "phantom.h"

#define PHANTOM_MAX_SERIAL_LEN   64
#define PHANTOM_MAX_MODEL_LEN    128
#define PHANTOM_MAX_MAC_LEN      6
#define PHANTOM_MAX_DISKS        8
#define PHANTOM_MAX_NICS         4
#define PHANTOM_MAX_GPUS         2
#define PHANTOM_MAX_DISPLAYS     4

#pragma pack(push, 1)

typedef struct _PHANTOM_DISK_PROFILE {
    ULONG Index;
    CHAR  Serial[PHANTOM_MAX_SERIAL_LEN];
    ULONG SerialLength;
    CHAR  Model[PHANTOM_MAX_MODEL_LEN];
    ULONG ModelLength;
    CHAR  FirmwareRev[PHANTOM_MAX_SERIAL_LEN];
    ULONG FirmwareRevLength;
} PHANTOM_DISK_PROFILE;

typedef struct _PHANTOM_NIC_PROFILE {
    UCHAR PermanentMac[PHANTOM_MAX_MAC_LEN];
    UCHAR CurrentMac[PHANTOM_MAX_MAC_LEN];
} PHANTOM_NIC_PROFILE;

typedef struct _PHANTOM_GPU_PROFILE {
    USHORT VendorId;
    USHORT DeviceId;
    ULONG  SubsystemId;
    CHAR   PnpInstanceId[PHANTOM_MAX_MODEL_LEN];
    ULONG  PnpInstanceIdLength;
} PHANTOM_GPU_PROFILE;

typedef struct _PHANTOM_TPM_PROFILE {
    CHAR  ManufacturerId[4];
} PHANTOM_TPM_PROFILE;

typedef struct _PHANTOM_DISPLAY_PROFILE {
    CHAR   ManufacturerCode[3];
    UCHAR  Padding;
    USHORT ProductCode;
    ULONG  SerialNumber;
} PHANTOM_DISPLAY_PROFILE;

typedef struct _PHANTOM_KERNEL_PROFILE {
    ULONG Magic;               /* 'PHNT' */
    ULONG Version;
    ULONG DiskCount;
    ULONG NicCount;
    ULONG GpuCount;
    ULONG HasTpm;
    ULONG DisplayCount;
    PHANTOM_DISK_PROFILE    Disks[PHANTOM_MAX_DISKS];
    PHANTOM_NIC_PROFILE     Nics[PHANTOM_MAX_NICS];
    PHANTOM_GPU_PROFILE     Gpus[PHANTOM_MAX_GPUS];
    PHANTOM_TPM_PROFILE     Tpm;
    PHANTOM_DISPLAY_PROFILE Displays[PHANTOM_MAX_DISPLAYS];
} PHANTOM_KERNEL_PROFILE;

#pragma pack(pop)

#define PHANTOM_PROFILE_MAGIC  0x544E4850  /* 'PHNT' */

/* ---------- API ---------- */

VOID     PhantomProfileStoreInit(VOID);
VOID     PhantomProfileStoreCleanup(VOID);

NTSTATUS PhantomProfileStoreSet(_In_reads_bytes_(Length) PVOID Buffer, _In_ ULONG Length);
VOID     PhantomProfileStoreClear(VOID);
BOOLEAN  PhantomProfileIsActive(VOID);

const PHANTOM_DISK_PROFILE*    PhantomGetDiskProfile(_In_ ULONG Index);
const PHANTOM_NIC_PROFILE*     PhantomGetNicProfile(_In_ ULONG Index);
const PHANTOM_GPU_PROFILE*     PhantomGetGpuProfile(_In_ ULONG Index);
const PHANTOM_TPM_PROFILE*     PhantomGetTpmProfile(VOID);
const PHANTOM_DISPLAY_PROFILE* PhantomGetDisplayProfile(_In_ ULONG Index);
