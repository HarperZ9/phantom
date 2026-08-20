/*
 * smbios_rewrite.h — SMBIOS table structure definitions and rewriting API.
 *
 * SMBIOS structures use a two-part layout:
 *   1. Formatted area: fixed-size header + type-specific fields
 *   2. Unformatted area: packed null-terminated strings, double-null terminated
 *
 * String fields in the formatted area are 1-based indices into the
 * unformatted string area. Index 0 means "no string."
 */

#ifndef PHANTOM_SMBIOS_REWRITE_H
#define PHANTOM_SMBIOS_REWRITE_H

#include <Uefi.h>

#pragma pack(1)

/* SMBIOS 2.x Entry Point (32-bit) */
typedef struct {
    UINT8   AnchorString[4];        /* _SM_ */
    UINT8   Checksum;
    UINT8   Length;
    UINT8   MajorVersion;
    UINT8   MinorVersion;
    UINT16  MaxStructureSize;
    UINT8   EntryPointRevision;
    UINT8   FormattedArea[5];
    UINT8   IntermediateAnchor[5];  /* _DMI_ */
    UINT8   IntermediateChecksum;
    UINT16  TableLength;
    UINT32  TableAddress;
    UINT16  NumberOfStructures;
    UINT8   BcdRevision;
} SMBIOS_ENTRY_POINT;

/* SMBIOS 3.x Entry Point (64-bit) */
typedef struct {
    UINT8   AnchorString[5];        /* _SM3_ */
    UINT8   Checksum;
    UINT8   Length;
    UINT8   MajorVersion;
    UINT8   MinorVersion;
    UINT8   DocRevision;
    UINT8   EntryPointRevision;
    UINT8   Reserved;
    UINT32  TableMaximumSize;
    UINT64  TableAddress;
} SMBIOS3_ENTRY_POINT;

/* Generic SMBIOS structure header */
typedef struct {
    UINT8   Type;
    UINT8   Length;
    UINT16  Handle;
} SMBIOS_HEADER;

/* Type 0: BIOS Information */
typedef struct {
    SMBIOS_HEADER   Header;
    UINT8           Vendor;             /* string index */
    UINT8           BiosVersion;        /* string index */
    UINT16          BiosSegment;
    UINT8           BiosReleaseDate;    /* string index */
    UINT8           BiosSize;
    UINT64          BiosCharacteristics;
} SMBIOS_TYPE0;

/* Type 1: System Information */
typedef struct {
    SMBIOS_HEADER   Header;
    UINT8           Manufacturer;       /* string index */
    UINT8           ProductName;        /* string index */
    UINT8           Version;            /* string index */
    UINT8           SerialNumber;       /* string index */
    UINT8           Uuid[16];           /* raw binary, not a string index */
    UINT8           WakeUpType;
    UINT8           SkuNumber;          /* string index */
    UINT8           Family;             /* string index */
} SMBIOS_TYPE1;

/* Type 2: Baseboard Information */
typedef struct {
    SMBIOS_HEADER   Header;
    UINT8           Manufacturer;       /* string index */
    UINT8           Product;            /* string index */
    UINT8           Version;            /* string index */
    UINT8           SerialNumber;       /* string index */
    UINT8           AssetTag;           /* string index */
    UINT8           FeatureFlags;
    UINT8           LocationInChassis;  /* string index */
    UINT16          ChassisHandle;
    UINT8           BoardType;
} SMBIOS_TYPE2;

/* Type 3: Chassis/Enclosure Information */
typedef struct {
    SMBIOS_HEADER   Header;
    UINT8           Manufacturer;       /* string index */
    UINT8           Type;
    UINT8           Version;            /* string index */
    UINT8           SerialNumber;       /* string index */
    UINT8           AssetTag;           /* string index */
} SMBIOS_TYPE3;

#pragma pack()

/*
 * Phantom SMBIOS profile — the fields to rewrite.
 * Stored in an EFI variable, read by the DXE module.
 */
#define PHANTOM_SMBIOS_MAGIC     0x534D4250  /* 'PBMS' */
#define PHANTOM_SMBIOS_VERSION   1
#define PHANTOM_SMBIOS_MAX_STR   128

typedef struct {
    UINT32  Magic;
    UINT32  Version;
    /* Type 0 fields */
    CHAR8   BiosVendor[PHANTOM_SMBIOS_MAX_STR];
    CHAR8   BiosVersion[PHANTOM_SMBIOS_MAX_STR];
    /* Type 1 fields */
    CHAR8   SystemManufacturer[PHANTOM_SMBIOS_MAX_STR];
    CHAR8   SystemProduct[PHANTOM_SMBIOS_MAX_STR];
    CHAR8   SystemSerial[PHANTOM_SMBIOS_MAX_STR];
    UINT8   SystemUuid[16];
    /* Type 2 fields */
    CHAR8   BoardManufacturer[PHANTOM_SMBIOS_MAX_STR];
    CHAR8   BoardProduct[PHANTOM_SMBIOS_MAX_STR];
    CHAR8   BoardSerial[PHANTOM_SMBIOS_MAX_STR];
    /* Type 3 fields */
    CHAR8   ChassisSerial[PHANTOM_SMBIOS_MAX_STR];
    CHAR8   ChassisAssetTag[PHANTOM_SMBIOS_MAX_STR];
} PHANTOM_SMBIOS_PROFILE;

/*
 * Locate the SMBIOS entry point table via EFI configuration table.
 * Returns EFI_SUCCESS and sets EntryPoint/TableAddress/TableLength,
 * or EFI_NOT_FOUND if no SMBIOS table is present.
 */
EFI_STATUS
PhantomLocateSmbios(
    IN  EFI_SYSTEM_TABLE    *SystemTable,
    OUT UINT8               **TableAddress,
    OUT UINTN               *TableLength,
    OUT UINT8               *MajorVersion
);

/*
 * Walk the SMBIOS structure table and rewrite string fields
 * according to the provided profile.
 */
EFI_STATUS
PhantomRewriteSmbiosTables(
    IN  UINT8                       *TableBase,
    IN  UINTN                       TableLength,
    IN  PHANTOM_SMBIOS_PROFILE      *Profile
);

/*
 * Replace a string at the given 1-based index in an SMBIOS
 * structure's unformatted area. The new string must be <= the
 * original length (excess is null-padded).
 */
EFI_STATUS
PhantomReplaceString(
    IN  SMBIOS_HEADER   *Structure,
    IN  UINTN           TableEnd,
    IN  UINT8           StringIndex,
    IN  CONST CHAR8     *NewString
);

/*
 * Recalculate SMBIOS entry point checksums after table modification.
 */
VOID
PhantomFixSmbiosChecksums(
    IN  VOID    *EntryPoint,
    IN  BOOLEAN Is64Bit
);

#endif /* PHANTOM_SMBIOS_REWRITE_H */
