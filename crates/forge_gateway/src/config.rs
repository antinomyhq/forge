use serde::Deserialize;

/// Configuration for the application
#[derive(Debug, Deserialize)]
pub struct Config {
    /// Provider API key
    pub provider_key: String,
    /// Provider Url
    pub provider_url: String,
    /// Supabase URL
    pub supabase_url: String,
    /// Supabase API key
    pub supabase_key: String,
    /// Clerk secret key for authentication
    pub clerk_secret_key: String,
    /// Server host
    pub host: String,
    /// Server port
    pub port: u16,
    /// Api key prefix
    pub api_key_prefix: String,
    /// APi key length
    pub api_key_length: u16,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let provider = forge_domain::Provider::from_env()
            .ok_or_else(|| anyhow::anyhow!("Failed to load provider"))?;
        let provider_url = provider.to_base_url().to_string();
        let provider_key = provider.get_key();

        Ok(Self {
            provider_key,
            provider_url,
            supabase_url: std::env::var("SUPABASE_URL")
                .map_err(|_| anyhow::anyhow!("SUPABASE_URL is required"))?,
            supabase_key: std::env::var("SUPABASE_KEY")
                .map_err(|_| anyhow::anyhow!("SUPABASE_KEY is required"))?,
            clerk_secret_key: std::env::var("CLERK_SECRET_KEY")
                .map_err(|_| anyhow::anyhow!("CLERK_SECRET_KEY is required"))?,
            host: std::env::var("HOST").map_err(|_| anyhow::anyhow!("HOST is required"))?,
            port: std::env::var("PORT")
                .map_err(|_| anyhow::anyhow!("PORT is required"))?
                .parse()
                .map_err(|_| anyhow::anyhow!("PORT must be a number"))?,
            api_key_length: std::env::var("API_KEY_LENGTH")
                .map_err(|_| anyhow::anyhow!("API_KEY_LENGTH is required"))?
                .parse()
                .map_err(|_| anyhow::anyhow!("API_KEY_LENGTH must be a number"))?,
            api_key_prefix: std::env::var("API_KEY_PREFIX")
                .map_err(|_| anyhow::anyhow!("API_KEY_PREFIX is required"))?,
        })
    }

    /// Returns the server address
    pub fn server_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}
