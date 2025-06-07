/// Information about a file or file range read operation
#[derive(Debug, Clone, PartialEq)]
pub struct FileInfo {
    /// Starting line position of the read operation
    pub start_line: u64,

    /// Ending line position of the read operation
    pub end_line: u64,

    /// Total number of lines in the file
    pub total_lines: u64,
}

impl FileInfo {
    /// Creates a new FileInfo with the specified parameters
    pub fn new(start_line: u64, end_line: u64, total_lines: u64) -> Self {
        Self { start_line, end_line, total_lines }
    }

    /// Returns true if this represents a partial file read
    pub fn is_partial(&self) -> bool {
        self.start_line > 0 || self.end_line < self.total_lines
    }

    /// Returns the percentage of the file that was read (0.0 to 1.0)
    pub fn percent_read(&self) -> f64 {
        if self.total_lines == 0 {
            return 0.0;
        }

        let lines_read = self.end_line - self.start_line;
        lines_read as f64 / self.total_lines as f64
    }
}
