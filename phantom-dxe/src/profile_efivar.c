/*
 * profile_efivar.c — Read/write Phantom profile and status EFI variables.
 *
 * Uses UEFI Runtime Services GetVariable/SetVariable to exchange data
 * between the OS-level CLI and the DXE boot module.
 *
 * The profile variable is non-volatile (NV) + boot-service-access (BS)
 * + runtime-access (RT) so it persists across reboots and is accessible
 * from both the DXE phase and from Windows via SetFirmwareEnvironmentVariable.
 *
 * The status variable is NV + BS + RT so the CLI can read back whether
 * the DXE module applied the profile successfully on the last boot.
 */

#include "profile_efivar.h"

#include <Library/UefiRuntimeServicesTableLib.h>
#include <Library/UefiBootServicesTableLib.h>
#include <Library/BaseMemoryLib.h>

static EFI_GUID gPhantomVendorGuid = PHANTOM_VENDOR_GUID;

EFI_STATUS
PhantomReadProfileVariable(
    OUT PHANTOM_SMBIOS_PROFILE  **Profile
)
{
    EFI_STATUS  status;
    UINTN       dataSize;
    UINT8       *buffer;
    PHANTOM_SMBIOS_PROFILE *profile;

    if (Profile == NULL) {
        return EFI_INVALID_PARAMETER;
    }

    *Profile = NULL;

    /* Query the variable size */
    dataSize = 0;
    status = gRT->GetVariable(
        PHANTOM_PROFILE_VAR_NAME,
        &gPhantomVendorGuid,
        NULL,
        &dataSize,
        NULL
    );

    if (status != EFI_BUFFER_TOO_SMALL) {
        return EFI_NOT_FOUND;
    }

    if (dataSize < sizeof(PHANTOM_SMBIOS_PROFILE)) {
        return EFI_COMPROMISED_DATA;
    }

    /* Allocate and read */
    status = gBS->AllocatePool(EfiBootServicesData, dataSize, (VOID **)&buffer);
    if (EFI_ERROR(status)) {
        return status;
    }

    status = gRT->GetVariable(
        PHANTOM_PROFILE_VAR_NAME,
        &gPhantomVendorGuid,
        NULL,
        &dataSize,
        buffer
    );

    if (EFI_ERROR(status)) {
        gBS->FreePool(buffer);
        return status;
    }

    profile = (PHANTOM_SMBIOS_PROFILE *)buffer;

    /* Validate magic and version */
    if (profile->Magic != PHANTOM_SMBIOS_MAGIC) {
        gBS->FreePool(buffer);
        return EFI_COMPROMISED_DATA;
    }

    if (profile->Version != PHANTOM_SMBIOS_VERSION) {
        gBS->FreePool(buffer);
        return EFI_INCOMPATIBLE_VERSION;
    }

    *Profile = profile;
    return EFI_SUCCESS;
}

EFI_STATUS
PhantomWriteStatusVariable(
    IN PHANTOM_DXE_STATUS   *Status
)
{
    if (Status == NULL) {
        return EFI_INVALID_PARAMETER;
    }

    return gRT->SetVariable(
        PHANTOM_STATUS_VAR_NAME,
        &gPhantomVendorGuid,
        EFI_VARIABLE_NON_VOLATILE |
        EFI_VARIABLE_BOOTSERVICE_ACCESS |
        EFI_VARIABLE_RUNTIME_ACCESS,
        sizeof(PHANTOM_DXE_STATUS),
        Status
    );
}

EFI_STATUS
PhantomDeleteProfileVariable(
    VOID
)
{
    /* Setting DataSize to 0 deletes the variable */
    return gRT->SetVariable(
        PHANTOM_PROFILE_VAR_NAME,
        &gPhantomVendorGuid,
        0,
        0,
        NULL
    );
}
