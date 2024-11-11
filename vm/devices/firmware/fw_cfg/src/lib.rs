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
use guestmem::GuestMemory;
use inspect::Inspect;
use inspect::InspectMut;
use spec::Selector;
use std::fs::File;
use thiserror::Error;
use vm_topology::memory::MemoryLayout;
use vm_topology::processor::x86::X86Topology;
use vm_topology::processor::ProcessorTopology;
use vmcore::device_state::ChangeDeviceState;
use zerocopy::AsBytes;

mod spec;

/// `fw_cfg` device static configuration data.
#[derive(Debug, Inspect)]
pub struct FwCfgConfig {
    /// Register layout of the `fw_cfg` device
    pub register_layout: FwCfgRegisterLayout,
    /// Number of VCPUs
    pub processor_topology: ProcessorTopology<X86Topology>,
    /// The VM's memory layout
    pub mem_layout: MemoryLayout,
}

/// File registered with the `fw_cfg` device
#[derive(Inspect)]
#[inspect(tag = "kind")]
pub enum FwCfgFile {
    /// String
    String(#[inspect(rename = "contents")] String),
    /// Blob
    Vec(#[inspect(rename = "contents")] Vec<u8>),
    /// File (prefer this when registering large files)
    File(#[inspect(rename = "contents")] File),
}

/// The base address for the `fw_cfg` device, either an MMIO address or an IO
/// port.
#[derive(Copy, Clone, Debug, Inspect)]
#[inspect(tag = "kind")]
pub enum FwCfgRegisterLayout {
    /// Fixed x86 IO ports.
    ///
    /// - Selector: 0x510
    /// - Data: 0x511
    /// - DMA: 0x514
    IoPort,
    /// Relocatable MMIO base address.
    ///
    /// - Selector: base + 8 (2 bytes)
    /// - Data: base + 0 (8 bytes)
    /// - DMA: base + 16 (8 bytes)
    Mmio(#[inspect(rename = "base")] u64),
}

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

    // Runtime deps
    gm: GuestMemory,

    // Runtime book-keeping
    buf: Vec<u8>,
    buf_valid: bool,

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
    pub fn new(gm: GuestMemory, config: FwCfgConfig) -> Result<FwCfg, Error> {
        let FwCfgConfig {
            register_layout,
            processor_topology,
            mem_layout,
        } = config;

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

        let mut files: Vec<(String, FwCfgFile)> = Vec::new();
        files.push(("etc/e820".into(), FwCfgFile::Vec(gen_e820(&mem_layout))));
        files.push(("etc/show-boot-menu".into(), FwCfgFile::Vec(vec![1])));

        Ok(FwCfg {
            register_layout,
            register_layout_regions,
            files,

            gm,

            buf: Vec::new(),
            buf_valid: false,

            selector: Selector(u16::MAX),
            data_offset: 0,
            dma_lo: 0,
            dma_hi: 0,
        })
    }

    fn write_selector(&mut self, data: u16) -> IoResult {
        let new_selector = Selector(data);
        let old_selector = self.selector;

        self.selector = new_selector;
        self.data_offset = 0;
        self.buf_valid = new_selector == old_selector;

        tracing::debug!(?new_selector, "set selector");

        IoResult::Ok
    }

    fn read_data(&mut self, data: &mut [u8]) -> IoResult {
        if !self.buf_valid {
            self.buf.clear();

            match self.selector {
                Selector::SIGNATURE => self.buf.extend_from_slice(b"QEMU"),
                Selector::ID => {
                    self.buf.extend_from_slice(
                        spec::IdBitmap::new()
                            .with_trad(true)
                            .with_dma(false) // DMA isn't implemented yet
                            .into_bits()
                            .as_bytes(),
                    )
                }
                Selector::FILE_DIR => {
                    if let Err(err) = gen_file_dir(&mut self.buf, &self.files) {
                        tracelimit::error_ratelimited!(
                            err = &err as &dyn std::error::Error,
                            "error constructing file dir listing"
                        );
                        self.buf_valid = false;
                        return IoResult::Ok;
                    }
                }
                Selector(n)
                    if !(Selector::BASE_FILE.0..Selector::BASE_ARCH_LOCAL.0).contains(&n) =>
                {
                    tracing::debug!(selector = ?self.selector, "accessing legacy fw_cfg selector");
                }
                Selector(file_id) => {
                    let Some((name, data)) =
                        self.files.get((file_id - Selector::BASE_FILE.0) as usize)
                    else {
                        tracelimit::warn_ratelimited!(file_id, "invalid file");
                        self.buf_valid = false;
                        return IoResult::Ok;
                    };

                    tracing::debug!(file_id, name, "init read buf");

                    match data {
                        FwCfgFile::String(s) => self.buf.extend_from_slice(s.as_bytes()),
                        FwCfgFile::Vec(v) => self.buf.extend_from_slice(v),
                        FwCfgFile::File(_file) => todo!(),
                    }
                }
            };

            self.buf_valid = true;
        }

        let buf_remaining = self.buf.len().saturating_sub(self.data_offset);
        let n = buf_remaining.min(data.len());
        if n > 0 {
            data[..n].copy_from_slice(&self.buf[self.data_offset..self.data_offset + n]);
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

        let access = match self.gm.read_plain::<spec::DmaAccess>(gpa) {
            Ok(v) => v,
            Err(err) => {
                tracelimit::error_ratelimited!(
                    err = &err as &dyn std::error::Error,
                    "failed to read DMA access struct from guest-mem"
                );
                return IoResult::Ok;
            }
        };

        let spec::DmaAccess {
            control,
            length,
            address,
        } = access;

        let control = spec::DmaControl::from_bits(control.get());
        let length = length.get() as usize;
        let address = address.get();

        // TODO: actually kick off the DMA
        let _ = (control, length, address);
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

fn gen_file_dir(buf: &mut Vec<u8>, files: &[(String, FwCfgFile)]) -> Result<(), Error> {
    buf.extend_from_slice(
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

        buf.extend_from_slice(
            spec::FileDirFile {
                size: size.into(),
                select: (Selector::BASE_FILE.0 + idx as u16).into(),
                reserved: 0.into(),
                name: name_buf,
            }
            .as_bytes(),
        );
    }
    Ok(())
}

fn gen_e820(mem_layout: &MemoryLayout) -> Vec<u8> {
    use zerocopy::AsBytes;
    use zerocopy::FromBytes;
    use zerocopy::FromZeroes;

    enum E820ReservationType {
        Ram = 1,
        Reserved = 2,
    }

    #[derive(FromBytes, AsBytes, FromZeroes)]
    #[repr(C, packed)]
    struct E820Reservation {
        pub address: u64,
        pub length: u64,
        pub ty: u32,
        // pub _padding: u32,
    };

    let mut v = Vec::new();

    for range in mem_layout.ram() {
        tracing::debug!(?range, "reporting e820 ram range");
        v.extend_from_slice(
            E820Reservation {
                address: range.range.start(),
                length: range.range.len(),
                ty: E820ReservationType::Ram as u32,
                // _padding: 0,
            }
            .as_bytes(),
        );
    }

    v
}
