/// Information about a file or file range read operation
#[derive(Debug, Clone, PartialEq)]
pub struct FileInfo {
    /// Starting character position of the read operation
    pub start_char: u64,

    /// Ending character position of the read operation
    pub end_char: u64,

    /// Total number of characters in the file
    pub total_chars: u64,
}

impl FileInfo {
    /// Creates a new FileInfo with the specified parameters
    pub fn new(start_char: u64, end_char: u64, total_chars: u64) -> Self {
        Self { start_char, end_char, total_chars }
    }

    /// Returns true if this represents a partial file read
    pub fn is_partial(&self) -> bool {
        self.start_char > 0 || self.end_char < self.total_chars
    }

    /// Returns the percentage of the file that was read (0.0 to 1.0)
    pub fn percent_read(&self) -> f64 {
        if self.total_chars == 0 {
            return 0.0;
        }

        let chars_read = self.end_char - self.start_char;
        chars_read as f64 / self.total_chars as f64
    }
}
