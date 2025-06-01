use std::collections::HashMap;
use std::sync::RwLock;

use glob::Pattern;
use hnsw_rs::prelude::*;
use serde_json::Value;
use tracing::info;

use super::{QueryOptions, QueryOutput, Store, StoreInput};

// Custom filter struct that implements the FilterT trait
struct CustomFilter<'a> {
    payloads: &'a HashMap<usize, Value>,
    kind_filter: Option<String>,
    path_filter: Option<Vec<String>>,
}

impl<'a> FilterT for CustomFilter<'a> {
    fn hnsw_filter(&self, id: &usize) -> bool {
        // Get the payload for this ID
        if let Some(payload) = self.payloads.get(id) {
            // Check kind filter if specified
            if let Some(ref kind) = self.kind_filter {
                if let Some(payload_kind) = payload.get("kind") {
                    if payload_kind.as_str() != Some(kind) {
                        return false;
                    }
                } else {
                    return false;
                }
            }

            // Check path filter if specified
            if let Some(ref paths) = self.path_filter {
                if let Some(payload_path) = payload.get("path") {
                    let path_str = payload_path.as_str().unwrap_or("");

                    // Check if the path matches any of the specified paths
                    let matches = paths.iter().any(|filter_path| {
                        // Use glob crate for proper pattern matching
                        match Pattern::new(filter_path) {
                            Ok(pattern) => pattern.matches(path_str),
                            Err(_) => {
                                // If pattern is invalid, fall back to prefix matching
                                path_str.starts_with(filter_path)
                            }
                        }
                    });

                    if !matches {
                        return false;
                    }
                } else {
                    return false;
                }
            }

            true
        } else {
            false
        }
    }
}

impl<'a> CustomFilter<'a> {
    // Helper function to create a filter from QueryOptions
    fn from_options(
        options: &QueryOptions,
        payloads: &'a HashMap<usize, Value>,
    ) -> Option<CustomFilter<'a>> {
        if options.kind.is_none() && options.path.is_none() {
            return None;
        }

        Some(CustomFilter {
            payloads,
            kind_filter: options.kind.clone(),
            path_filter: options.path.clone(),
        })
    }
}

pub struct HnswStore<'a> {
    hnsw: RwLock<Hnsw<'a, f32, DistCosine>>,
    payloads: RwLock<HashMap<usize, Value>>,
    id: RwLock<usize>,
    dimension: usize,
}

impl<'a> HnswStore<'a> {
    pub fn new(dimension: usize) -> Self {
        // HNSW parameters
        let max_nb_connection = 16;
        let nb_layer = 16.min((dimension as f32).ln().floor() as usize);
        let ef_construction = 200;

        // Create HNSW index
        let hnsw = Hnsw::new(
            max_nb_connection,
            dimension,
            nb_layer,
            ef_construction,
            DistCosine,
        );

        Self {
            hnsw: RwLock::new(hnsw),
            payloads: RwLock::new(HashMap::new()),
            id: RwLock::new(0),
            dimension,
        }
    }

    fn get_next_id(&self) -> usize {
        let mut id = self.id.write().unwrap();
        let new_id = *id;
        *id += 1;
        new_id
    }
}

#[async_trait::async_trait]
impl Store for HnswStore<'_> {
    async fn store<T>(&self, inputs: Vec<StoreInput<T>>) -> anyhow::Result<()>
    where
        T: Into<serde_json::Value> + Send + Sync,
    {
        info!("Storing embeddings in In-memory");
        let mut payloads = self.payloads.write().unwrap();

        let inputs_with_identifier = inputs
            .into_iter()
            .map(|input| {
                let id = self.get_next_id();
                (
                    id,
                    StoreInput {
                        embeddings: input.embeddings,
                        metadata: input.metadata.into(),
                    },
                )
            })
            .collect::<Vec<(usize, StoreInput<serde_json::Value>)>>();

        // First collect all payloads
        for (id, input) in &inputs_with_identifier {
            if input.embeddings.len() != self.dimension {
                return Err(anyhow::anyhow!(
                    "Vector dimension mismatch: expected {}, got {}",
                    self.dimension,
                    input.embeddings.len()
                ));
            }
            payloads.insert(*id, input.metadata.clone());
        }

        // Prepare data for parallel insertion
        let data: Vec<_> = inputs_with_identifier
            .into_iter()
            .map(|(id, input)| (input.embeddings, id))
            .collect();

        // Convert to vector of references for parallel_insert
        let data_refs: Vec<_> = data.iter().map(|(vec, id)| (vec, *id)).collect();

        // Insert all vectors in parallel
        let hnsw = self.hnsw.read().unwrap();
        hnsw.parallel_insert(&data_refs);

        info!("Stored embeddings in In-memory");

        Ok(())
    }
    async fn query<T>(
        &self,
        query: Vec<f32>,
        options: QueryOptions,
    ) -> anyhow::Result<Vec<QueryOutput<T>>>
    where
        T: serde::de::DeserializeOwned + Send + Sync,
    {
        if query.len() != self.dimension {
            return Err(anyhow::anyhow!(
                "Vector dimension mismatch: expected {}, got {}",
                self.dimension,
                query.len()
            ));
        }

        info!("Querying in-memory embeddings with options: {:#?}", options);

        let hnsw = self.hnsw.read().unwrap();
        let payloads = self.payloads.read().unwrap();
        let limit = options.limit;

        // Search for nearest neighbors
        // Using a fixed ef_search value of 100 (should be >= limit)
        let ef_search = std::cmp::max(100, limit * 2);

        // Create a filter using the helper function
        let filter = CustomFilter::from_options(&options, &payloads);

        // Search with filter if applicable
        let nearest = if let Some(filter) = filter {
            hnsw.search_filter(&query, limit as usize, ef_search as usize, Some(&filter))
        } else {
            hnsw.search(&query, limit as usize, ef_search as usize)
        };

        // Convert to SearchResult format
        let results = nearest
            .iter()
            .filter_map(|neighbor| {
                payloads.get(&neighbor.d_id).cloned().and_then(|payload| {
                    match serde_json::from_value::<T>(payload) {
                        Ok(deserialized_payload) => Some(QueryOutput {
                            score: 1.0 - neighbor.distance,
                            payload: deserialized_payload,
                        }),
                        Err(_) => None,
                    }
                })
            })
            .collect::<Vec<_>>();

        info!("Retrieved {} results from in-memory store", results.len());

        Ok(results)
    }

    async fn reset(&self) -> anyhow::Result<()> {
        // Clear payloads
        let mut payloads = self.payloads.write().unwrap();
        payloads.clear();

        // Reset HNSW index by replacing it with a new instance
        let mut hnsw = self.hnsw.write().unwrap();

        // Create a new HNSW index with the same parameters
        let max_nb_connection = 16;
        let nb_layer = 16.min((self.dimension as f32).ln().floor() as usize);
        let ef_construction = 200;

        // Replace the existing HNSW index with a new one
        *hnsw = Hnsw::new(
            max_nb_connection,
            self.dimension,
            nb_layer,
            ef_construction,
            DistCosine,
        );

        // Reset the ID counter
        let mut id = self.id.write().unwrap();
        *id = 0;

        Ok(())
    }
}
