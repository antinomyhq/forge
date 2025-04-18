use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use anyhow::Result;
use http::StatusCode;
use reqwest::{
    Client, Method, Request, Response, RequestBuilder,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Configuration for the mock client
#[derive(Debug, Clone)]
pub struct MockClientConfig {
    /// Whether to use cached responses or make real requests
    pub mode: MockMode,
    /// Directory to store cached responses
    pub cache_dir: PathBuf,
    /// Whether to update the cache with new responses
    pub update_cache: bool,
}

/// Mode for the mock client
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MockMode {
    /// Make real requests and cache responses
    Real,
    /// Use cached responses only
    Mock,
}

impl Default for MockClientConfig {
    fn default() -> Self {
        Self {
            mode: MockMode::Real,
            cache_dir: PathBuf::from("tests/fixtures/http_cache"),
            update_cache: false,
        }
    }
}

/// A cached HTTP response
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

/// A mock HTTP client that can cache responses
#[derive(Debug, Clone)]
pub struct MockClient {
    inner: Client,
    config: MockClientConfig,
    cache: Arc<Mutex<HashMap<String, CachedResponse>>>,
}

impl MockClient {
    /// Create a new mock client with the given configuration
    pub fn new(config: MockClientConfig) -> Self {
        // Create cache directory if it doesn't exist
        if !config.cache_dir.exists() {
            fs::create_dir_all(&config.cache_dir).expect("Failed to create cache directory");
        }

        // Load existing cache
        let cache = Self::load_cache(&config.cache_dir);

        Self {
            inner: Client::new(),
            config,
            cache: Arc::new(Mutex::new(cache)),
        }
    }

    /// Load the cache from disk
    fn load_cache(cache_dir: &Path) -> HashMap<String, CachedResponse> {
        let mut cache = HashMap::new();

        if !cache_dir.exists() {
            return cache;
        }

        for entry in fs::read_dir(cache_dir).expect("Failed to read cache directory") {
            let entry = entry.expect("Failed to read cache entry");
            let path = entry.path();

            if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                let mut file = File::open(&path).expect("Failed to open cache file");
                let mut contents = String::new();
                file.read_to_string(&mut contents)
                    .expect("Failed to read cache file");

                let cached_response: CachedResponse =
                    serde_json::from_str(&contents).expect("Failed to parse cache file");

                let key = path
                    .file_stem()
                    .expect("Failed to get file stem")
                    .to_string_lossy()
                    .to_string();
                cache.insert(key, cached_response);
            }
        }

        cache
    }

    /// Save the cache to disk
    fn save_cache(&self, key: &str, response: &CachedResponse) -> Result<()> {
        let file_path = self.config.cache_dir.join(format!("{}.json", key));
        let json = serde_json::to_string_pretty(response)?;
        let mut file = File::create(file_path)?;
        file.write_all(json.as_bytes())?;
        Ok(())
    }
    
    /// Generate a cache key from a request
    fn cache_key(request: &Request) -> String {
        let method = request.method().as_str();
        let url = request.url().to_string();
        let body = request
            .body()
            .and_then(|body| {
                if let Some(bytes) = body.as_bytes() {
                    String::from_utf8(bytes.to_vec()).ok()
                } else {
                    None
                }
            })
            .unwrap_or_default();

        // Create a hash of the request to use as the cache key
        let mut hasher = DefaultHasher::new();
        method.hash(&mut hasher);
        url.hash(&mut hasher);
        body.hash(&mut hasher);
        format!("{}_{}", method, hasher.finish())
    }
    
    /// Execute a request, using the cache if available
    pub async fn execute(&self, request: Request) -> Result<Response> {
        let key = Self::cache_key(&request);
        let url = request.url().clone();

        // Check if we have a cached response
        if self.config.mode == MockMode::Mock {
            let cache = self.cache.lock().unwrap();
            if let Some(cached) = cache.get(&key) {
                debug!("Using cached response for {}", url);
                return self.build_response_from_cache(cached);
            } else {
                warn!(
                    "No cached response found for {} in mock mode",
                    url
                );
                return Err(anyhow::anyhow!(
                    "No cached response found for {} in mock mode",
                    url
                ));
            }
        }

        // Make the real request
        debug!("Making real request to {}", url);
        // We need to clone the request because we can't move it out of self.inner.execute
        let req_clone = match request.try_clone() {
            Some(req) => req,
            None => return Err(anyhow::anyhow!("Failed to clone request")),
        };
        
        let response = self.inner.execute(req_clone).await?;
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.text().await?;

        // Cache the response if needed
        if self.config.update_cache || !self.cache.lock().unwrap().contains_key(&key) {
            let cached_response = CachedResponse {
                status: status.as_u16(),
                headers: headers
                    .iter()
                    .map(|(name, value)| {
                        (
                            name.to_string(),
                            value.to_str().unwrap_or_default().to_string(),
                        )
                    })
                    .collect(),
                body: body.clone(),
            };

            let mut cache = self.cache.lock().unwrap();
            cache.insert(key.clone(), cached_response.clone());
            if let Err(e) = self.save_cache(&key, &cached_response) {
                warn!("Failed to save cache: {}", e);
            } else {
                info!("Cached response for {}", url);
            }
        }

        // Build a new response with the body
        let mut response_builder = http::response::Builder::new().status(status);
        for (name, value) in headers.iter() {
            response_builder = response_builder.header(name.as_str(), value);
        }

        let http_response = response_builder
            .body(body.clone())
            .map_err(|e| anyhow::anyhow!("Failed to build response: {}", e))?;

        // Convert http::Response to reqwest::Response
        let response = Response::from(http_response);
        Ok(response)
    }

    /// Build a response from a cached response
    fn build_response_from_cache(&self, cached: &CachedResponse) -> Result<Response> {
        let status = StatusCode::from_u16(cached.status)?;
        let mut response_builder = http::response::Builder::new().status(status);

        for (name, value) in &cached.headers {
            response_builder = response_builder.header(name, value);
        }

        let http_response = response_builder
            .body(cached.body.clone())
            .map_err(|e| anyhow::anyhow!("Failed to build response: {}", e))?;

        // Convert http::Response to reqwest::Response
        let response = Response::from(http_response);
        Ok(response)
    }
    
    /// Create a request builder
    pub fn request(&self, method: Method, url: &str) -> RequestBuilder {
        self.inner.request(method, url)
    }

    /// Create a GET request
    pub fn get(&self, url: &str) -> RequestBuilder {
        self.request(Method::GET, url)
    }

    /// Create a POST request
    pub fn post(&self, url: &str) -> RequestBuilder {
        self.request(Method::POST, url)
    }
}

// Implement From<MockClient> for reqwest::Client
impl From<MockClient> for reqwest::Client {
    fn from(client: MockClient) -> Self {
        // For now, we'll just return the inner client
        // In a real implementation, we would need to intercept all requests
        client.inner.clone()
    }
}