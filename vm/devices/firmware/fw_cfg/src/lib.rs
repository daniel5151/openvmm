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

use chipset_device::io::IoResult;
use chipset_device::mmio::MmioIntercept;
use chipset_device::pio::PortIoIntercept;
use chipset_device::ChipsetDevice;
use fw_cfg_resources::FwCfgRegisterLayout;
use inspect::InspectMut;
use vmcore::device_state::ChangeDeviceState;

pub mod resolver;

enum FwCfgRegisterLayoutRegions {
    IoPort([(&'static str, std::ops::RangeInclusive<u16>); 2]),
    Mmio([(&'static str, std::ops::RangeInclusive<u64>); 3]),
}

/// An implementation of the QEMU firmware configuration device.
#[derive(InspectMut)]
pub struct FwCfg {
    // Static Config
    #[inspect(skip)]
    register_layout_regions: FwCfgRegisterLayoutRegions,
}

impl FwCfg {
    /// Create the `fw_cfg` device.
    pub fn new(register_layout: FwCfgRegisterLayout) -> FwCfg {
        FwCfg {
            register_layout_regions: match register_layout {
                FwCfgRegisterLayout::IoPort => FwCfgRegisterLayoutRegions::IoPort([
                    ("selector+data", 0x510..=0x511),
                    ("dma", 0x514..=0x514 + 8),
                ]),
                FwCfgRegisterLayout::Mmio(base) => FwCfgRegisterLayoutRegions::Mmio([
                    ("selector", (base + 8)..=(base + 8) + 1),
                    ("data", base..=base + 7),
                    ("dma", (base + 16)..=(base + 16) + 7),
                ]),
            },
        }
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
    fn mmio_read(&mut self, _addr: u64, _data: &mut [u8]) -> IoResult {
        todo!()
    }

    fn mmio_write(&mut self, _addr: u64, _data: &[u8]) -> IoResult {
        todo!()
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
        todo!()
    }

    fn io_write(&mut self, addr: u16, data: &[u8]) -> IoResult {
        todo!()
    }

    fn get_static_regions(&mut self) -> &[(&str, std::ops::RangeInclusive<u16>)] {
        if let FwCfgRegisterLayoutRegions::IoPort(regions) = &self.register_layout_regions {
            &regions[..]
        } else {
            &[]
        }
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
