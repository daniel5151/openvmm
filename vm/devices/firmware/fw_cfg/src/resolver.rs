//! A resolver for [`FwCfgHandle`] resources.

use crate::FwCfg;
use chipset_device_resources::ResolveChipsetDeviceHandleParams;
use chipset_device_resources::ResolvedChipsetDevice;
use fw_cfg_resources::FwCfgHandle;
use std::convert::Infallible;
use vm_resource::declare_static_resolver;
use vm_resource::kind::ChipsetDeviceHandleKind;
use vm_resource::ResolveResource;

/// A resolver for [`FwCfgHandle`] resources.
pub struct FwCfgResolver;

declare_static_resolver!(FwCfgResolver, (ChipsetDeviceHandleKind, FwCfgHandle));

impl ResolveResource<ChipsetDeviceHandleKind, FwCfgHandle> for FwCfgResolver {
    type Output = ResolvedChipsetDevice;
    type Error = Infallible;

    fn resolve(
        &self,
        resource: FwCfgHandle,
        _input: ResolveChipsetDeviceHandleParams<'_>,
    ) -> Result<Self::Output, Self::Error> {
        let dev = FwCfg::new(resource.register_layout);
        Ok(dev.into())
    }
}
