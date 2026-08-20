/*
 * profile_efivar.h — EFI variable interface for the Phantom SMBIOS profile.
 *
 * The CLI writes a serialized PHANTOM_SMBIOS_PROFILE to the EFI variable
 * "PhantomProfile" under the Phantom vendor GUID. The DXE module reads
 * it on boot and applies the contained SMBIOS overrides.
 *
 * Variable lifecycle:
 *   - Created by:  phantom apply <profile> --layers 0  (from Windows)
 *   - Read by:     PhantomDxe.efi during DXE phase
 *   - Deleted by:  phantom revert --layers 0  (from Windows)
 */

#ifndef PHANTOM_PROFILE_EFIVAR_H
#define PHANTOM_PROFILE_EFIVAR_H

#include <Uefi.h>
#include "smbios_rewrite.h"

/* Phantom UEFI vendor GUID: {7B3E8A1C-4F2D-49A5-B1C6-8D0F3E5A72B9} */
#define PHANTOM_VENDOR_GUID \
    { 0x7B3E8A1C, 0x4F2D, 0x49A5, \
      { 0xB1, 0xC6, 0x8D, 0x0F, 0x3E, 0x5A, 0x72, 0xB9 } }

#define PHANTOM_PROFILE_VAR_NAME  L"PhantomProfile"
#define PHANTOM_STATUS_VAR_NAME   L"PhantomStatus"

/* Status values written back to indicate DXE module ran */
#define PHANTOM_DXE_STATUS_IDLE     0
#define PHANTOM_DXE_STATUS_APPLIED  1
#define PHANTOM_DXE_STATUS_ERROR    2

typedef struct {
    UINT32  Status;
    UINT32  TablesModified;
    UINT32  StringsReplaced;
    UINT32  LastErrorCode;
} PHANTOM_DXE_STATUS;

/*
 * Read the Phantom SMBIOS profile from the EFI variable store.
 * Allocates the profile using AllocatePool; caller must FreePool.
 */
EFI_STATUS
PhantomReadProfileVariable(
    OUT PHANTOM_SMBIOS_PROFILE  **Profile
);

/*
 * Write the DXE module status back to an EFI variable so the
 * CLI can report whether the firmware layer applied successfully.
 */
EFI_STATUS
PhantomWriteStatusVariable(
    IN PHANTOM_DXE_STATUS   *Status
);

/*
 * Delete the profile variable after successful application
 * (optional — can be left for re-application on next boot).
 */
EFI_STATUS
PhantomDeleteProfileVariable(
    VOID
);

#endif /* PHANTOM_PROFILE_EFIVAR_H */
