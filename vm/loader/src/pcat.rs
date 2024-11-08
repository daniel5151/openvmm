// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! PCAT specific loader definitions and implementation.

use crate::importer::ImageLoad;
use crate::importer::SegmentRegister;
use crate::importer::X86Register;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Importer error")]
    Importer(#[source] anyhow::Error),
}

/// Setup initial register state to support booting via PCAT BIOS.
///
/// The actual PCAT BIOS image is "loaded" into the guest address space by other
/// parts of the VMM, which model the special ROM and memory-mirroring semantics
/// it requires in order to operate.
pub fn load(importer: &mut dyn ImageLoad<X86Register>) -> Result<(), Error> {
    // Enable MTRRs, default MTRR is uncached, and set lowest 640KB and highest 128KB as WB
    let mut import_reg = |register| {
        importer
            .import_vp_register(register)
            .map_err(Error::Importer)
    };
    import_reg(X86Register::MtrrDefType(0xc00))?;
    import_reg(X86Register::MtrrFix64k00000(0x0606060606060606))?;
    import_reg(X86Register::MtrrFix16k80000(0x0606060606060606))?;
    import_reg(X86Register::MtrrFix4kE0000(0x0606060606060606))?;
    import_reg(X86Register::MtrrFix4kE8000(0x0606060606060606))?;
    import_reg(X86Register::MtrrFix4kF0000(0x0606060606060606))?;
    import_reg(X86Register::MtrrFix4kF8000(0x0606060606060606))?;

    // The PCAT bios expects the reset vector to be mapped at the top of 1MB, not the architectural reset
    // value at the top of 4GB, so set CS to the same values HyperV does to ensure things work across all
    // virtualization stacks.
    import_reg(X86Register::Cs(SegmentRegister {
        base: 0xF0000,
        limit: 0xFFFF,
        selector: 0xF000,
        attributes: 0x9B,
    }))?;

    Ok(())
}
