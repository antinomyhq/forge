use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result};
use reqwest::{Client, Request, Response, StatusCode};
use reqwest_eventsource::{Event, EventSource, RequestBuilderExt};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Mock HTTP client that can record and replay HTTP responses
pub struct MockableHttpClient {
    /// The real HTTP client
    client: Client,
    /// Directory where mock data is stored
    mock_data_dir: Option<PathBuf>,
    /// Whether to record real responses to files
    record_mode: bool,
    /// Cache of loaded mock responses
    response_cache: Arc<Mutex<HashMap<String, MockResponse>>>,
}

/// Mock response data structure for serialization/deserialization
#[derive(Serialize, Deserialize, Clone)]
pub struct MockResponse {
    /// HTTP status code
    pub status: u16,
    /// Response body as bytes
    #[serde(with = "serde_bytes")]
    pub body: Vec<u8>,
    /// Response headers
    pub headers: HashMap<String, String>,
}

impl MockableHttpClient {
    /// Create a new mockable HTTP client
    pub fn new(client: Client, mock_data_dir: Option<PathBuf>, record_mode: bool) -> Self {
        // If mock_data_dir is provided, create the directory if it doesn't exist
        if let Some(ref dir) = mock_data_dir {
            if !dir.exists() {
                if let Err(e) = fs::create_dir_all(dir) {
                    warn!("Failed to create mock data directory: {}", e);
                }
            }
        }
        
        Self {
            client,
            mock_data_dir,
            record_mode,
            response_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    /// Generate a cache key for the given request
    fn cache_key(&self, request: &Request) -> String {
        // Use the method, URL, and a hash of the body as the cache key
        let method = request.method().as_str();
        let url = request.url().to_string();
        
        // Get the request body if available
        let body = request.body().and_then(|body| {
            body.as_bytes().map(|bytes| {
                // Use only the first 100 bytes of the body for the cache key
                // to avoid excessively long filenames
                let truncated = if bytes.len() > 100 {
                    &bytes[..100]
                } else {
                    bytes
                };
                
                // Convert to a string for the cache key
                String::from_utf8_lossy(truncated).to_string()
            })
        }).unwrap_or_default();
        
        // Create a simple hash of the body
        let body_hash = if !body.is_empty() {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            
            let mut hasher = DefaultHasher::new();
            body.hash(&mut hasher);
            format!("-{:x}", hasher.finish())
        } else {
            String::new()
        };
        
        format!("{}-{}{}", method, url.replace("://", "-").replace('/', "_").replace(':', "_"), body_hash)
    }
    
    /// Get the file path for the given cache key
    fn file_path(&self, cache_key: &str) -> Option<PathBuf> {
        self.mock_data_dir.as_ref().map(|dir| {
            // Sanitize the cache key to create a valid filename
            let sanitized = cache_key.chars()
                .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
                .collect::<String>();
            
            dir.join(format!("{}.json", sanitized))
        })
    }
    
    /// Load mock response from file
    fn load_mock_response(&self, cache_key: &str) -> Result<MockResponse> {
        let file_path = self.file_path(cache_key)
            .ok_or_else(|| anyhow::anyhow!("Mock data directory not set"))?;
        
        if !file_path.exists() {
            return Err(anyhow::anyhow!("Mock response file not found: {:?}", file_path));
        }
        
        let file_content = fs::read_to_string(&file_path)
            .with_context(|| format!("Failed to read mock response file: {:?}", file_path))?;
        
        let mock_response: MockResponse = serde_json::from_str(&file_content)
            .with_context(|| format!("Failed to parse mock response file: {:?}", file_path))?;
        
        Ok(mock_response)
    }
    
    /// Save mock response to file
    fn save_mock_response(&self, cache_key: &str, response: &MockResponse) -> Result<()> {
        let file_path = self.file_path(cache_key)
            .ok_or_else(|| anyhow::anyhow!("Mock data directory not set"))?;
        
        let file_content = serde_json::to_string_pretty(response)
            .with_context(|| "Failed to serialize mock response data")?;
        
        fs::write(&file_path, file_content)
            .with_context(|| format!("Failed to write mock response file: {:?}", file_path))?;
        
        Ok(())
    }
    
    /// Convert a real response to a mock response
    async fn response_to_mock(response: Response) -> Result<(MockResponse, Response)> {
        // Clone the response status and headers
        let status = response.status().as_u16();
        
        let mut headers = HashMap::new();
        for (name, value) in response.headers() {
            if let Ok(value_str) = value.to_str() {
                headers.insert(name.to_string(), value_str.to_string());
            }
        }
        
        // Clone the response body
        let bytes = response.bytes().await?;
        let body = bytes.to_vec();
        
        // Create a new response with the same data
        let new_response = Response::from(
            http::Response::builder()
                .status(StatusCode::from_u16(status)?)
                .body(bytes)?
        );
        
        Ok((
            MockResponse { status, body, headers },
            new_response
        ))
    }
    
    /// Execute a request and return the response
    pub async fn execute(&self, request: Request) -> Result<Response> {
        let cache_key = self.cache_key(&request);
        
        // Check if we're in mock mode and not record mode
        if self.mock_data_dir.is_some() && !self.record_mode {
            // Try to get from cache first
            let cached_response = {
                let cache = self.response_cache.lock().unwrap();
                cache.get(&cache_key).cloned()
            };
            
            // If not in cache, try to load from file
            let mock_response = match cached_response {
                Some(response) => {
                    debug!("Using cached mock response for {}", cache_key);
                    response
                },
                None => match self.load_mock_response(&cache_key) {
                    Ok(response) => {
                        debug!("Loaded mock response from file for {}", cache_key);
                        // Update cache
                        {
                            let mut cache = self.response_cache.lock().unwrap();
                            cache.insert(cache_key.clone(), response.clone());
                        }
                        response
                    },
                    Err(e) => {
                        if self.record_mode {
                            warn!("Mock response not found for {}, will record real response: {}", cache_key, e);
                            // Fall through to make a real request
                            return self.execute_real(request, Some(cache_key)).await;
                        } else {
                            return Err(anyhow::anyhow!("Mock response not found for {}: {}", cache_key, e));
                        }
                    }
                }
            };
            
            // Convert the mock response to a real response
            let response = Response::from(
                http::Response::builder()
                    .status(StatusCode::from_u16(mock_response.status)?)
                    .body(mock_response.body)?
            );
            
            Ok(response)
        } else {
            // Make a real request
            self.execute_real(request, if self.record_mode { Some(cache_key) } else { None }).await
        }
    }
    
    /// Execute a real request and optionally save the response
    async fn execute_real(&self, request: Request, cache_key: Option<String>) -> Result<Response> {
        // Make the real request
        let response = self.client.execute(request).await?;
        
        // If we're in record mode, save the response
        if let Some(cache_key) = cache_key {
            let (mock_response, new_response) = Self::response_to_mock(response).await?;
            
            // Update the cache
            {
                let mut cache = self.response_cache.lock().unwrap();
                cache.insert(cache_key.clone(), mock_response.clone());
            }
            
            // Save to file
            if let Err(e) = self.save_mock_response(&cache_key, &mock_response) {
                warn!("Failed to save mock response: {}", e);
            }
            
            Ok(new_response)
        } else {
            Ok(response)
        }
    }
    
    /// Create an event source from a request
    pub fn eventsource(&self, request: Request) -> Result<MockableEventSource> {
        let cache_key = self.cache_key(&request);
        
        // Check if we're in mock mode and not record mode
        if self.mock_data_dir.is_some() && !self.record_mode {
            // Try to get from cache first
            let cached_response = {
                let cache = self.response_cache.lock().unwrap();
                cache.get(&cache_key).cloned()
            };
            
            // If not in cache, try to load from file
            let mock_response = match cached_response {
                Some(response) => {
                    debug!("Using cached mock response for {}", cache_key);
                    response
                },
                None => match self.load_mock_response(&cache_key) {
                    Ok(response) => {
                        debug!("Loaded mock response from file for {}", cache_key);
                        // Update cache
                        {
                            let mut cache = self.response_cache.lock().unwrap();
                            cache.insert(cache_key.clone(), response.clone());
                        }
                        response
                    },
                    Err(e) => {
                        if self.record_mode {
                            warn!("Mock response not found for {}, will record real response: {}", cache_key, e);
                            // Fall through to make a real request
                            return self.eventsource_real(request, Some(cache_key));
                        } else {
                            return Err(anyhow::anyhow!("Mock response not found for {}: {}", cache_key, e));
                        }
                    }
                }
            };
            
            // Create a mock event source
            Ok(MockableEventSource::Mock {
                events: String::from_utf8_lossy(&mock_response.body).to_string(),
                cache_key: cache_key.clone(),
                client: self.clone(),
            })
        } else {
            // Make a real request
            self.eventsource_real(request, if self.record_mode { Some(cache_key) } else { None })
        }
    }
    
    /// Create a real event source and optionally save the events
    fn eventsource_real(&self, request: Request, cache_key: Option<String>) -> Result<MockableEventSource> {
        // Create a real event source
        let es = self.client.execute_request(request).eventsource()?;
        
        Ok(MockableEventSource::Real {
            es,
            events: Vec::new(),
            cache_key,
            client: self.clone(),
        })
    }
}

impl Clone for MockableHttpClient {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            mock_data_dir: self.mock_data_dir.clone(),
            record_mode: self.record_mode,
            response_cache: self.response_cache.clone(),
        }
    }
}

/// A mockable event source that can record and replay events
pub enum MockableEventSource {
    /// A real event source
    Real {
        /// The real event source
        es: EventSource,
        /// Recorded events
        events: Vec<String>,
        /// Cache key for saving events
        cache_key: Option<String>,
        /// The HTTP client
        client: MockableHttpClient,
    },
    /// A mock event source
    Mock {
        /// The events as a string
        events: String,
        /// Cache key for the events
        cache_key: String,
        /// The HTTP client
        client: MockableHttpClient,
    },
}

impl MockableEventSource {
    /// Get the next event from the event source
    pub async fn next(&mut self) -> Option<Result<Event, anyhow::Error>> {
        match self {
            MockableEventSource::Real { es, events, cache_key, client } => {
                // Get the next event from the real event source
                let event = es.next().await;
                
                // If we're in record mode, save the event
                if let Some(event) = &event {
                    if let Ok(event_data) = event {
                        // Convert the event to a string
                        let event_str = format!("{:?}", event_data);
                        events.push(event_str);
                    }
                } else if let Some(cache_key) = cache_key.take() {
                    // End of stream, save the events
                    let events_str = events.join("\n");
                    let mock_response = MockResponse {
                        status: 200,
                        body: events_str.into_bytes(),
                        headers: HashMap::new(),
                    };
                    
                    // Update the cache
                    {
                        let mut cache = client.response_cache.lock().unwrap();
                        cache.insert(cache_key.clone(), mock_response.clone());
                    }
                    
                    // Save to file
                    if let Err(e) = client.save_mock_response(&cache_key, &mock_response) {
                        warn!("Failed to save mock events: {}", e);
                    }
                }
                
                event
            },
            MockableEventSource::Mock { events, .. } => {
                // Parse the next event from the mock events
                if events.is_empty() {
                    None
                } else {
                    // Find the next event boundary
                    let event_end = events.find('\n').unwrap_or(events.len());
                    let event_str = events[..event_end].to_string();
                    
                    // Remove the event from the string
                    *events = if event_end < events.len() {
                        events[event_end + 1..].to_string()
                    } else {
                        String::new()
                    };
                    
                    // Parse the event
                    // This is a simplified implementation - in a real implementation,
                    // you would need to parse the event string into an Event object
                    Some(Ok(Event::Message {
                        data: event_str,
                        event: None,
                        id: None,
                        retry: None,
                    }))
                }
            },
        }
    }
}

/// Module for serde serialization of bytes
mod serde_bytes {
    use serde::{Deserialize, Deserializer, Serializer};
    use serde::de::Error;
    use base64::{Engine as _, engine::general_purpose};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let base64 = general_purpose::STANDARD.encode(bytes);
        serializer.serialize_str(&base64)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let base64 = String::deserialize(deserializer)?;
        general_purpose::STANDARD.decode(base64.as_bytes())
            .map_err(|e| Error::custom(format!("Failed to decode base64: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
    use tempfile::TempDir;
    
    #[tokio::test]
    async fn test_mockable_http_client_basic() {
        // Create a temporary directory for mock data
        let temp_dir = TempDir::new().unwrap();
        
        // Create a real client
        let client = Client::new();
        
        // Create a mockable client in record mode
        let mockable_client = MockableHttpClient::new(
            client.clone(),
            Some(temp_dir.path().to_path_buf()),
            true,
        );
        
        // Create a request
        let request = client.get("https://httpbin.org/get")
            .build()
            .unwrap();
        
        // Execute the request
        let response = mockable_client.execute(request.try_clone().unwrap()).await.unwrap();
        
        // Verify the response
        assert_eq!(response.status(), StatusCode::OK);
        
        // Now create a mockable client in replay mode
        let mockable_client = MockableHttpClient::new(
            client.clone(),
            Some(temp_dir.path().to_path_buf()),
            false,
        );
        
        // Execute the same request
        let response = mockable_client.execute(request).await.unwrap();
        
        // Verify the response
        assert_eq!(response.status(), StatusCode::OK);
    }
    
    #[tokio::test]
    async fn test_mockable_http_client_post() {
        // Create a temporary directory for mock data
        let temp_dir = TempDir::new().unwrap();
        
        // Create a real client
        let client = Client::new();
        
        // Create a mockable client in record mode
        let mockable_client = MockableHttpClient::new(
            client.clone(),
            Some(temp_dir.path().to_path_buf()),
            true,
        );
        
        // Create a request with a body
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        
        let request = client.post("https://httpbin.org/post")
            .headers(headers)
            .body(r#"{"test": "value"}"#)
            .build()
            .unwrap();
        
        // Execute the request
        let response = mockable_client.execute(request.try_clone().unwrap()).await.unwrap();
        
        // Verify the response
        assert_eq!(response.status(), StatusCode::OK);
        
        // Now create a mockable client in replay mode
        let mockable_client = MockableHttpClient::new(
            client.clone(),
            Some(temp_dir.path().to_path_buf()),
            false,
        );
        
        // Execute the same request
        let response = mockable_client.execute(request).await.unwrap();
        
        // Verify the response
        assert_eq!(response.status(), StatusCode::OK);
    }
    
    #[tokio::test]
    async fn test_mockable_http_client_error_when_no_mock_data() {
        // Create a temporary directory for mock data
        let temp_dir = TempDir::new().unwrap();
        
        // Create a real client
        let client = Client::new();
        
        // Create a mockable client in replay mode
        let mockable_client = MockableHttpClient::new(
            client.clone(),
            Some(temp_dir.path().to_path_buf()),
            false,
        );
        
        // Create a request that doesn't have a mock response
        let request = client.get("https://httpbin.org/status/418")
            .build()
            .unwrap();
        
        // Execute the request
        let result = mockable_client.execute(request).await;
        
        // Verify that we get an error
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("Mock response not found"));
    }
}
