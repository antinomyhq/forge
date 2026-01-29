use std::sync::Arc;

use forge_app::{FsReadService, ReadChunk, SearchReportOutput, SearchReportService};

/// Service for generating search reports by reading file chunks in parallel
pub struct ForgeSearchReport<F> {
    fs_read_service: Arc<F>,
}

impl<F> ForgeSearchReport<F> {
    pub fn new(fs_read_service: Arc<F>) -> Self {
        Self { fs_read_service }
    }
}

#[async_trait::async_trait]
impl<F: FsReadService + Send + Sync> SearchReportService for ForgeSearchReport<F> {
    async fn generate_report(
        &self,
        chunks: Vec<forge_domain::ChunkSelection>,
    ) -> anyhow::Result<SearchReportOutput> {
        // Read all chunks in parallel using futures
        let read_futures: Vec<_> = chunks
            .into_iter()
            .map(|chunk| {
                let normalized_path = chunk.file_path.display().to_string();
                let start_line = chunk.start_line.map(|s| s as u64);
                let end_line = chunk.end_line.map(|e| e as u64);
                let relevance = chunk.relevance.as_ref().to_string();
                let fs_read_service = self.fs_read_service.clone();

                async move {
                    let read_output = match fs_read_service
                        .read(normalized_path.clone(), start_line, end_line)
                        .await
                    {
                        Ok(output) => output,
                        Err(_) => return Ok::<Option<ReadChunk>, anyhow::Error>(None), // Skip this chunk on error
                    };

                    let content = read_output.content.file_content().to_string();
                    Ok(Some(ReadChunk {
                        file_path: normalized_path,
                        content,
                        start_line: read_output.start_line,
                        end_line: read_output.end_line,
                        relevance,
                    }))
                }
            })
            .collect();

        let chunks = futures::future::try_join_all(read_futures)
            .await?
            .into_iter()
            .flatten()
            .collect();

        Ok(SearchReportOutput { chunks })
    }
}
