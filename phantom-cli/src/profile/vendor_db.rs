use rand::Rng;

pub struct BoardVendor {
    pub manufacturer: &'static str,
    pub products: &'static [&'static str],
    pub serial_prefix: &'static str,
    pub serial_suffix_len: usize,
    pub serial_charset: SerialCharset,
    pub bios_vendor: &'static str,
    pub bios_version_prefix: &'static str,
}

pub struct DiskVendor {
    pub manufacturer: &'static str,
    pub models: &'static [&'static str],
    pub serial_prefix: &'static str,
    pub serial_suffix_len: usize,
    pub serial_charset: SerialCharset,
    pub firmware_prefix: &'static str,
    pub firmware_suffix_len: usize,
}

pub struct NicVendor {
    pub manufacturer: &'static str,
    pub adapter_name_prefix: &'static str,
    pub oui_prefixes: &'static [[u8; 3]],
}

pub struct GpuDevice {
    pub vendor_name: &'static str,
    pub vendor_id: u16,
    pub devices: &'static [(u16, &'static str)],
}

pub struct TpmVendor {
    pub manufacturer_id: &'static str,
    pub manufacturer_name: &'static str,
    pub spec_version: &'static str,
}

pub struct DisplayVendor {
    pub manufacturer_code: &'static str,
    pub manufacturer_name: &'static str,
    pub product_code_range: (u16, u16),
}

#[derive(Clone, Copy)]
pub enum SerialCharset {
    AlphaNumeric,
    Numeric,
    HexUpper,
}

impl SerialCharset {
    pub fn generate_char<R: Rng>(&self, rng: &mut R) -> char {
        match self {
            SerialCharset::AlphaNumeric => {
                const CHARS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
                CHARS[rng.gen_range(0..CHARS.len())] as char
            }
            SerialCharset::Numeric => {
                (b'0' + rng.gen_range(0..10u8)) as char
            }
            SerialCharset::HexUpper => {
                const CHARS: &[u8] = b"0123456789ABCDEF";
                CHARS[rng.gen_range(0..CHARS.len())] as char
            }
        }
    }
}

pub fn generate_serial<R: Rng>(
    prefix: &str,
    suffix_len: usize,
    charset: SerialCharset,
    rng: &mut R,
) -> String {
    let mut s = String::with_capacity(prefix.len() + suffix_len);
    s.push_str(prefix);
    for _ in 0..suffix_len {
        s.push(charset.generate_char(rng));
    }
    s
}

pub static BOARD_VENDORS: &[BoardVendor] = &[
    BoardVendor {
        manufacturer: "ASUSTeK COMPUTER INC.",
        products: &[
            "ROG STRIX B550-F GAMING",
            "PRIME Z790-P WIFI",
            "TUF GAMING B650-PLUS WIFI",
            "ROG MAXIMUS Z790 HERO",
            "PRIME B660M-A WIFI D4",
            "ROG STRIX X670E-E GAMING WIFI",
            "TUF GAMING B550M-PLUS",
            "ProArt Z790-CREATOR WIFI",
        ],
        serial_prefix: "M8",
        serial_suffix_len: 16,
        serial_charset: SerialCharset::Numeric,
        bios_vendor: "American Megatrends Inc.",
        bios_version_prefix: "ASUS ",
    },
    BoardVendor {
        manufacturer: "Gigabyte Technology Co., Ltd.",
        products: &[
            "B550 AORUS PRO AC",
            "Z790 AORUS ELITE AX",
            "B650 AORUS ELITE AX",
            "X670E AORUS MASTER",
            "B660M DS3H DDR4",
            "Z690 GAMING X DDR4",
            "B550M DS3H",
            "X570 AORUS ELITE WIFI",
        ],
        serial_prefix: "SN",
        serial_suffix_len: 12,
        serial_charset: SerialCharset::AlphaNumeric,
        bios_vendor: "American Megatrends International, LLC.",
        bios_version_prefix: "F",
    },
    BoardVendor {
        manufacturer: "Micro-Star International Co., Ltd.",
        products: &[
            "MAG B550 TOMAHAWK",
            "PRO Z790-A WIFI",
            "MAG B650 TOMAHAWK WIFI",
            "MEG Z690 UNIFY",
            "MPG B550 GAMING EDGE WIFI",
            "PRO B660M-A WIFI DDR4",
            "MAG X570S TOMAHAWK MAX WIFI",
            "MPG Z790 CARBON WIFI",
        ],
        serial_prefix: "K7",
        serial_suffix_len: 10,
        serial_charset: SerialCharset::Numeric,
        bios_vendor: "American Megatrends LLC.",
        bios_version_prefix: "A.",
    },
    BoardVendor {
        manufacturer: "ASRock",
        products: &[
            "B550M Steel Legend",
            "Z790 Pro RS",
            "B650 LiveMixer",
            "X670E Taichi",
            "B660M Pro RS D4",
            "Z690 Phantom Gaming 4",
            "B550 Phantom Gaming-ITX/ax",
            "X570 Taichi",
        ],
        serial_prefix: "",
        serial_suffix_len: 18,
        serial_charset: SerialCharset::AlphaNumeric,
        bios_vendor: "American Megatrends International, LLC.",
        bios_version_prefix: "P",
    },
];

pub static DISK_VENDORS: &[DiskVendor] = &[
    DiskVendor {
        manufacturer: "Samsung",
        models: &[
            "Samsung SSD 970 EVO Plus 1TB",
            "Samsung SSD 980 PRO 2TB",
            "Samsung SSD 990 PRO 1TB",
            "Samsung SSD 870 EVO 500GB",
            "Samsung SSD 860 EVO 1TB",
            "Samsung SSD 990 EVO 2TB",
        ],
        serial_prefix: "S5GX",
        serial_suffix_len: 11,
        serial_charset: SerialCharset::AlphaNumeric,
        firmware_prefix: "2B2Q",
        firmware_suffix_len: 4,
    },
    DiskVendor {
        manufacturer: "Western Digital",
        models: &[
            "WDC WD10EZEX-00BBHA0",
            "WD_BLACK SN850X 1TB",
            "WD_BLACK SN770 2TB",
            "WDC WD20EZBX-00ATAAO",
            "WD Blue SN580 1TB",
            "WD_BLACK SN850X 2TB",
        ],
        serial_prefix: "WD-WMC",
        serial_suffix_len: 8,
        serial_charset: SerialCharset::AlphaNumeric,
        firmware_prefix: "01.01",
        firmware_suffix_len: 3,
    },
    DiskVendor {
        manufacturer: "Seagate",
        models: &[
            "ST1000DM010-2EP102",
            "ST2000DM008-2UB102",
            "Seagate FireCuda 530 1TB",
            "Seagate Barracuda Q5 1TB",
            "ST4000DM004-2CV104",
            "Seagate FireCuda 540 2TB",
        ],
        serial_prefix: "ZA",
        serial_suffix_len: 6,
        serial_charset: SerialCharset::AlphaNumeric,
        firmware_prefix: "CC",
        firmware_suffix_len: 2,
    },
    DiskVendor {
        manufacturer: "Crucial/Micron",
        models: &[
            "CT1000MX500SSD1",
            "CT1000P5PSSD8",
            "CT2000T500SSD8",
            "CT500P3SSD8",
            "Micron 3400 NVMe 1024GB",
            "CT1000T700SSD3",
        ],
        serial_prefix: "2143",
        serial_suffix_len: 8,
        serial_charset: SerialCharset::HexUpper,
        firmware_prefix: "M3CR",
        firmware_suffix_len: 3,
    },
];

pub static NIC_VENDORS: &[NicVendor] = &[
    NicVendor {
        manufacturer: "Intel",
        adapter_name_prefix: "Intel(R) Ethernet",
        oui_prefixes: &[
            [0x00, 0x1B, 0x21],
            [0x3C, 0x22, 0xFB],
            [0xA0, 0x36, 0x9F],
            [0x48, 0x21, 0x0B],
            [0xA4, 0xBB, 0x6D],
            [0x8C, 0xEC, 0x4B],
        ],
    },
    NicVendor {
        manufacturer: "Realtek",
        adapter_name_prefix: "Realtek PCIe GbE",
        oui_prefixes: &[
            [0x00, 0xE0, 0x4C],
            [0x52, 0x54, 0x00],
            [0x00, 0x0C, 0xEC],
            [0x30, 0xB4, 0x9E],
            [0xD8, 0xBB, 0xC1],
        ],
    },
    NicVendor {
        manufacturer: "Broadcom",
        adapter_name_prefix: "Broadcom NetXtreme",
        oui_prefixes: &[
            [0x00, 0x10, 0x18],
            [0x00, 0x24, 0xD7],
            [0x20, 0xF8, 0x5E],
            [0xAC, 0x10, 0x25],
        ],
    },
    NicVendor {
        manufacturer: "Qualcomm",
        adapter_name_prefix: "Qualcomm Atheros",
        oui_prefixes: &[
            [0x00, 0x03, 0x7F],
            [0x28, 0xC6, 0x3F],
            [0x9C, 0xB7, 0x0D],
        ],
    },
];

pub static GPU_VENDORS: &[GpuDevice] = &[
    GpuDevice {
        vendor_name: "NVIDIA",
        vendor_id: 0x10DE,
        devices: &[
            (0x2684, "NVIDIA GeForce RTX 4090"),
            (0x2704, "NVIDIA GeForce RTX 4080"),
            (0x2782, "NVIDIA GeForce RTX 4070 Ti"),
            (0x2786, "NVIDIA GeForce RTX 4070"),
            (0x2204, "NVIDIA GeForce RTX 3090"),
            (0x2206, "NVIDIA GeForce RTX 3080"),
            (0x2484, "NVIDIA GeForce RTX 3070"),
            (0x2504, "NVIDIA GeForce RTX 3060"),
        ],
    },
    GpuDevice {
        vendor_name: "AMD",
        vendor_id: 0x1002,
        devices: &[
            (0x744C, "AMD Radeon RX 7900 XTX"),
            (0x7480, "AMD Radeon RX 7800 XT"),
            (0x73BF, "AMD Radeon RX 6800 XT"),
            (0x73DF, "AMD Radeon RX 6700 XT"),
            (0x7460, "AMD Radeon RX 7600"),
            (0x73FF, "AMD Radeon RX 6600 XT"),
        ],
    },
];

pub static TPM_VENDORS: &[TpmVendor] = &[
    TpmVendor {
        manufacturer_id: "IFX",
        manufacturer_name: "Infineon",
        spec_version: "2.0",
    },
    TpmVendor {
        manufacturer_id: "STM",
        manufacturer_name: "STMicroelectronics",
        spec_version: "2.0",
    },
    TpmVendor {
        manufacturer_id: "NTC",
        manufacturer_name: "Nuvoton Technology",
        spec_version: "2.0",
    },
    TpmVendor {
        manufacturer_id: "INTC",
        manufacturer_name: "Intel Platform Trust Technology",
        spec_version: "2.0",
    },
];

pub static DISPLAY_VENDORS: &[DisplayVendor] = &[
    DisplayVendor {
        manufacturer_code: "DEL",
        manufacturer_name: "Dell",
        product_code_range: (0xD0A0, 0xD0FF),
    },
    DisplayVendor {
        manufacturer_code: "GSM",
        manufacturer_name: "LG Electronics",
        product_code_range: (0x5B08, 0x5BFF),
    },
    DisplayVendor {
        manufacturer_code: "SAM",
        manufacturer_name: "Samsung",
        product_code_range: (0x0C4C, 0x0CFF),
    },
    DisplayVendor {
        manufacturer_code: "ACI",
        manufacturer_name: "ASUS",
        product_code_range: (0x2480, 0x24FF),
    },
    DisplayVendor {
        manufacturer_code: "ACR",
        manufacturer_name: "Acer",
        product_code_range: (0x0490, 0x04FF),
    },
    DisplayVendor {
        manufacturer_code: "BNQ",
        manufacturer_name: "BenQ",
        product_code_range: (0x8024, 0x80FF),
    },
    DisplayVendor {
        manufacturer_code: "HWP",
        manufacturer_name: "HP",
        product_code_range: (0x3340, 0x33FF),
    },
];

pub static COMPUTER_NAME_ADJECTIVES: &[&str] = &[
    "DESKTOP", "WORKSTATION", "PC", "HOME", "OFFICE", "LAB", "DEV",
];

pub static PRODUCT_ID_PREFIXES: &[&str] = &[
    "00330", "00331", "00325", "00326", "00327", "00376", "00378",
];
