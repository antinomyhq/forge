use forge_domain::Provider;
use forge_services::ProviderService;

type ProviderSearch = (&'static str, Box<dyn FnOnce(&str) -> Provider>);

#[derive(Clone)]
pub struct ForgeProviderService;

impl ProviderService for ForgeProviderService {
    fn get(&self, key: &str) -> Provider {
        resolve_env_provider().unwrap_or(Provider::antinomy(key))
    }
}

// For backwards compatibility
fn resolve_env_provider() -> Option<Provider> {
    let keys: [ProviderSearch; 4] = [
        ("FORGE_KEY", Box::new(Provider::antinomy)),
        ("OPENROUTER_API_KEY", Box::new(Provider::open_router)),
        ("OPENAI_API_KEY", Box::new(Provider::openai)),
        ("ANTHROPIC_API_KEY", Box::new(Provider::anthropic)),
    ];

    keys.into_iter().find_map(|(key, fun)| {
        std::env::var(key).ok().map(|key| {
            let mut provider = fun(&key);

            if let Ok(url) = std::env::var("OPENAI_URL") {
                provider.open_ai_url(url);
            }

            // Check for Anthropic URL override
            if let Ok(url) = std::env::var("ANTHROPIC_URL") {
                provider.anthropic_url(url);
            }

            provider
        })
    })
}
