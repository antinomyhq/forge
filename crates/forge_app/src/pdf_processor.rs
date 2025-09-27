use anyhow::{Context, Result};
use lopdf::Document;
use pdf_extract::extract_text;

pub struct PdfProcessor;

impl PdfProcessor {
    /// Extract text from a PDF file with context limits
    pub async fn extract_text_with_limits(
        pdf_data: &[u8],
        max_text_length: usize,
        max_pages: u32,
    ) -> Result<(String, u32, u32, usize)> {
        // Load PDF document
        let doc = Document::load_mem(pdf_data).context("Failed to load PDF document")?;

        let total_pages = doc.get_pages().len() as u32;
        let pages_to_extract = total_pages.min(max_pages);

        // Extract text from PDF - need to write to temp file first
        let temp_dir = tempfile::TempDir::new()?;
        let temp_path = temp_dir.path().join("temp.pdf");
        std::fs::write(&temp_path, pdf_data)?;

        // Note: pdf-extract may print "Unicode mismatch" messages to stderr during
        // processing. These are informational messages about Unicode ligature
        // characters (like ﬁ, ﬂ) and don't affect functionality. They're
        // expected behavior for PDFs with ligatures.
        let extracted_text = extract_text(&temp_path).context("Failed to extract text from PDF")?;

        // Apply text length limit
        let final_text = if extracted_text.len() > max_text_length {
            // Try to truncate at word boundaries
            let mut truncated = extracted_text
                .chars()
                .take(max_text_length)
                .collect::<String>();

            // Remove incomplete last word
            if let Some(last_space) = truncated.rfind(' ') {
                truncated.truncate(last_space);
            }

            format!(
                "{}... [TRUNCATED - text exceeded limit of {} characters]",
                truncated, max_text_length
            )
        } else {
            extracted_text.clone()
        };

        let text_length = final_text.len();
        Ok((final_text, total_pages, pages_to_extract, text_length))
    }

    /// Check if a file is a PDF based on its magic bytes
    pub fn is_pdf(file_data: &[u8]) -> bool {
        file_data.len() >= 4
            && file_data[0] == b'%'
            && file_data[1] == b'P'
            && file_data[2] == b'D'
            && file_data[3] == b'F'
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_is_pdf() {
        // Valid PDF header
        let pdf_header = b"%PDF-1.4";
        assert!(PdfProcessor::is_pdf(pdf_header));

        // Invalid PDF header
        let not_pdf = b"Not a PDF";
        assert!(!PdfProcessor::is_pdf(not_pdf));

        // Too short to be PDF
        let too_short = b"%PD";
        assert!(!PdfProcessor::is_pdf(too_short));
    }
}
