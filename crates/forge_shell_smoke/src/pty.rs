//! Portable PTY test harness.
//!
//! Provides [`PtySession`] which spawns an arbitrary process inside a real
//! pseudo-terminal and exposes helpers for writing to its stdin and reading
//! from its combined stdout/stderr output.  Because the child process sees a
//! genuine TTY on both sides, readline libraries (rustyline, reedline, …) and
//! TTY-detection checks behave exactly as they would in an interactive terminal
//! session.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, NativePtySystem, PtyPair, PtySize, PtySystem as _};

/// A live pseudo-terminal session wrapping a spawned child process.
///
/// The process sees a real TTY on stdin, stdout, and stderr so that
/// TTY-detection and readline libraries work correctly.
pub struct PtySession {
    /// Write half of the PTY (connected to the child's stdin).
    writer: Box<dyn Write + Send>,
    /// Accumulated output captured from the PTY master read side.
    output: Arc<Mutex<Vec<u8>>>,
    /// Set to `true` once the reader thread has drained all output (child exited).
    eof: Arc<AtomicBool>,
    /// The PTY pair kept alive for the lifetime of the session.
    _pair: PtyPair,
}

impl PtySession {
    /// Spawns `program` with `args` inside a new 80×24 PTY.
    ///
    /// # Errors
    /// Returns an error if the PTY cannot be created or the child process
    /// cannot be spawned.
    pub fn spawn(program: impl Into<PathBuf>, args: &[&str]) -> anyhow::Result<Self> {
        Self::spawn_with_env(program, args, &[])
    }

    /// Spawns `program` with `args` and additional environment variables inside
    /// a new 80×24 PTY.
    ///
    /// `extra_env` is a slice of `(key, value)` pairs that are merged into the
    /// child's environment on top of the current process environment.
    ///
    /// # Errors
    /// Returns an error if the PTY cannot be created or the child process
    /// cannot be spawned.
    pub fn spawn_with_env(
        program: impl Into<PathBuf>,
        args: &[&str],
        extra_env: &[(&str, &str)],
    ) -> anyhow::Result<Self> {
        let pty_system = NativePtySystem::default();
        let pair = pty_system.openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut cmd = CommandBuilder::new(program.into());
        for arg in args {
            cmd.arg(arg);
        }
        for (key, val) in extra_env {
            cmd.env(key, val);
        }

        // Spawn the child attached to the slave side of the PTY.
        let _child = pair.slave.spawn_command(cmd)?;

        // Obtain a writer to the master (drives the child's stdin).
        let writer = pair.master.take_writer()?;

        // Obtain a reader from the master (receives the child's stdout + stderr).
        let mut reader = pair.master.try_clone_reader()?;

        // Background thread: continuously drain PTY output into a shared buffer
        // and set the `eof` flag when the child closes the PTY master.
        let output = Arc::new(Mutex::new(Vec::<u8>::new()));
        let eof = Arc::new(AtomicBool::new(false));

        let output_clone = Arc::clone(&output);
        let eof_clone = Arc::clone(&eof);
        std::thread::spawn(move || {
            let mut buf = [0u8; 256];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let mut guard = output_clone.lock().unwrap();
                        guard.extend_from_slice(&buf[..n]);
                    }
                }
            }
            // Signal that no more output will arrive.
            eof_clone.store(true, Ordering::Release);
        });

        Ok(Self { writer, output, eof, _pair: pair })
    }

    /// Writes `line` followed by `\n` to the child's stdin.
    ///
    /// # Errors
    /// Returns an error if the underlying PTY write fails.
    pub fn send_line(&mut self, line: &str) -> anyhow::Result<()> {
        write!(self.writer, "{}\n", line)?;
        self.writer.flush()?;
        Ok(())
    }

    /// Writes raw bytes to the child's stdin.
    ///
    /// Use this to send control characters such as Ctrl-D (`0x04`) or
    /// Ctrl-C (`0x03`).
    ///
    /// # Errors
    /// Returns an error if the underlying PTY write fails.
    pub fn send(&mut self, data: &[u8]) -> anyhow::Result<()> {
        self.writer.write_all(data)?;
        self.writer.flush()?;
        Ok(())
    }

    /// Returns a snapshot of all output collected so far as a UTF-8 `String`.
    ///
    /// Non-UTF-8 bytes are replaced with the Unicode replacement character.
    pub fn output(&self) -> String {
        let guard = self.output.lock().unwrap();
        String::from_utf8_lossy(&guard).into_owned()
    }

    /// Returns the number of bytes captured so far.
    ///
    /// Pair with [`output_since`] to isolate the output produced by a single
    /// command without carrying forward accumulated banner or TUI noise.
    ///
    /// # Pattern
    /// ```ignore
    /// let mark = session.output_len();
    /// session.send_line("echo hello")?;
    /// session.expect("hello", Duration::from_secs(5))?;
    /// let fresh = session.output_since(mark); // contains only the new bytes
    /// ```
    pub fn output_len(&self) -> usize {
        self.output.lock().unwrap().len()
    }

    /// Returns only the output captured *after* byte offset `since`.
    ///
    /// Pair with [`output_len`] — capture a mark before sending a command,
    /// then call this after [`expect`] returns to get a clean window of just
    /// that command's output.
    pub fn output_since(&self, since: usize) -> String {
        let guard = self.output.lock().unwrap();
        let slice = &guard[since.min(guard.len())..];
        String::from_utf8_lossy(slice).into_owned()
    }

    /// Returns `true` once the child process has exited and all of its output
    /// has been drained into the internal buffer.
    pub fn is_done(&self) -> bool {
        self.eof.load(Ordering::Acquire)
    }

    /// Blocks until `needle` appears somewhere in the accumulated output, then
    /// returns the full output seen so far.
    ///
    /// If the child exits before `needle` is found the function returns an
    /// error immediately (rather than spinning until the full timeout) so that
    /// tests targeting short-lived commands finish quickly.
    ///
    /// # Errors
    /// Returns an error if `timeout` elapses or the child exits before
    /// `needle` is found.
    pub fn expect(&self, needle: &str, timeout: Duration) -> anyhow::Result<String> {
        let start = Instant::now();
        loop {
            let current = self.output();
            if current.contains(needle) {
                return Ok(current);
            }

            // Child has exited — no more output is coming; fail fast.
            if self.is_done() {
                // One tiny extra window for any trailing bytes the background
                // thread may not have committed to the buffer yet.
                std::thread::sleep(Duration::from_millis(20));
                let final_output = self.output();
                if final_output.contains(needle) {
                    return Ok(final_output);
                }
                return Err(anyhow::anyhow!(
                    "Child exited without producing {:?}.\nFull output:\n{}",
                    needle,
                    final_output
                ));
            }

            if start.elapsed() >= timeout {
                return Err(anyhow::anyhow!(
                    "Timeout after {:?} waiting for {:?}.\nOutput so far:\n{}",
                    timeout,
                    needle,
                    current
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
