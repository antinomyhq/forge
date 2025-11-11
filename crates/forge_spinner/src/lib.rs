use std::io::{self, Write};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use rand::seq::IndexedRandom;

/// Render the spinner line consistently with styling and flush.
fn render_spinner_line(frame: &str, status: &str, seconds: u64) {
    // Clear current line, then render spinner + message + timer + hint
    print!("\r\x1b[2K");
    print!(
        "\r\x1b[32m{}\x1b[0m  \x1b[1;32m{}\x1b[0m {}s · \x1b[2;37mCtrl+C to interrupt\x1b[0m",
        frame, status, seconds
    );
    let _ = io::stdout().flush();
}

/// Commands for the spinner background thread
enum Cmd {
    Write(String),
    Flush(mpsc::Sender<()>),
    Stop,
}

/// Manages spinner functionality for the UI
#[derive(Default)]
pub struct SpinnerManager {
    tx: Option<mpsc::Sender<Cmd>>,  // channel to spinner thread
    handle: Option<JoinHandle<()>>, // spinner thread handle
    message: Option<String>,        // current status text
    running: bool,
}

impl SpinnerManager {
    pub fn new() -> Self {
        Self::default()
    }
    /// Start the spinner with a message
    pub fn start(&mut self, message: Option<&str>) -> Result<()> {
        self.stop(None)?;

        // Enter raw mode
        enable_raw_mode()?;

        let words = [
            "Thinking",
            "Processing",
            "Analyzing",
            "Forging",
            "Researching",
            "Synthesizing",
            "Reasoning",
            "Contemplating",
        ];

        // Use a random word from the list
        let word = match message {
            None => words.choose(&mut rand::rng()).unwrap_or(&words[0]),
            Some(msg) => msg,
        };
        let status_text = word.to_string();
        self.message = Some(status_text.clone());

        let (tx, rx) = mpsc::channel::<Cmd>();

        let handle = thread::spawn(move || {
            // Old visual: frames and pace
            let spinner_frames: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let mut idx: usize = 0;
            let tick = Duration::from_millis(60);
            let mut last = std::time::Instant::now();
            let start_time = std::time::Instant::now();

            // Hide cursor and draw initial spinner line
            print!("\x1b[?25l");
            let seconds = 0u64;
            render_spinner_line(spinner_frames[idx], &status_text, seconds);

            let mut keep_running = true;
            let mut ctrl_c = false;
            while keep_running {
                // Handle incoming commands quickly
                match rx.recv_timeout(Duration::from_millis(5)) {
                    Ok(Cmd::Write(s)) => {
                        // Print above spinner then redraw spinner line
                        print!("\r\x1b[2K");
                        if !s.ends_with('\n') {
                            print!("{}\n", s);
                        } else {
                            print!("{}", s);
                        }
                        // Redraw spinner line with current visuals
                        let elapsed = start_time.elapsed().as_secs();
                        render_spinner_line(spinner_frames[idx], &status_text, elapsed);
                    }
                    Ok(Cmd::Flush(ack)) => {
                        let _ = ack.send(());
                    }
                    Ok(Cmd::Stop) => {
                        keep_running = false;
                        continue;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        keep_running = false;
                        continue;
                    }
                }

                // Drain input; capture Ctrl-C and request shutdown
                while event::poll(Duration::from_millis(0)).unwrap_or(false) {
                    match event::read() {
                        Ok(Event::Key(key)) => {
                            if key.modifiers.contains(KeyModifiers::CONTROL) {
                                if let KeyCode::Char('c') | KeyCode::Char('C') = key.code {
                                    ctrl_c = true;
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(_) => break,
                    }
                }

                if keep_running && last.elapsed() >= tick {
                    idx = (idx + 1) % spinner_frames.len();
                    let elapsed = start_time.elapsed().as_secs();
                    // Redraw the full spinner line to avoid artifacts
                    render_spinner_line(spinner_frames[idx], &status_text, elapsed);
                    last = std::time::Instant::now();
                }

                if ctrl_c {
                    keep_running = false;
                }
            }

            // Cleanup: clear line and show cursor
            print!("\r\x1b[2K");
            print!("\x1b[?25h");
            let _ = io::stdout().flush();

            if ctrl_c {
                // Cleanup: clear line and show cursor and disable raw mode
                print!("\r\x1b[2K");
                print!("\x1b[?25h");
                let _ = disable_raw_mode();
                // Exit with 130 to emulate SIGINT after cleanup
                std::process::exit(130);
            }
        });

        self.tx = Some(tx);
        self.handle = Some(handle);
        self.running = true;
        Ok(())
    }

    /// Stop the active spinner if any
    pub fn stop(&mut self, message: Option<String>) -> Result<()> {
        // Signal spinner thread to stop and join it
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(Cmd::Stop);
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }

        // Restore terminal mode
        let _ = disable_raw_mode();

        // Print trailing message if provided
        if let Some(msg) = message {
            println!("{}", msg);
        }

        self.running = false;
        self.message = None;
        Ok(())
    }

    pub fn write_ln(&mut self, message: impl ToString) -> Result<()> {
        let s = message.to_string();
        if let Some(tx) = &self.tx {
            // Write above spinner while it continues running
            let _ = tx.send(Cmd::Write(s));
        } else {
            println!("{}", s);
        }
        Ok(())
    }
}
