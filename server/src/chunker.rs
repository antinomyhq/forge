/// Result of chunking a single file.
///
/// Each chunk preserves the original file path and tracks its exact
/// line range (1-based, inclusive) within the source file.
#[derive(Debug, Clone)]
pub struct ChunkResult {
    /// Original file path
    pub path: String,
    /// Chunk content
    pub content: String,
    /// Start line in the source file (1-based, inclusive)
    pub start_line: u32,
    /// End line in the source file (1-based, inclusive)
    pub end_line: u32,
}

/// Splits a file into line-aware chunks suitable for embedding.
///
/// # Arguments
/// * `path` - File path (preserved in each chunk for identification)
/// * `content` - Full file content
/// * `min_size` - Minimum chunk size in bytes; the last chunk is merged if smaller
/// * `max_size` - Maximum chunk size in bytes; chunks split at line boundaries
///
/// # Returns
/// A vector of chunks with accurate line numbers. Empty files produce 0 chunks.
pub fn chunk_file(path: &str, content: &str, min_size: u32, max_size: u32) -> Vec<ChunkResult> {
    if content.is_empty() {
        return vec![];
    }

    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return vec![];
    }

    let mut chunks: Vec<ChunkResult> = Vec::new();
    let mut chunk_lines: Vec<&str> = Vec::new();
    let mut chunk_bytes: usize = 0;
    let mut chunk_start_line: u32 = 1;

    for (i, line) in lines.iter().enumerate() {
        let line_bytes = line.len() + 1; // +1 for newline

        // If adding this line exceeds max_size and we already have content, finalize chunk
        if chunk_bytes + line_bytes > max_size as usize && !chunk_lines.is_empty() {
            let end_line = chunk_start_line + chunk_lines.len() as u32 - 1;
            chunks.push(ChunkResult {
                path: path.to_string(),
                content: chunk_lines.join("\n"),
                start_line: chunk_start_line,
                end_line,
            });
            chunk_lines.clear();
            chunk_bytes = 0;
            chunk_start_line = (i + 1) as u32; // 1-based
        }

        chunk_lines.push(line);
        chunk_bytes += line_bytes;
    }

    // Finalize the last chunk
    if !chunk_lines.is_empty() {
        let end_line = chunk_start_line + chunk_lines.len() as u32 - 1;
        let last_chunk = ChunkResult {
            path: path.to_string(),
            content: chunk_lines.join("\n"),
            start_line: chunk_start_line,
            end_line,
        };

        // If the last chunk is smaller than min_size and there's a previous chunk, merge them
        if chunk_bytes < min_size as usize && !chunks.is_empty() {
            let prev = chunks.last_mut().unwrap();
            prev.content.push('\n');
            prev.content.push_str(&last_chunk.content);
            prev.end_line = last_chunk.end_line;
        } else {
            chunks.push(last_chunk);
        }
    }

    chunks
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_empty_file_produces_no_chunks() {
        let actual = chunk_file("test.rs", "", 100, 1500);
        assert!(actual.is_empty());
    }

    #[test]
    fn test_small_file_produces_one_chunk() {
        let content = "fn main() {\n    println!(\"hello\");\n}";
        let actual = chunk_file("main.rs", content, 100, 1500);

        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].path, "main.rs");
        assert_eq!(actual[0].start_line, 1);
        assert_eq!(actual[0].end_line, 3);
        assert_eq!(actual[0].content, content);
    }

    #[test]
    fn test_large_file_splits_into_multiple_chunks() {
        // Create a file with 10 lines, each 20 bytes, max_size = 50
        let lines: Vec<String> = (1..=10).map(|i| format!("line_{i:015}xxxxx")).collect();
        let content = lines.join("\n");

        let actual = chunk_file("big.rs", &content, 10, 50);

        assert!(actual.len() > 1, "Expected multiple chunks, got {}", actual.len());

        // Verify line continuity: last chunk's end_line == 10
        let last = actual.last().unwrap();
        assert_eq!(last.end_line, 10);

        // Verify first chunk starts at line 1
        assert_eq!(actual[0].start_line, 1);

        // Verify no gaps between chunks
        for window in actual.windows(2) {
            assert_eq!(window[0].end_line + 1, window[1].start_line);
        }
    }

    #[test]
    fn test_last_small_chunk_merged_with_previous() {
        // 3 lines: first two are big enough, third is tiny
        let line1 = "a".repeat(80);
        let line2 = "b".repeat(80);
        let line3 = "c"; // tiny last chunk
        let content = format!("{line1}\n{line2}\n{line3}");

        let actual = chunk_file("merge.rs", &content, 50, 100);

        // The tiny last chunk should be merged into the previous one
        let last = actual.last().unwrap();
        assert_eq!(last.end_line, 3);
        assert!(last.content.contains(line3));
    }
}
