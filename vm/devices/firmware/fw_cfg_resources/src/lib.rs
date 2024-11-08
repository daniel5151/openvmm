// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Resources for the `fw_cfg` device.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use mesh::MeshPayload;
use vm_resource::kind::ChipsetDeviceHandleKind;
use vm_resource::ResourceId;

/// A handle to the `fw_cfg` device.
#[derive(MeshPayload)]
pub struct FwCfgHandle {
    /// `fw_cfg` register layout
    pub register_layout: FwCfgRegisterLayout,
}

impl ResourceId<ChipsetDeviceHandleKind> for FwCfgHandle {
    const ID: &'static str = "fw_cfg";
}

/// The base address for the `fw_cfg` device, either an MMIO address or an IO
/// port.
#[derive(MeshPayload)]
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
    Mmio(u64),
}
