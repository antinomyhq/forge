use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::Stream;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::{Buffer, ForgeBuffer, JsonlIterator};

impl JsonlIterator {
    fn new(file: File) -> Self {
        let reader = BufReader::new(file);
        Self { lines: reader.lines() }
    }
}

impl Stream for JsonlIterator {
    type Item = anyhow::Result<Buffer>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.lines).poll_next_line(cx) {
            Poll::Ready(Ok(Some(line))) => {
                let line = line.trim();
                if line.is_empty() {
                    // Skip empty lines and recursively poll for next
                    self.poll_next(cx)
                } else {
                    match serde_json::from_str::<Buffer>(line) {
                        Ok(buffer) => Poll::Ready(Some(Ok(buffer))),
                        Err(e) => Poll::Ready(Some(Err(anyhow::anyhow!(
                            "Failed to parse JSON line: {}",
                            e
                        )))),
                    }
                }
            }
            Poll::Ready(Ok(None)) => Poll::Ready(None), // EOF
            Poll::Ready(Err(e)) => Poll::Ready(Some(Err(anyhow::anyhow!("IO error: {}", e)))),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl ForgeBuffer {
    /// Returns an iterator that yields Buffer items from JSONL format.
    /// Uses the file path from the ForgeBuffer instance to open a read handle.
    pub async fn read(&self, path: &Path) -> anyhow::Result<JsonlIterator> {
        let file = File::open(&path).await?;
        Ok(JsonlIterator::new(file))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use futures::StreamExt;
    use pretty_assertions::assert_eq;
    use tempfile::NamedTempFile;
    use tokio::io::AsyncWriteExt;

    use super::*;
    use crate::{Buffer, BufferEvent};

    struct TempFile {
        #[allow(dead_code)]
        // just used to store file to avoid dropping it
        named_temp_file: NamedTempFile,
        path: PathBuf,
    }

    async fn create_test_jsonl() -> anyhow::Result<TempFile> {
        let temp_file = NamedTempFile::new()?;
        let temp_path = temp_file.path().to_path_buf();

        let test_data = vec![
            Buffer {
                event: BufferEvent::Input,
                content: "Hello World".to_string(),
            },
            Buffer {
                event: BufferEvent::Output,
                content: "Test Content".to_string(),
            },
        ];

        let mut file = File::create(&temp_path).await?;
        for buffer in test_data {
            let mut line = serde_json::to_string(&buffer)?;
            line.push('\n');
            file.write_all(line.as_bytes()).await?;
        }
        file.flush().await?;

        Ok(TempFile { named_temp_file: temp_file, path: temp_path })
    }

    #[tokio::test]
    async fn test_read_jsonl() {
        let fixture_path = create_test_jsonl().await.unwrap();
        let forge_buffer = ForgeBuffer::new();
        let mut actual = forge_buffer.read(&fixture_path.path).await.unwrap();

        let first = actual.next().await.unwrap().unwrap();
        let expected_first = Buffer {
            event: BufferEvent::Input,
            content: "Hello World".to_string(),
        };
        assert_eq!(first, expected_first);

        let second = actual.next().await.unwrap().unwrap();
        let expected_second = Buffer {
            event: BufferEvent::Output,
            content: "Test Content".to_string(),
        };
        assert_eq!(second, expected_second);

        let third = actual.next().await;
        assert!(third.is_none());
    }
}
