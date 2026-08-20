/*
 * efi_main.c — Phantom DXE entry point.
 *
 * This UEFI DXE driver loads during early boot (before the OS) and
 * rewrites SMBIOS tables directly in firmware memory. Because the
 * modification happens at the physical memory level, Windows APIs
 * that read SMBIOS data (GetSystemFirmwareTable, MmMapIoSpace)
 * see the spoofed values natively — no kernel hooking required.
 *
 * Boot flow:
 *   1. UEFI firmware POST
 *   2. DXE phase begins, DXE drivers load
 *   3. PhantomDxeMain runs:
 *      a. Reads PHANTOM_SMBIOS_PROFILE from EFI variable
 *      b. Locates SMBIOS table via EFI configuration table
 *      c. Walks SMBIOS structures, rewrites string fields in-place
 *      d. Fixes entry point checksums
 *      e. Writes status back to EFI variable
 *   4. BDS phase, OS boot loader runs
 *   5. Windows reads spoofed SMBIOS from physical memory
 *
 * Installation: Copy PhantomDxe.efi to the EFI System Partition and
 * add it as a DXE driver via the firmware's driver load list, or
 * chain-load it from a UEFI shell script.
 *
 * Requires Secure Boot disabled (unsigned binary).
 */

#include <Uefi.h>
#include <Library/UefiBootServicesTableLib.h>
#include <Library/UefiRuntimeServicesTableLib.h>
#include <Library/UefiLib.h>
#include <Library/BaseMemoryLib.h>
#include <Library/DebugLib.h>

#include "smbios_rewrite.h"
#include "profile_efivar.h"

/*
 * Count how many SMBIOS structures of types 0-3 were modified.
 * Walk the table and count structures we handle.
 */
static UINT32
CountTargetStructures(
    IN UINT8    *TableBase,
    IN UINTN   TableLength
)
{
    SMBIOS_HEADER   *current;
    UINTN           tableEnd;
    UINT32          count = 0;
    CHAR8           *strArea;
    CHAR8           *ptr;

    tableEnd = (UINTN)TableBase + TableLength;
    current = (SMBIOS_HEADER *)TableBase;

    while (current != NULL && (UINTN)current < tableEnd) {
        if (current->Type == 127) {
            break;
        }

        if (current->Type <= 3) {
            count++;
        }

        /* Advance past this structure */
        strArea = (CHAR8 *)current + current->Length;
        ptr = strArea;

        while ((UINTN)ptr < tableEnd - 1) {
            if (ptr[0] == '\0' && ptr[1] == '\0') {
                ptr += 2;
                break;
            }
            ptr++;
        }

        if ((UINTN)ptr >= tableEnd) {
            break;
        }

        current = (SMBIOS_HEADER *)ptr;
    }

    return count;
}

/*
 * Locate the raw entry point pointer for checksum recalculation.
 * We need the original entry point, not just the table address.
 */
static VOID *
FindEntryPoint(
    IN EFI_SYSTEM_TABLE *SystemTable,
    OUT BOOLEAN         *Is64Bit
)
{
    UINTN i;

    static EFI_GUID smbios3Guid = {
        0xF2FD1544, 0x9794, 0x4A2C,
        { 0x99, 0x2E, 0xE5, 0xBB, 0xCF, 0x20, 0xE3, 0x94 }
    };
    static EFI_GUID smbiosGuid = {
        0xEB9D2D31, 0x2D88, 0x11D3,
        { 0x9A, 0x16, 0x00, 0x90, 0x27, 0x3F, 0xC1, 0x4D }
    };

    for (i = 0; i < SystemTable->NumberOfTableEntries; i++) {
        if (CompareMem(&SystemTable->ConfigurationTable[i].VendorGuid,
                       &smbios3Guid, sizeof(EFI_GUID)) == 0) {
            *Is64Bit = TRUE;
            return SystemTable->ConfigurationTable[i].VendorTable;
        }
    }

    for (i = 0; i < SystemTable->NumberOfTableEntries; i++) {
        if (CompareMem(&SystemTable->ConfigurationTable[i].VendorGuid,
                       &smbiosGuid, sizeof(EFI_GUID)) == 0) {
            *Is64Bit = FALSE;
            return SystemTable->ConfigurationTable[i].VendorTable;
        }
    }

    return NULL;
}

EFI_STATUS
EFIAPI
PhantomDxeMain(
    IN EFI_HANDLE        ImageHandle,
    IN EFI_SYSTEM_TABLE  *SystemTable
)
{
    EFI_STATUS              status;
    PHANTOM_SMBIOS_PROFILE  *profile = NULL;
    UINT8                   *tableAddress = NULL;
    UINTN                   tableLength = 0;
    UINT8                   smbiosMajor = 0;
    VOID                    *entryPoint = NULL;
    BOOLEAN                 is64Bit = FALSE;
    PHANTOM_DXE_STATUS      dxeStatus;

    SetMem(&dxeStatus, sizeof(dxeStatus), 0);

    /* Step 1: Read the profile from the EFI variable store */
    status = PhantomReadProfileVariable(&profile);
    if (EFI_ERROR(status)) {
        if (status == EFI_NOT_FOUND) {
            /* No profile set — nothing to do, this is normal */
            return EFI_SUCCESS;
        }
        dxeStatus.Status = PHANTOM_DXE_STATUS_ERROR;
        dxeStatus.LastErrorCode = (UINT32)status;
        PhantomWriteStatusVariable(&dxeStatus);
        return status;
    }

    /* Step 2: Locate the SMBIOS table in firmware memory */
    status = PhantomLocateSmbios(SystemTable, &tableAddress, &tableLength, &smbiosMajor);
    if (EFI_ERROR(status)) {
        dxeStatus.Status = PHANTOM_DXE_STATUS_ERROR;
        dxeStatus.LastErrorCode = (UINT32)status;
        PhantomWriteStatusVariable(&dxeStatus);
        gBS->FreePool(profile);
        return status;
    }

    /* Step 3: Count target structures for status reporting */
    dxeStatus.TablesModified = CountTargetStructures(tableAddress, tableLength);

    /* Step 4: Rewrite SMBIOS strings and UUID in-place */
    status = PhantomRewriteSmbiosTables(tableAddress, tableLength, profile);
    if (EFI_ERROR(status)) {
        dxeStatus.Status = PHANTOM_DXE_STATUS_ERROR;
        dxeStatus.LastErrorCode = (UINT32)status;
        PhantomWriteStatusVariable(&dxeStatus);
        gBS->FreePool(profile);
        return status;
    }

    /* Step 5: Fix SMBIOS entry point checksums */
    entryPoint = FindEntryPoint(SystemTable, &is64Bit);
    if (entryPoint != NULL) {
        PhantomFixSmbiosChecksums(entryPoint, is64Bit);
    }

    /* Step 6: Report success */
    dxeStatus.Status = PHANTOM_DXE_STATUS_APPLIED;
    PhantomWriteStatusVariable(&dxeStatus);

    gBS->FreePool(profile);
    return EFI_SUCCESS;
}
