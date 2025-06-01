use std::path::Path;

use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;

use crate::{Buffer, ForgeBuffer};

impl ForgeBuffer {
    async fn file(&self, path: &Path) -> anyhow::Result<File> {
        Ok(OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .await?)
    }
    pub async fn write(&self, path: &Path, buffer: Buffer) -> anyhow::Result<()> {
        let mut string = serde_json::to_string(&buffer)?;
        string.push('\n');
        self.file(path).await?.write_all(string.as_bytes()).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use pretty_assertions::assert_eq;
    use tempfile::NamedTempFile;
    use tokio::fs;
    use tokio::io::AsyncReadExt;

    use super::*;
    use crate::{Buffer, BufferEvent};

    struct TempFile {
        #[allow(dead_code)]
        // just used to store file to avoid dropping it
        named_temp_file: NamedTempFile,
        path: PathBuf,
    }

    async fn create_temp_file() -> anyhow::Result<TempFile> {
        let temp_file = NamedTempFile::new()?;
        let temp_path = temp_file.path().to_path_buf();

        Ok(TempFile { named_temp_file: temp_file, path: temp_path })
    }

    #[tokio::test]
    async fn test_write_single_buffer() {
        let fixture_path = create_temp_file().await.unwrap();
        let forge_buffer = ForgeBuffer::new();

        let buffer = Buffer {
            event: BufferEvent::Input,
            content: "Hello World".to_string(),
        };

        let actual = forge_buffer.write(&fixture_path.path, buffer.clone()).await;
        assert!(actual.is_ok());

        // Verify the content was written
        let mut file_content = String::new();
        let mut file = fs::File::open(&fixture_path.path).await.unwrap();
        file.read_to_string(&mut file_content).await.unwrap();

        let expected = format!("{}{}", serde_json::to_string(&buffer).unwrap(), "\n");
        assert_eq!(file_content, expected);
    }

    #[tokio::test]
    async fn test_write_append_mode() {
        let fixture_path = create_temp_file().await.unwrap();

        // Create first ForgeBuffer instance and write
        let first_buffer = ForgeBuffer::new();
        let first_entry = Buffer {
            event: BufferEvent::Input,
            content: "Initial Entry".to_string(),
        };
        first_buffer
            .write(&fixture_path.path, first_entry.clone())
            .await
            .unwrap();

        // Create second ForgeBuffer instance for same file and write
        let mut second_buffer = ForgeBuffer::new();
        let second_entry = Buffer {
            event: BufferEvent::Output,
            content: "Appended Entry".to_string(),
        };
        second_buffer
            .write(&fixture_path.path, second_entry.clone())
            .await
            .unwrap();

        // Verify both entries exist
        let mut file_content = String::new();
        let mut file = File::open(&fixture_path.path).await.unwrap();
        file.read_to_string(&mut file_content).await.unwrap();

        let expected = format!(
            "{}\n{}\n",
            serde_json::to_string(&first_entry).unwrap(),
            serde_json::to_string(&second_entry).unwrap(),
        );
        assert_eq!(file_content, expected);
    }
}
