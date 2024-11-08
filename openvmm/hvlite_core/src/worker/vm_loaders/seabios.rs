// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use guestmem::GuestMemory;
use loader::importer::X86Register;
use thiserror::Error;
use vm_loader::Loader;
use vm_topology::memory::MemoryLayout;

#[derive(Debug, Error)]
pub enum Error {
    #[error("seabios loader error")]
    Loader(#[source] loader::seabios::Error),
}

/// Load SeaBIOS.
///
/// Since the BIOS is in ROM, this actually just returns the SeaBIOS initial
/// registers.
#[cfg_attr(not(guest_arch = "x86_64"), allow(dead_code))]
pub fn load_seabios(
    gm: &GuestMemory,
    mem_layout: &MemoryLayout,
) -> Result<Vec<X86Register>, Error> {
    let mut loader = Loader::new(gm.clone(), mem_layout, hvdef::Vtl::Vtl0);
    loader::seabios::load(&mut loader).map_err(Error::Loader)?;
    Ok(loader.initial_regs())
}
