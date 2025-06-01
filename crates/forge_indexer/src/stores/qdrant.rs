use std::sync::Arc;

use async_trait::async_trait;
use futures::future::join_all;
use qdrant_client::Payload;
use qdrant_client::qdrant::{
    Condition, DeleteCollection, FieldCondition, Filter, Match, PointStruct, RepeatedStrings,
    SearchPointsBuilder, UpsertPointsBuilder, condition, r#match,
};
use tracing::info;
use uuid::Uuid;

use super::{QueryOptions, QueryOutput, Store, StoreInput};

// Implement conversion from QueryOptions to Qdrant Filter
impl From<&QueryOptions> for Option<Filter> {
    fn from(options: &QueryOptions) -> Self {
        let mut filter_conditions = Vec::new();

        // Add kind filter if specified
        if let Some(kind) = &options.kind {
            let field_condition = FieldCondition {
                key: "kind".to_string(),
                r#match: Some(Match {
                    match_value: Some(r#match::MatchValue::Keyword(kind.clone())),
                }),
                ..Default::default()
            };

            let mut condition = Condition::default();
            condition.condition_one_of = Some(condition::ConditionOneOf::Field(field_condition));

            filter_conditions.push(condition);
        }

        // Add path filter if specified
        let paths = options.path.clone().unwrap_or_default();
        if !paths.is_empty() {
            // Since we're only dealing with absolute paths without glob patterns,
            // we can use the Keywords match type directly
            let field_condition = FieldCondition {
                key: "path".to_string(),
                r#match: Some(Match {
                    match_value: Some(r#match::MatchValue::Keywords(RepeatedStrings {
                        strings: paths,
                    })),
                }),
                ..Default::default()
            };

            let mut condition = Condition::default();
            condition.condition_one_of = Some(condition::ConditionOneOf::Field(field_condition));

            filter_conditions.push(condition);
        }

        if filter_conditions.is_empty() {
            None
        } else {
            Some(Filter { must: filter_conditions, ..Default::default() })
        }
    }
}

#[derive(Clone)]
pub struct QdrantStore {
    client: Arc<qdrant_client::Qdrant>,
    collection_name: String,
}

impl QdrantStore {
    pub fn try_new<S: Into<String>, U: AsRef<str>>(
        url: U,
        api_key: S,
        collection_name: S,
    ) -> anyhow::Result<Self> {
        let client = Arc::new(
            qdrant_client::Qdrant::from_url(url.as_ref())
                .api_key(api_key.into())
                .skip_compatibility_check()
                .build()?,
        );

        Ok(Self { collection_name: collection_name.into(), client })
    }
}

#[async_trait]
impl Store for QdrantStore {
    async fn store<T>(&self, inputs: Vec<StoreInput<T>>) -> anyhow::Result<()>
    where
        T: Into<serde_json::Value> + Send + Sync,
    {
        info!("Storing embeddings in Qdrant");
        let mut points = Vec::with_capacity(inputs.len());
        for input in inputs {
            let vector = input.embeddings;
            let id = Uuid::new_v4().to_string();
            let payload: Payload = input.metadata.into().try_into()?;
            let point = PointStruct::new(id, vector, payload);
            points.push(point);
        }

        // Process batches in parallel
        let futures = points.chunks(40).map(|batch| {
            let client = Arc::clone(&self.client);
            let collection_name = self.collection_name.clone();
            async move {
                client
                    .upsert_points(UpsertPointsBuilder::new(collection_name, batch))
                    .await
            }
        });

        // Join all futures and collect results
        let _results = join_all(futures).await;

        info!("Stored embeddings in Qdrant");

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
        info!("Querying Qdrant with embeddings");
        let mut search_builder =
            SearchPointsBuilder::new(self.collection_name.clone(), query, options.limit);

        // Apply filters based on QueryOptions using the From implementation
        if let Some(filter) = Option::<Filter>::from(&options) {
            search_builder = search_builder.filter(filter);
        }

        let search_request = search_builder.build();
        let output = self.client.search_points(search_request).await?.result;

        let results = output
            .into_iter()
            .map(|point| {
                let score = point.score;
                // TODO: figure out better way to do this.
                let payload = serde_json::value::to_value(&point.payload)
                    .and_then(|value| serde_json::from_value::<T>(value))?;
                Ok::<QueryOutput<T>, anyhow::Error>(QueryOutput { score, payload })
            })
            .collect::<Result<Vec<_>, _>>()?;

        info!("Retrieved {} results from Qdrant", results.len());

        Ok(results)
    }

    async fn reset(&self) -> anyhow::Result<()> {
        let _ = self
            .client
            .delete_collection(DeleteCollection {
                collection_name: self.collection_name.clone(),
                timeout: None,
            })
            .await?;
        Ok(())
    }
}
