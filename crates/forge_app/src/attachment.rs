use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use base64::Engine;
use forge_domain::{Attachment, AttachmentService, ContentType};
// TODO: bring pdf support, pdf is just a collection of images.

pub struct ForgeChatRequest;

#[async_trait::async_trait]
impl AttachmentService for ForgeChatRequest {
    async fn attachments(&self, content: String) -> anyhow::Result<(String, HashSet<Attachment>)> {
        Ok(handle_binary_attachments(content).await)
    }
}

#[derive(Debug, strum_macros::EnumIter, strum_macros::Display, strum_macros::EnumString)]
pub enum ImageTypes {
    #[strum(
        to_string = "jpeg",
        serialize = "jpeg",
        serialize = "jpg",
        ascii_case_insensitive
    )]
    Jpeg,
    #[strum(to_string = "png", ascii_case_insensitive)]
    Png,
    #[strum(
        to_string = "webp",
        serialize = "gif",
        serialize = "webp",
        ascii_case_insensitive
    )]
    Webp,
}
async fn populate_attachments(v: PathBuf) -> anyhow::Result<Attachment> {
    let path = v.to_string_lossy().to_string();
    let ext = v.extension().map(|v| v.to_string_lossy().to_string());
    let read = tokio::fs::read(v).await?;
    if let Some(extension) = ext.as_ref().and_then(|v| ImageTypes::from_str(v).ok()) {
        let b64 = format!(
            "data:image/{};base64,{}",
            extension,
            base64::engine::general_purpose::STANDARD.encode(read)
        );
        Ok(Attachment { content: b64, path, content_type: ContentType::ImageURL })
    } else {
        Ok(Attachment {
            content: String::from_utf8(read)?,
            path,
            content_type: ContentType::Text,
        })
    }
}

async fn prepare_attachments<T: AsRef<Path>>(paths: Vec<T>) -> HashSet<Attachment> {
    futures::future::join_all(
        paths
            .into_iter()
            .map(|v| v.as_ref().to_path_buf())
            .map(populate_attachments),
    )
    .await
    .into_iter()
    .filter_map(|v| v.ok())
    .collect::<HashSet<_>>()
}

fn prepare_message(mut message: String, attachments: &mut HashSet<Attachment>) -> String {
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

pub async fn handle_binary_attachments<T: ToString>(v: T) -> (String, HashSet<Attachment>) {
    let chat = v.to_string();
    let words = chat
        .split(" ")
        .filter_map(|v| v.strip_prefix("@").map(String::from))
        .collect::<Vec<_>>();

    let mut attachments = prepare_attachments(words).await;

    (prepare_message(chat, &mut attachments), attachments)
}
