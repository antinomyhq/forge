use std::io;

/// Trait for synchronized output writing.
/// Provides two output channels (primary and error) with flush support.
/// Implementors must ensure thread-safe writes.
pub trait OutputPrinter: Send + Sync {
    /// Writes bytes to primary output.
    fn write(&self, buf: &[u8]) -> io::Result<usize>;
    /// Writes bytes to error output.
    fn write_err(&self, buf: &[u8]) -> io::Result<usize>;
    /// Flushes primary output.
    fn flush(&self) -> io::Result<()>;
    /// Flushes error output.
    fn flush_err(&self) -> io::Result<()>;
}
