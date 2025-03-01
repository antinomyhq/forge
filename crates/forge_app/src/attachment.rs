use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use base64::Engine;
use forge_domain::{Attachment, AttachmentService, ContentType, ImageType};

use crate::{FileReadService, Infrastructure};
// TODO: bring pdf support, pdf is just a collection of images.

pub struct ForgeChatRequest<F> {
    infra: Arc<F>,
}

impl<F: Infrastructure> ForgeChatRequest<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }

    async fn prepare_attachments<T: AsRef<Path>>(&self, paths: HashSet<T>) -> HashSet<Attachment> {
        futures::future::join_all(
            paths
                .into_iter()
                .map(|v| v.as_ref().to_path_buf())
                .map(|v| self.populate_attachments(v)),
        )
        .await
        .into_iter()
        .filter_map(|v| v.ok())
        .collect::<HashSet<_>>()
    }

    fn prepare_message(
        &self,
        mut message: String,
        attachments: &mut HashSet<Attachment>,
    ) -> String {
        for attachment in attachments.clone() {
            if let ContentType::Text = &attachment.content_type {
                let xml = format!(
                    "<file path=\"{}\">{}</file>",
                    attachment.path, attachment.content
                );
                message.push_str(&xml);

                attachments.remove(&attachment);
            }
        }

        message
    }
    async fn populate_attachments(&self, v: PathBuf) -> anyhow::Result<Attachment> {
        let path = v.to_string_lossy().to_string();
        let ext = v.extension().map(|v| v.to_string_lossy().to_string());
        let read = self.infra.file_read_service().read(v.as_path()).await?;
        if let Some(extension) = ext.as_ref().and_then(|v| ImageType::from_str(v).ok()) {
            Ok(Attachment {
                content: base64::engine::general_purpose::STANDARD.encode(read),
                path,
                content_type: ContentType::Image(extension),
            })
        } else {
            Ok(Attachment {
                content: String::from_utf8(read.to_vec())?,
                path,
                content_type: ContentType::Text,
            })
        }
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure> AttachmentService for ForgeChatRequest<F> {
    async fn attachments(&self, chat: String) -> anyhow::Result<(String, HashSet<Attachment>)> {
        let mut attachments = self.prepare_attachments(Attachment::parse_all(&chat)).await;

        Ok((self.prepare_message(chat, &mut attachments), attachments))
    }
}
