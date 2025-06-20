use std::sync::Arc;

use forge_app::ProviderRegistry;
use forge_domain::{ForgeKey, Provider};

use crate::ProviderInfra;

pub struct ForgeProviderRegistry<F> {
    infra: Arc<F>,
}

impl<F: ProviderInfra> ForgeProviderRegistry<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }
}

impl<F: ProviderInfra> ProviderRegistry for ForgeProviderRegistry<F> {
    fn get_provider(&self, forge_key: Option<ForgeKey>) -> Option<Provider> {
        self.infra.get_provider_infra(forge_key)
    }
}
