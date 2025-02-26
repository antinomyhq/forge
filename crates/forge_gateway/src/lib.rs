pub mod config;
mod data;
mod error;
mod presentation;
mod service;

use std::sync::Arc;

use axum::Router;
use clerk_rs::clerk::Clerk;
use clerk_rs::validators::authorizer::ClerkAuthorizer;
use clerk_rs::ClerkConfiguration;
use config::Config;
use data::*;
use error::{Error, Result};
use forge_open_router::ProviderBuilder;
use postgrest::Postgrest;
use presentation::*;
use service::*;

pub struct ForgeGateway;

impl ForgeGateway {
    pub fn init(config: Config) -> Router {
        // Setup provider
        let provider = ProviderBuilder::from_url(config.provider_url.clone())
            .with_key(config.provider_key.clone())
            .build()
            .expect("Failed to build provider");

        // Initialize Supabase client
        let client =
            Postgrest::new(&config.supabase_url).insert_header("apikey", &config.supabase_key);

        // Initialize dependencies
        let key_gen_service = Arc::new(KeyGeneratorServiceImpl::new(
            config.api_key_length as usize,
            &config.api_key_prefix,
        ));
        let api_key_repository_impl = Arc::new(ApiKeyRepositoryImpl::new(client));
        let api_key_service =
            Arc::new(ApiKeyService::new(api_key_repository_impl, key_gen_service));
        let proxy_service = Arc::new(ProxyService::new(provider));

        // Initialize authorization dependencies auth
        let clerk_config =
            ClerkConfiguration::new(None, None, Some(config.clerk_secret_key.clone()), None);
        let auth_service = Arc::new(ClerkAuthorizeService::new(ClerkAuthorizer::new(
            Clerk::new(clerk_config),
            true,
        )));

        app(api_key_service, proxy_service, auth_service)
    }
}
