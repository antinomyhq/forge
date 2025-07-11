use crate::Provider;

#[derive(Clone)]
pub struct User {
    pub auth_provider_id: Option<String>,
    pub provider: Provider,
    pub is_tracked: bool,
}

impl User {
    pub fn into_provider(self) -> Provider {
        self.provider
    }
}
