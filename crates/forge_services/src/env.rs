use std::sync::Arc;

use forge_app::domain::Config;
use forge_app::{EnvironmentInfra, EnvironmentService};

pub struct ForgeEnvironmentService<F>(Arc<F>);

impl<F> ForgeEnvironmentService<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self(infra)
    }
}

impl<F: EnvironmentInfra> EnvironmentService for ForgeEnvironmentService<F> {
    fn get_environment(&self) -> Config {
        self.0.get_environment()
    }
}
