// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! SeaBIOS specific loader definitions and implementation.

use crate::importer::ImageLoad;
use crate::importer::SegmentRegister;
use crate::importer::X86Register;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Importer error")]
    Importer(#[source] anyhow::Error),
}

/// Setup initial register state to support booting via SeaBIOS.
///
/// The actual SeaBIOS image is "loaded" into the guest address space by other
/// parts of the VMM, which model the special ROM and memory-mirroring semantics
/// it requires in order to operate.
pub fn load(importer: &mut dyn ImageLoad<X86Register>) -> Result<(), Error> {
    let mut import_reg = |register| {
        importer
            .import_vp_register(register)
            .map_err(Error::Importer)
    };

    // SeaBIOS expects the reset vector to be mapped at the top of 4GB, as
    // defined by the x86 architecture.
    import_reg(X86Register::Cs(SegmentRegister {
        base: 0xFFFF0000,
        limit: 0xFFFF,
        selector: 0xF000,
        attributes: 0x9B,
    }))?;

    Ok(())
}
