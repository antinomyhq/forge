use std::path::Path;
use std::pin::Pin;

use forge_buffer::ForgeBuffer;
use forge_domain::{Buffer, BufferEvent, JsonlIterator};
use forge_services::BufferService;
use futures::{Stream, StreamExt};

pub struct ForgeBufferService {
    service: ForgeBuffer,
}

impl Default for ForgeBufferService {
    fn default() -> Self {
        Self::new()
    }
}

impl ForgeBufferService {
    pub fn new() -> Self {
        Self { service: ForgeBuffer::new() }
    }
    fn buffer(buffer: Buffer) -> forge_buffer::Buffer {
        forge_buffer::Buffer {
            event: match buffer.event {
                BufferEvent::Input => forge_buffer::BufferEvent::Input,
                BufferEvent::Output => forge_buffer::BufferEvent::Output,
            },
            content: buffer.content,
        }
    }
}

#[async_trait::async_trait]
impl BufferService for ForgeBufferService {
    async fn read(&self, path: &Path) -> anyhow::Result<JsonlIterator> {
        let forge_buffer_iter = self.service.read(path).await?;
        let stream: Pin<Box<dyn Stream<Item = Buffer> + Send + Sync>> = Box::pin(
            futures::stream::unfold(forge_buffer_iter, |mut iter| async move {
                let buffer = StreamExt::next(&mut iter).await.and_then(|v| v.ok())?;

                let domain_buffer = Buffer {
                    event: match buffer.event {
                        forge_buffer::BufferEvent::Input => BufferEvent::Input,
                        forge_buffer::BufferEvent::Output => BufferEvent::Output,
                    },
                    content: buffer.content,
                };
                Some((domain_buffer, iter))
            }),
        );
        Ok(JsonlIterator::new(stream))
    }

    async fn write(&self, path: &Path, buffer: Buffer) -> anyhow::Result<()> {
        self.service.write(path, Self::buffer(buffer)).await
    }
}
