use forge_gateway::config::Config;
use forge_gateway::ForgeGateway;
use shuttle_runtime::SecretStore;
use tower_http::cors::{Any, CorsLayer};

#[shuttle_runtime::main]
async fn main(#[shuttle_runtime::Secrets] secret_store: SecretStore) -> shuttle_axum::ShuttleAxum {
    // load the secrets into the present environment.
    secret_store.into_iter().for_each(|(key, val)| {
        std::env::set_var(key, val);
    });

    let gateway_config = Config::from_env()?;
    let router = ForgeGateway::init(gateway_config);

    // Create CORS layer
    let cors = CorsLayer::new()
        .allow_methods(Any)
        .allow_headers(Any)
        .allow_origin(Any);

    Ok(router.layer(cors).into())
}
