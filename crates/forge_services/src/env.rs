use std::sync::Arc;

use forge_app::domain::Environment;
use forge_app::{EnvironmentInfra, EnvironmentService};

pub struct ForgeEnvironmentService<F>(Arc<F>);

impl<F> ForgeEnvironmentService<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self(infra)
    }
}

impl<F: EnvironmentInfra> EnvironmentService for ForgeEnvironmentService<F> {
    fn get_environment(&self) -> Environment {
        self.0.get_environment()
    }
}
