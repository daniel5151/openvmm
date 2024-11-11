// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use bitfield_struct::bitfield;
use inspect::Inspect;
use packed_nums::*;
use zerocopy::AsBytes;
use zerocopy::FromBytes;
use zerocopy::FromZeroes;

#[allow(non_camel_case_types)]
mod packed_nums {
    pub type u16_be = zerocopy::U16<zerocopy::BigEndian>;
    pub type u32_be = zerocopy::U32<zerocopy::BigEndian>;
    pub type u64_be = zerocopy::U64<zerocopy::BigEndian>;
}

#[derive(Inspect)]
#[bitfield(u32)]
pub struct IdBitmap {
    pub trad: bool,
    pub dma: bool,
    #[bits(30)]
    pub _reserved: u32,
}

#[derive(Inspect)]
#[bitfield(u32)]
pub struct DmaControl {
    pub error: bool,
    pub read: bool,
    pub skip: bool,
    pub select: bool,
    pub write: bool,
    #[bits(11)]
    pub _reserved: u32,
    #[bits(16)]
    pub selector: Selector,
}

#[derive(AsBytes, FromBytes, FromZeroes)]
#[repr(C)]
pub struct DmaAccess {
    pub control: u32_be,
    pub length: u32_be,
    pub address: u64_be,
}

open_enum::open_enum! {
    #[derive(Inspect)]
    #[inspect(debug)]
    pub enum Selector: u16 {
        SIGNATURE = 0x0,
        ID        = 0x1,
        FILE_DIR  = 0x19,

        // deprecated variants, pulled from SeaBIOS source code
        UUID      = 0x02,
        NOGRAPHIC = 0x04,
        NUMA      = 0x0d,
        BOOT_MENU = 0x0e,
        NB_CPUS   = 0x05,
        MAX_CPUS  = 0x0f,
        X86_ACPI_TABLES    = Self::BASE_ARCH_LOCAL.0,
        X86_SMBIOS_ENTRIES = Self::BASE_ARCH_LOCAL.0 + 1,
        X86_IRQ0_OVERRIDE  = Self::BASE_ARCH_LOCAL.0 + 2,
        X86_E820_TABLE     = Self::BASE_ARCH_LOCAL.0 + 3,

        // offsets
        BASE_FILE = 0x20,
        BASE_ARCH_LOCAL = 0x8000,
    }
}

// implementations for use with `bitfield`
impl Selector {
    const fn from_bits(v: u16) -> Self {
        Self(v)
    }

    const fn into_bits(self) -> u16 {
        self.0
    }
}

#[derive(AsBytes, FromBytes, FromZeroes)]
#[repr(C)]
pub struct FileDirFiles {
    /// Number of entries
    pub count: u32_be,
    // ...followed by `count` FileDirFile entries
}

/// Individual file entry, exactly 64 bytes total
#[derive(Clone, Debug, AsBytes, FromBytes, FromZeroes)]
#[repr(C)]
pub struct FileDirFile {
    /// Size of referenced fw_cfg item
    pub size: u32_be,
    /// Selector key of fw_cfg item
    pub select: u16_be,
    /// Reserved field for alignment
    pub reserved: u16_be,
    /// fw_cfg item name, NUL-terminated ASCII (56 bytes)
    pub name: [u8; 56],
}
