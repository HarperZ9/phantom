/*
 * smbios_rewrite.c — SMBIOS table walking and string rewriting.
 *
 * Locates the SMBIOS table via EFI configuration tables, walks the
 * structure chain, and replaces string fields in-place. Strings are
 * replaced only when the new value fits within the existing allocation
 * (remainder is null-padded). This avoids reallocating or relocating
 * the SMBIOS table, which would require updating the entry point
 * address — a much more invasive operation.
 *
 * UUID in Type 1 is a 16-byte binary field, not a string, so it's
 * written directly from the profile.
 */

#include "smbios_rewrite.h"

#include <Library/BaseMemoryLib.h>
#include <Library/DebugLib.h>

/* SMBIOS GUIDs from the UEFI spec */
static EFI_GUID gSmbiosTableGuid = {
    0xEB9D2D31, 0x2D88, 0x11D3,
    { 0x9A, 0x16, 0x00, 0x90, 0x27, 0x3F, 0xC1, 0x4D }
};

static EFI_GUID gSmbios3TableGuid = {
    0xF2FD1544, 0x9794, 0x4A2C,
    { 0x99, 0x2E, 0xE5, 0xBB, 0xCF, 0x20, 0xE3, 0x94 }
};

/*
 * Get the start of the unformatted (string) area for a structure.
 * It begins immediately after the formatted area (at Header.Length offset).
 */
static CHAR8 *
GetStringArea(
    IN SMBIOS_HEADER *Header
)
{
    return (CHAR8 *)Header + Header->Length;
}

/*
 * Compute the total size of an SMBIOS structure including its
 * string area and the double-null terminator.
 */
static UINTN
GetStructureSize(
    IN SMBIOS_HEADER    *Header,
    IN UINTN            TableEnd
)
{
    CHAR8   *strArea = GetStringArea(Header);
    CHAR8   *ptr = strArea;
    CHAR8   *end = (CHAR8 *)TableEnd;

    if (ptr >= end) {
        return Header->Length + 2;
    }

    /* Walk past all strings until we hit a double-null */
    while (ptr < end - 1) {
        if (ptr[0] == '\0' && ptr[1] == '\0') {
            return (UINTN)((UINT8 *)(ptr + 2) - (UINT8 *)Header);
        }
        ptr++;
    }

    /* Malformed — no double-null found, return to end */
    return (UINTN)(end - (CHAR8 *)Header);
}

/*
 * Find the Nth string (1-based) in a structure's unformatted area.
 * Returns pointer to the string start and its length (excluding null).
 */
static CHAR8 *
FindString(
    IN  SMBIOS_HEADER   *Header,
    IN  UINTN           TableEnd,
    IN  UINT8           StringIndex,
    OUT UINTN           *StringLength
)
{
    CHAR8   *ptr;
    CHAR8   *end;
    UINT8   current;

    if (StringIndex == 0) {
        *StringLength = 0;
        return NULL;
    }

    ptr = GetStringArea(Header);
    end = (CHAR8 *)TableEnd;
    current = 1;

    while (ptr < end) {
        CHAR8 *strStart = ptr;

        /* Find end of current string */
        while (ptr < end && *ptr != '\0') {
            ptr++;
        }

        if (current == StringIndex) {
            *StringLength = (UINTN)(ptr - strStart);
            return strStart;
        }

        current++;
        ptr++; /* skip null terminator */

        /* Check for double-null (end of string area) */
        if (ptr < end && *ptr == '\0') {
            break;
        }
    }

    *StringLength = 0;
    return NULL;
}

static UINTN
AsciiStrLen(
    IN CONST CHAR8 *Str
)
{
    UINTN len = 0;
    while (Str[len] != '\0') {
        len++;
    }
    return len;
}

EFI_STATUS
PhantomReplaceString(
    IN  SMBIOS_HEADER   *Structure,
    IN  UINTN           TableEnd,
    IN  UINT8           StringIndex,
    IN  CONST CHAR8     *NewString
)
{
    CHAR8   *existing;
    UINTN   existingLen;
    UINTN   newLen;

    if (StringIndex == 0 || NewString == NULL || NewString[0] == '\0') {
        return EFI_INVALID_PARAMETER;
    }

    existing = FindString(Structure, TableEnd, StringIndex, &existingLen);
    if (existing == NULL) {
        return EFI_NOT_FOUND;
    }

    newLen = AsciiStrLen(NewString);

    if (newLen > existingLen) {
        /* Can't grow — truncate to fit the existing allocation */
        newLen = existingLen;
    }

    /* Copy new string */
    CopyMem(existing, NewString, newLen);

    /* Null-pad the remainder */
    if (newLen < existingLen) {
        SetMem(existing + newLen, existingLen - newLen, 0);
        /* Restore the null terminator at the original position
         * to keep the structure valid for other string lookups */
        existing[existingLen] = '\0';
    }

    return EFI_SUCCESS;
}

/*
 * Advance past one SMBIOS structure to the next.
 */
static SMBIOS_HEADER *
NextStructure(
    IN SMBIOS_HEADER    *Current,
    IN UINTN            TableEnd
)
{
    UINTN size = GetStructureSize(Current, TableEnd);
    UINT8 *next = (UINT8 *)Current + size;

    if ((UINTN)next >= TableEnd) {
        return NULL;
    }

    return (SMBIOS_HEADER *)next;
}

static VOID
RewriteType0(
    IN SMBIOS_HEADER            *Header,
    IN UINTN                    TableEnd,
    IN PHANTOM_SMBIOS_PROFILE   *Profile
)
{
    SMBIOS_TYPE0 *t0 = (SMBIOS_TYPE0 *)Header;

    if (Profile->BiosVendor[0] != '\0') {
        PhantomReplaceString(Header, TableEnd, t0->Vendor, Profile->BiosVendor);
    }
    if (Profile->BiosVersion[0] != '\0') {
        PhantomReplaceString(Header, TableEnd, t0->BiosVersion, Profile->BiosVersion);
    }
}

static VOID
RewriteType1(
    IN SMBIOS_HEADER            *Header,
    IN UINTN                    TableEnd,
    IN PHANTOM_SMBIOS_PROFILE   *Profile
)
{
    SMBIOS_TYPE1 *t1 = (SMBIOS_TYPE1 *)Header;

    if (Header->Length < sizeof(SMBIOS_TYPE1)) {
        return;
    }

    if (Profile->SystemManufacturer[0] != '\0') {
        PhantomReplaceString(Header, TableEnd, t1->Manufacturer, Profile->SystemManufacturer);
    }
    if (Profile->SystemProduct[0] != '\0') {
        PhantomReplaceString(Header, TableEnd, t1->ProductName, Profile->SystemProduct);
    }
    if (Profile->SystemSerial[0] != '\0') {
        PhantomReplaceString(Header, TableEnd, t1->SerialNumber, Profile->SystemSerial);
    }

    /* UUID is raw binary in the formatted area, not a string reference */
    {
        UINT8 zeroUuid[16];
        SetMem(zeroUuid, 16, 0);
        if (CompareMem(Profile->SystemUuid, zeroUuid, 16) != 0) {
            CopyMem(t1->Uuid, Profile->SystemUuid, 16);
        }
    }
}

static VOID
RewriteType2(
    IN SMBIOS_HEADER            *Header,
    IN UINTN                    TableEnd,
    IN PHANTOM_SMBIOS_PROFILE   *Profile
)
{
    SMBIOS_TYPE2 *t2 = (SMBIOS_TYPE2 *)Header;

    if (Profile->BoardManufacturer[0] != '\0') {
        PhantomReplaceString(Header, TableEnd, t2->Manufacturer, Profile->BoardManufacturer);
    }
    if (Profile->BoardProduct[0] != '\0') {
        PhantomReplaceString(Header, TableEnd, t2->Product, Profile->BoardProduct);
    }
    if (Profile->BoardSerial[0] != '\0') {
        PhantomReplaceString(Header, TableEnd, t2->SerialNumber, Profile->BoardSerial);
    }
}

static VOID
RewriteType3(
    IN SMBIOS_HEADER            *Header,
    IN UINTN                    TableEnd,
    IN PHANTOM_SMBIOS_PROFILE   *Profile
)
{
    SMBIOS_TYPE3 *t3 = (SMBIOS_TYPE3 *)Header;

    if (Profile->ChassisSerial[0] != '\0') {
        PhantomReplaceString(Header, TableEnd, t3->SerialNumber, Profile->ChassisSerial);
    }
    if (Profile->ChassisAssetTag[0] != '\0') {
        PhantomReplaceString(Header, TableEnd, t3->AssetTag, Profile->ChassisAssetTag);
    }
}

EFI_STATUS
PhantomLocateSmbios(
    IN  EFI_SYSTEM_TABLE    *SystemTable,
    OUT UINT8               **TableAddress,
    OUT UINTN               *TableLength,
    OUT UINT8               *MajorVersion
)
{
    UINTN   i;

    /* Try SMBIOS 3.x (64-bit) first */
    for (i = 0; i < SystemTable->NumberOfTableEntries; i++) {
        EFI_CONFIGURATION_TABLE *entry = &SystemTable->ConfigurationTable[i];

        if (CompareMem(&entry->VendorGuid, &gSmbios3TableGuid, sizeof(EFI_GUID)) == 0) {
            SMBIOS3_ENTRY_POINT *ep3 = (SMBIOS3_ENTRY_POINT *)entry->VendorTable;

            if (ep3->AnchorString[0] == '_' && ep3->AnchorString[1] == 'S' &&
                ep3->AnchorString[2] == 'M' && ep3->AnchorString[3] == '3' &&
                ep3->AnchorString[4] == '_')
            {
                *TableAddress = (UINT8 *)(UINTN)ep3->TableAddress;
                *TableLength = (UINTN)ep3->TableMaximumSize;
                *MajorVersion = ep3->MajorVersion;
                return EFI_SUCCESS;
            }
        }
    }

    /* Fall back to SMBIOS 2.x (32-bit) */
    for (i = 0; i < SystemTable->NumberOfTableEntries; i++) {
        EFI_CONFIGURATION_TABLE *entry = &SystemTable->ConfigurationTable[i];

        if (CompareMem(&entry->VendorGuid, &gSmbiosTableGuid, sizeof(EFI_GUID)) == 0) {
            SMBIOS_ENTRY_POINT *ep = (SMBIOS_ENTRY_POINT *)entry->VendorTable;

            if (ep->AnchorString[0] == '_' && ep->AnchorString[1] == 'S' &&
                ep->AnchorString[2] == 'M' && ep->AnchorString[3] == '_')
            {
                *TableAddress = (UINT8 *)(UINTN)ep->TableAddress;
                *TableLength = (UINTN)ep->TableLength;
                *MajorVersion = ep->MajorVersion;
                return EFI_SUCCESS;
            }
        }
    }

    return EFI_NOT_FOUND;
}

EFI_STATUS
PhantomRewriteSmbiosTables(
    IN  UINT8                       *TableBase,
    IN  UINTN                       TableLength,
    IN  PHANTOM_SMBIOS_PROFILE      *Profile
)
{
    SMBIOS_HEADER   *current;
    UINTN           tableEnd;

    if (TableBase == NULL || Profile == NULL || TableLength == 0) {
        return EFI_INVALID_PARAMETER;
    }

    tableEnd = (UINTN)TableBase + TableLength;
    current = (SMBIOS_HEADER *)TableBase;

    while (current != NULL && (UINTN)current < tableEnd) {
        /* Type 127 is end-of-table */
        if (current->Type == 127) {
            break;
        }

        switch (current->Type) {
        case 0:
            RewriteType0(current, tableEnd, Profile);
            break;
        case 1:
            RewriteType1(current, tableEnd, Profile);
            break;
        case 2:
            RewriteType2(current, tableEnd, Profile);
            break;
        case 3:
            RewriteType3(current, tableEnd, Profile);
            break;
        default:
            break;
        }

        current = NextStructure(current, tableEnd);
    }

    return EFI_SUCCESS;
}

VOID
PhantomFixSmbiosChecksums(
    IN  VOID    *EntryPoint,
    IN  BOOLEAN Is64Bit
)
{
    UINT8   *bytes;
    UINTN   length;
    UINTN   i;
    UINT8   sum;

    if (Is64Bit) {
        SMBIOS3_ENTRY_POINT *ep3 = (SMBIOS3_ENTRY_POINT *)EntryPoint;
        bytes = (UINT8 *)ep3;
        length = ep3->Length;

        ep3->Checksum = 0;
        sum = 0;
        for (i = 0; i < length; i++) {
            sum += bytes[i];
        }
        ep3->Checksum = (UINT8)(0 - sum);
    } else {
        SMBIOS_ENTRY_POINT *ep = (SMBIOS_ENTRY_POINT *)EntryPoint;

        /* Fix intermediate (_DMI_) checksum first (bytes 16-30) */
        ep->IntermediateChecksum = 0;
        sum = 0;
        bytes = (UINT8 *)ep;
        for (i = 16; i < 31; i++) {
            sum += bytes[i];
        }
        ep->IntermediateChecksum = (UINT8)(0 - sum);

        /* Fix main checksum (bytes 0 through Length-1) */
        ep->Checksum = 0;
        sum = 0;
        length = ep->Length;
        for (i = 0; i < length; i++) {
            sum += bytes[i];
        }
        ep->Checksum = (UINT8)(0 - sum);
    }
}
