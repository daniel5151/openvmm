// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Exports [`FwCfg`]: an implementation of the QEMU firmware configuration
//! device.
//!
//! This implementation is derived from reading technical specifications of how
//! the device should operate, as well as reading guest-side code which
//! interacts with the device.**No code is shared or derived with the QEMU
//! `fw_cfg` device implementation.**

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use chipset_device::io::IoError;
use chipset_device::io::IoResult;
use chipset_device::mmio::MmioIntercept;
use chipset_device::pio::PortIoIntercept;
use chipset_device::ChipsetDevice;
use fw_cfg_resources::FwCfgFile;
use fw_cfg_resources::FwCfgRegisterLayout;
use inspect::InspectMut;
use spec::Selector;
use thiserror::Error;
use vmcore::device_state::ChangeDeviceState;
use zerocopy::AsBytes;

pub mod resolver;

enum FwCfgRegisterLayoutRegions {
    IoPort([(&'static str, std::ops::RangeInclusive<u16>); 2]),
    Mmio([(&'static str, std::ops::RangeInclusive<u64>); 3]),
}

/// An implementation of the QEMU firmware configuration device.
#[derive(InspectMut)]
pub struct FwCfg {
    // Static Config
    register_layout: FwCfgRegisterLayout,
    #[inspect(skip)]
    register_layout_regions: FwCfgRegisterLayoutRegions,
    #[inspect(with = "|v| inspect::iter_by_key(v.iter().map(|(k, v)| (k, v)))")]
    files: Vec<(String, FwCfgFile)>,
    // response for Selector::FILE_DIR, computed once during init based on the
    // provided file set.
    file_dir_buf: Vec<u8>,
    id_bitmap: u32,

    // Runtime book-keeping
    // ... none yet ...

    // Volatile state
    selector: Selector,
    data_offset: usize,
    dma_lo: u32,
    dma_hi: u32,
}

/// Error which may occur during `fw_cfg` device construction
#[derive(Debug, Error)]
#[allow(missing_docs)]
pub enum Error {
    #[error("filename '{0}' is not ascii")]
    FilenameNotAscii(String),
    #[error("filename '{0}' is too long (max 55 ascii chars)")]
    FilenameTooLong(String),
    #[error("failed to query metadata for file with filename '{0}'")]
    MetadataIo(String, #[source] std::io::Error),
    #[error("file with filename '{0}' is too large (len cannot exceed u32::MAX)")]
    FileTooBig(String),
}

impl FwCfg {
    /// Create the `fw_cfg` device.
    pub fn new(register_layout: FwCfgRegisterLayout) -> Result<FwCfg, Error> {
        let register_layout_regions = match register_layout {
            FwCfgRegisterLayout::IoPort => FwCfgRegisterLayoutRegions::IoPort([
                (
                    "selector+data",
                    (PioRegisters::SELECTOR.0)..=(PioRegisters::DATA.0),
                ),
                (
                    "dma",
                    (PioRegisters::DMA_HI.0)..=(PioRegisters::DMA_HI.0 + 7),
                ),
            ]),
            FwCfgRegisterLayout::Mmio(base) => FwCfgRegisterLayoutRegions::Mmio([
                (
                    "selector",
                    (base + MmioRegister::SELECTOR.0)..=(base + MmioRegister::SELECTOR.0 + 1),
                ),
                (
                    "data",
                    (base + MmioRegister::DATA.0)..=(base + MmioRegister::DATA.0 + 7),
                ),
                (
                    "dma",
                    (base + MmioRegister::DMA_HI.0)..=(base + MmioRegister::DMA_HI.0 + 7),
                ),
            ]),
        };

        // TODO: figure out the right API for actually registering files.
        //
        // during bringup - just hard-code files on an as-needed basis
        let mut files: Vec<(String, FwCfgFile)> = Vec::new();

        let mut file_dir_buf = Vec::new();
        file_dir_buf.extend_from_slice(
            spec::FileDirFiles {
                count: (files.len() as u32).into(),
            }
            .as_bytes(),
        );
        for (idx, (name, content)) in files.iter().enumerate() {
            let name_buf = {
                if !name.is_ascii() {
                    return Err(Error::FilenameNotAscii(name.clone()));
                }

                if name.len() > 55 {
                    return Err(Error::FilenameTooLong(name.clone()));
                }

                let mut name_buf = [0; 56];
                name_buf[..name.len()].copy_from_slice(name.as_bytes());
                name_buf
            };

            let size = {
                let size = match content {
                    FwCfgFile::String(s) => s.len() as u64,
                    FwCfgFile::Vec(v) => v.len() as u64,
                    FwCfgFile::File(file) => file
                        .metadata()
                        .map_err(|e| Error::MetadataIo(name.clone(), e))?
                        .len(),
                };

                if size > u32::MAX as u64 {
                    return Err(Error::FileTooBig(name.clone()));
                }

                size as u32
            };

            file_dir_buf.extend_from_slice(
                spec::FileDirFile {
                    size: size.into(),
                    select: (Selector::BASE_FILE.0 + idx as u16).into(),
                    reserved: 0.into(),
                    name: name_buf,
                }
                .as_bytes(),
            )
        }

        let id_bitmap = spec::IdBitmap::new()
            .with_trad(true)
            .with_dma(false) // DMA isn't implemented yet
            .into_bits();

        Ok(FwCfg {
            register_layout,
            register_layout_regions,
            id_bitmap,
            files,
            file_dir_buf,

            selector: Selector::SIGNATURE,
            data_offset: 0,
            dma_lo: 0,
            dma_hi: 0,
        })
    }

    fn write_selector(&mut self, data: u16) -> IoResult {
        self.selector = Selector(data);
        self.data_offset = 0;
        tracing::debug!(selector = ?self.selector, "set selector");
        IoResult::Ok
    }

    fn read_data(&mut self, data: &mut [u8]) -> IoResult {
        let buf = match self.selector {
            Selector::SIGNATURE => b"QEMU",
            Selector::ID => self.id_bitmap.as_bytes(),
            Selector::FILE_DIR => &self.file_dir_buf,
            Selector(n) if !(Selector::BASE_FILE.0..Selector::BASE_ARCH_LOCAL.0).contains(&n) => {
                tracing::debug!(selector = ?self.selector, "accessing legacy fw_cfg selector");
                &[]
            }
            Selector(_file_id) => &[],
        };

        let buf_remaining = buf.len().saturating_sub(self.data_offset);
        let n = buf_remaining.min(data.len());
        if n > 0 {
            data[..n].copy_from_slice(&buf[self.data_offset..self.data_offset + n]);
        }
        self.data_offset += n;

        IoResult::Ok
    }

    fn write_dma_hi(&mut self, val: &[u8]) -> IoResult {
        self.dma_hi = {
            let mut dma_hi = zerocopy::byteorder::big_endian::U32::new(0);
            let len = val.len().min(4);
            dma_hi.as_bytes_mut()[..len].copy_from_slice(&val[..len]);
            dma_hi.get()
        };

        if val.len() == 8 {
            self.write_dma_lo(&val[4..])
        } else {
            IoResult::Ok
        }
    }

    fn write_dma_lo(&mut self, val: &[u8]) -> IoResult {
        self.dma_lo = {
            let mut dma_lo = zerocopy::byteorder::big_endian::U32::new(0);
            let len = val.len().min(4);
            dma_lo.as_bytes_mut()[..len].copy_from_slice(&val[..len]);
            dma_lo.get()
        };

        let gpa = ((self.dma_hi as u64) << 4) | self.dma_lo as u64;

        // TODO: actually trigger the DMA operation when the lo register is
        // written to
        let _ = gpa;
        todo!()
    }
}

impl ChangeDeviceState for FwCfg {
    fn start(&mut self) {}

    async fn stop(&mut self) {}

    async fn reset(&mut self) {}
}

impl ChipsetDevice for FwCfg {
    fn supports_pio(&mut self) -> Option<&mut dyn PortIoIntercept> {
        Some(self)
    }

    fn supports_mmio(&mut self) -> Option<&mut dyn MmioIntercept> {
        Some(self)
    }
}

impl MmioIntercept for FwCfg {
    fn mmio_read(&mut self, addr: u64, data: &mut [u8]) -> IoResult {
        let FwCfgRegisterLayout::Mmio(base) = self.register_layout else {
            unreachable!();
        };

        match MmioRegister(addr - base) {
            MmioRegister::DATA => self.read_data(data),
            // all other regs are write-only
            _ => IoResult::Err(IoError::InvalidRegister),
        }
    }

    fn mmio_write(&mut self, addr: u64, data: &[u8]) -> IoResult {
        let FwCfgRegisterLayout::Mmio(base) = self.register_layout else {
            unreachable!();
        };

        match MmioRegister(addr - base) {
            MmioRegister::SELECTOR => {
                let Ok(data) = data.try_into() else {
                    return IoResult::Err(IoError::InvalidAccessSize);
                };
                // big-endian when on MMIO
                self.write_selector(u16::from_be_bytes(data))
            }
            MmioRegister::DATA => {
                tracelimit::warn_ratelimited!("writing via the data register is deprecated");
                IoResult::Ok
            }
            MmioRegister::DMA_HI => self.write_dma_hi(data),
            MmioRegister::DMA_LO => self.write_dma_lo(data),
            _ => IoResult::Err(IoError::InvalidRegister),
        }
    }

    fn get_static_regions(&mut self) -> &[(&str, std::ops::RangeInclusive<u64>)] {
        if let FwCfgRegisterLayoutRegions::Mmio(regions) = &self.register_layout_regions {
            &regions[..]
        } else {
            &[]
        }
    }
}

impl PortIoIntercept for FwCfg {
    fn io_read(&mut self, addr: u16, data: &mut [u8]) -> IoResult {
        let FwCfgRegisterLayout::IoPort = self.register_layout else {
            unreachable!()
        };

        match PioRegisters(addr) {
            PioRegisters::DATA => self.read_data(data),
            // all other regs are write-only
            _ => IoResult::Err(IoError::InvalidRegister),
        }
    }

    fn io_write(&mut self, addr: u16, data: &[u8]) -> IoResult {
        let FwCfgRegisterLayout::IoPort = self.register_layout else {
            unreachable!()
        };

        match PioRegisters(addr) {
            PioRegisters::SELECTOR => {
                let Ok(data) = data.try_into() else {
                    return IoResult::Err(IoError::InvalidAccessSize);
                };
                self.write_selector(u16::from_le_bytes(data))
            }
            PioRegisters::DATA => {
                tracelimit::warn_ratelimited!("writing via the data port is deprecated");
                IoResult::Ok
            }
            PioRegisters::DMA_HI => self.write_dma_hi(data),
            PioRegisters::DMA_LO => self.write_dma_lo(data),
            _ => IoResult::Err(IoError::InvalidRegister),
        }
    }

    fn get_static_regions(&mut self) -> &[(&str, std::ops::RangeInclusive<u16>)] {
        if let FwCfgRegisterLayoutRegions::IoPort(regions) = &self.register_layout_regions {
            &regions[..]
        } else {
            &[]
        }
    }
}

open_enum::open_enum! {
    enum PioRegisters: u16 {
        SELECTOR = 0x510, // 16-bit, overlaps with DATA
        DATA     = 0x511, // 8-bit
        DMA_HI   = 0x514, // 32-bit
        DMA_LO   = 0x518, // 32-bit
    }
}

open_enum::open_enum! {
    enum MmioRegister: u64 {
        DATA     = 0,  // Variable width (aligned, any of 64/32/16/8-bit)
        SELECTOR = 8,  // 16-bit, overlaps with DATA
        DMA_HI   = 16, // 32-bit
        DMA_LO   = 20, // 32-bit
    }
}

mod spec {
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
    }

    #[derive(Inspect)]
    #[bitfield(u32)]
    pub struct IdBitmap {
        pub trad: bool,
        pub dma: bool,
        #[bits(30)]
        pub _reserved: u32,
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
}

mod save_restore {
    use super::*;
    use vmcore::save_restore::NoSavedState;
    use vmcore::save_restore::RestoreError;
    use vmcore::save_restore::SaveError;
    use vmcore::save_restore::SaveRestore;

    impl SaveRestore for FwCfg {
        type SavedState = NoSavedState; // TODO

        fn save(&mut self) -> Result<Self::SavedState, SaveError> {
            Ok(NoSavedState)
        }

        fn restore(&mut self, _state: Self::SavedState) -> Result<(), RestoreError> {
            Ok(())
        }
    }
}
