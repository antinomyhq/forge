use std::sync::Arc;

use forge_app::ProviderService;
use forge_domain::{ForgeKey, Provider};

use crate::ProviderInfra;

pub struct ForgeProviderService<F> {
    infra: Arc<F>,
}

impl<F: ProviderInfra> ForgeProviderService<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }
}

impl<F: ProviderInfra> ProviderService for ForgeProviderService<F> {
    fn get_provider(&self, forge_key: Option<ForgeKey>) -> Option<Provider> {
        self.infra.get_provider_infra(forge_key)
    }
}
