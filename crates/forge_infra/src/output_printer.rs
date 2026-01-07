//! Shared output printer for synchronized writes to stdout/stderr.
//!
//! Prevents interleaving when multiple threads write to terminal output.

use std::io::{self, Stderr, Stdout, Write};
use std::sync::{Arc, Mutex};

use forge_app::OutputPrinterInfra;

/// Thread-safe output printer that synchronizes writes to stdout/stderr.
///
/// Wraps writers in mutexes to prevent output interleaving when multiple
/// threads (e.g., streaming markdown and shell commands) write concurrently.
///
/// Generic over writer types `O` (stdout) and `E` (stderr) to support testing
/// with mock writers.
#[derive(Debug)]
pub struct OutputPrinter<O = Stdout, E = Stderr> {
    stdout: Arc<Mutex<O>>,
    stderr: Arc<Mutex<E>>,
}

impl<O, E> Clone for OutputPrinter<O, E> {
    fn clone(&self) -> Self {
        Self { stdout: self.stdout.clone(), stderr: self.stderr.clone() }
    }
}

impl Default for OutputPrinter<Stdout, Stderr> {
    fn default() -> Self {
        Self {
            stdout: Arc::new(Mutex::new(io::stdout())),
            stderr: Arc::new(Mutex::new(io::stderr())),
        }
    }
}

impl<O, E> OutputPrinter<O, E> {
    /// Creates a new OutputPrinter with custom writers.
    pub fn with_writers(stdout: O, stderr: E) -> Self {
        Self {
            stdout: Arc::new(Mutex::new(stdout)),
            stderr: Arc::new(Mutex::new(stderr)),
        }
    }
}

impl<O: Write + Send, E: Write + Send> OutputPrinterInfra for OutputPrinter<O, E> {
    fn write_stdout(&self, buf: &[u8]) -> io::Result<usize> {
        let mut guard = self
            .stdout
            .lock()
            .map_err(|_| io::Error::other("mutex poisoned"))?;
        guard.write(buf)
    }

    fn write_stderr(&self, buf: &[u8]) -> io::Result<usize> {
        let mut guard = self
            .stderr
            .lock()
            .map_err(|_| io::Error::other("mutex poisoned"))?;
        guard.write(buf)
    }

    fn flush_stdout(&self) -> io::Result<()> {
        let mut guard = self
            .stdout
            .lock()
            .map_err(|_| io::Error::other("mutex poisoned"))?;
        guard.flush()
    }

    fn flush_stderr(&self) -> io::Result<()> {
        let mut guard = self
            .stderr
            .lock()
            .map_err(|_| io::Error::other("mutex poisoned"))?;
        guard.flush()
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::thread;

    use super::*;

    #[test]
    fn test_concurrent_writes_dont_interleave() {
        let stdout = Cursor::new(Vec::new());
        let stderr = Cursor::new(Vec::new());
        let printer = OutputPrinter::with_writers(stdout, stderr);
        let p1 = printer.clone();
        let p2 = printer.clone();

        let h1 = thread::spawn(move || {
            p1.write_stdout(b"AAAA").unwrap();
            p1.write_stdout(b"BBBB").unwrap();
            p1.flush_stdout().unwrap();
        });

        let h2 = thread::spawn(move || {
            p2.write_stdout(b"XXXX").unwrap();
            p2.write_stdout(b"ZZZZ").unwrap();
            p2.flush_stdout().unwrap();
        });

        h1.join().unwrap();
        h2.join().unwrap();

        // Verify output is one of the valid non-interleaved orderings
        let actual = printer.stdout.lock().unwrap().get_ref().clone();
        let valid_orderings = [b"AAAABBBBXXXXZZZZ".to_vec(), b"XXXXZZZZAAAABBBB".to_vec()];
        assert!(
            valid_orderings.contains(&actual),
            "Output was interleaved: {:?}",
            String::from_utf8_lossy(&actual)
        );
    }

    #[test]
    fn test_with_mock_writer() {
        let stdout = Cursor::new(Vec::new());
        let stderr = Cursor::new(Vec::new());
        let printer = OutputPrinter::with_writers(stdout, stderr);

        printer.write_stdout(b"hello").unwrap();
        printer.write_stderr(b"error").unwrap();

        let stdout_content = printer.stdout.lock().unwrap().get_ref().clone();
        let stderr_content = printer.stderr.lock().unwrap().get_ref().clone();

        assert_eq!(stdout_content, b"hello");
        assert_eq!(stderr_content, b"error");
    }
}
