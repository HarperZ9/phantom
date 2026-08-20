/*
 * phantom.h — shared definitions for the Phantom kernel driver.
 */

#pragma once

#include <ntddk.h>
#include <wdm.h>

#define PHANTOM_DEVICE_NAME  L"\\Device\\PhantomSpoof"
#define PHANTOM_SYMLINK      L"\\DosDevices\\PhantomSpoof"

#define PHANTOM_POOL_TAG     'tnhP'

/* ---------- IOCTL codes for CLI <-> driver IPC ---------- */

#define PHANTOM_IOCTL_BASE   0x8000

#define IOCTL_PHANTOM_SET_PROFILE    CTL_CODE(PHANTOM_IOCTL_BASE, 0x800, METHOD_BUFFERED, FILE_WRITE_ACCESS)
#define IOCTL_PHANTOM_CLEAR_PROFILE  CTL_CODE(PHANTOM_IOCTL_BASE, 0x801, METHOD_BUFFERED, FILE_WRITE_ACCESS)
#define IOCTL_PHANTOM_GET_STATUS     CTL_CODE(PHANTOM_IOCTL_BASE, 0x802, METHOD_BUFFERED, FILE_READ_ACCESS)
#define IOCTL_PHANTOM_ATTACH_FILTER  CTL_CODE(PHANTOM_IOCTL_BASE, 0x803, METHOD_BUFFERED, FILE_WRITE_ACCESS)
#define IOCTL_PHANTOM_DETACH_FILTER  CTL_CODE(PHANTOM_IOCTL_BASE, 0x804, METHOD_BUFFERED, FILE_WRITE_ACCESS)
#define IOCTL_PHANTOM_CALIBRATE      CTL_CODE(PHANTOM_IOCTL_BASE, 0x805, METHOD_BUFFERED, FILE_READ_ACCESS)

/* ---------- Filter types ---------- */

typedef enum _PHANTOM_FILTER_TYPE {
    PHANTOM_FILTER_DISK = 0,
    PHANTOM_FILTER_NIC  = 1,
    PHANTOM_FILTER_GPU  = 2,
    PHANTOM_FILTER_TPM  = 3,
    PHANTOM_FILTER_EDID = 4,
    PHANTOM_FILTER_MAX
} PHANTOM_FILTER_TYPE;

/* ---------- Filter device extension ---------- */

typedef struct _PHANTOM_FILTER_EXT {
    PDEVICE_OBJECT       LowerDevice;
    PDEVICE_OBJECT       PhysicalDevice;
    PHANTOM_FILTER_TYPE  FilterType;
    ULONG                DeviceIndex;
    LARGE_INTEGER        BaselineLatencyTicks;
    LARGE_INTEGER        LatencyStdDevTicks;
} PHANTOM_FILTER_EXT, *PPHANTOM_FILTER_EXT;

/* ---------- Status structure returned to userland ---------- */

#pragma pack(push, 1)
typedef struct _PHANTOM_STATUS {
    ULONG   Version;
    BOOLEAN ProfileActive;
    ULONG   AttachedDiskCount;
    ULONG   AttachedNicCount;
    ULONG   AttachedGpuCount;
    ULONG   AttachedTpmCount;
    ULONG   AttachedEdidCount;
    ULONG   InterceptedIoctlCount;
    ULONG   TimingCalibratedCount;
} PHANTOM_STATUS;
#pragma pack(pop)

/* ---------- Function prototypes ---------- */

/* driver.c */
VOID PhantomDetachAllFilters(_In_ PDRIVER_OBJECT DriverObject);

/* control_ipc.c */
NTSTATUS PhantomHandleControlIoctl(PDEVICE_OBJECT DeviceObject, PIRP Irp);

/* disk_filter.c */
BOOLEAN  PhantomIsDiskIdentIoctl(ULONG IoControlCode);
NTSTATUS PhantomInterceptDiskIoctl(PPHANTOM_FILTER_EXT Ext, PIRP Irp, PIO_STACK_LOCATION IrpSp);

/* nic_filter.c */
BOOLEAN  PhantomIsNicIdentIoctl(ULONG IoControlCode);
NTSTATUS PhantomInterceptNicIoctl(PPHANTOM_FILTER_EXT Ext, PIRP Irp, PIO_STACK_LOCATION IrpSp);

/* gpu_filter.c */
BOOLEAN  PhantomIsGpuIdentIoctl(ULONG IoControlCode);
NTSTATUS PhantomInterceptGpuIoctl(PPHANTOM_FILTER_EXT Ext, PIRP Irp, PIO_STACK_LOCATION IrpSp);

/* tpm_filter.c */
BOOLEAN  PhantomIsTpmIdentIoctl(ULONG IoControlCode);
NTSTATUS PhantomInterceptTpmIoctl(PPHANTOM_FILTER_EXT Ext, PIRP Irp, PIO_STACK_LOCATION IrpSp);

/* edid_filter.c */
BOOLEAN  PhantomIsEdidIdentIoctl(ULONG IoControlCode);
NTSTATUS PhantomInterceptEdidIoctl(PPHANTOM_FILTER_EXT Ext, PIRP Irp, PIO_STACK_LOCATION IrpSp);
