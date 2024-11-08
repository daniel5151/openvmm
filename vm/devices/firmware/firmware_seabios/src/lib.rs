// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! SeaBIOS helper device.
//!
//! Unlike the Hyper-V firmware devices, this device simply owns the SeaBIOS ROM
//! mapping, deferring runtime configuration responsibilities to the standard
//! `fw_cfg` device.
//!
//! The only reason this device exists as a standalone entity is that it
//! wouldn't be correct to have `fw_cfg` own the SeaBIOS rom mapping, as
//! `fw_cfg` is a generic VMM-to-guest communication channel, not specific to
//! just SeaBIOS.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

use chipset_device::ChipsetDevice;
use guestmem::MapRom;
use guestmem::UnmapRom;
use inspect::InspectMut;
use std::fmt::Debug;
use thiserror::Error;
use vmcore::device_state::ChangeDeviceState;

/// SeaBIOS device runtime dependencies.
pub struct SeabiosRuntimeDeps {
    /// The BIOS ROM.
    ///
    /// If missing, then assume the ROM is already in memory.
    pub rom: Option<Box<dyn MapRom>>,
}

/// SeaBIOS helper device.
#[derive(InspectMut)]
pub struct SeabiosDevice {
    #[inspect(skip)]
    _rom_mems: Vec<Box<dyn UnmapRom>>,
}

/// Errors which may occur during SeaBIOS helper device initialization.
#[derive(Debug, Error)]
#[allow(missing_docs)] // self-explanatory variants
pub enum SeabiosDeviceInitError {
    #[error("invalid ROM size {0:x} bytes, expected 256KB")]
    InvalidRomSize(u64),
    #[error("error mapping ROM")]
    Rom(#[source] std::io::Error),
}

impl SeabiosDevice {
    /// Create a new instance of the SeaBIOS helper device.
    pub fn new(runtime_deps: SeabiosRuntimeDeps) -> Result<SeabiosDevice, SeabiosDeviceInitError> {
        let SeabiosRuntimeDeps { rom } = runtime_deps;

        let mut rom_mems = Vec::new();
        if let Some(rom) = rom {
            let rom_size = rom.len();
            if rom_size != 0x40000 {
                return Err(SeabiosDeviceInitError::InvalidRomSize(rom_size));
            }

            // Map the ROM at both high and low memory.
            #[allow(clippy::erasing_op, clippy::identity_op)]
            for (gpa, offset, len) in [
                (0xfffc0000, 0, 0x40000),
                // need to carefully chunk the rom mapping to account for the
                // distinct x86 memory regions below 1mb.
                (0xe0000, 0x20000 + 0x4000 * 0, 0x4000),
                (0xe4000, 0x20000 + 0x4000 * 1, 0x4000),
                (0xe8000, 0x20000 + 0x4000 * 2, 0x4000),
                (0xec000, 0x20000 + 0x4000 * 3, 0x4000),
                (0xf0000, 0x20000 + 0x4000 * 4, 0x10000),
            ] {
                let mem = rom
                    .map_rom(gpa, offset, len)
                    .map_err(SeabiosDeviceInitError::Rom)?;
                rom_mems.push(mem);
            }
        }

        Ok(SeabiosDevice {
            _rom_mems: rom_mems,
        })
    }
}

impl ChangeDeviceState for SeabiosDevice {
    fn start(&mut self) {}
    async fn stop(&mut self) {}
    async fn reset(&mut self) {}
}

impl ChipsetDevice for SeabiosDevice {}

mod save_restore {
    use super::*;
    use vmcore::save_restore::NoSavedState;
    use vmcore::save_restore::RestoreError;
    use vmcore::save_restore::SaveError;
    use vmcore::save_restore::SaveRestore;

    impl SaveRestore for SeabiosDevice {
        type SavedState = NoSavedState;

        fn save(&mut self) -> Result<Self::SavedState, SaveError> {
            Ok(NoSavedState)
        }

        fn restore(&mut self, NoSavedState: Self::SavedState) -> Result<(), RestoreError> {
            Ok(())
        }
    }
}
