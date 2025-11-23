use std::io::{self, Write};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use rand::seq::IndexedRandom;
use tokio::sync::broadcast;

/// Render the spinner line consistently with styling and flush.
fn render_spinner_line(frame: &str, status: &str, seconds: u64) {
    // Clear current line, then render spinner + message + timer + hint
    eprint!("\r\x1b[2K");
    eprint!(
        "\r\x1b[32m{}\x1b[0m  \x1b[1;32m{}\x1b[0m {}s · \x1b[2;37mCtrl+C to interrupt\x1b[0m",
        frame, status, seconds
    );
    let _ = io::stdout().flush();
}

/// Commands for the spinner background thread
enum Cmd {
    Start(String),
    Write(String),
    Hide,
    Show,
    Stop,
}

/// Manages spinner functionality for the UI
#[derive(Default)]
pub struct SpinnerManager {
    tx: Option<mpsc::Sender<Cmd>>,  // channel to spinner thread
    handle: Option<JoinHandle<()>>, // spinner thread handle
    message: Option<String>,        // current status text
    running: bool,
    hidden: bool,
}

impl SpinnerManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Initialize the spinner thread and return a receiver for Ctrl+C events
    pub fn init(&mut self) -> Result<broadcast::Receiver<()>> {
        let (tx, rx) = mpsc::channel::<Cmd>();
        let (ctrl_c_tx, ctrl_c_rx) = broadcast::channel(1);

        let handle = thread::spawn(move || {
            let spinner_frames: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let mut idx: usize = 0;
            let tick = Duration::from_millis(60);
            let mut last = std::time::Instant::now();
            let mut start_time = std::time::Instant::now();
            let mut status_text = String::new();
            let mut active = false;
            let mut hidden = false;

            loop {
                let cmd = if active && !hidden {
                    rx.recv_timeout(Duration::from_millis(5))
                } else {
                    rx.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected)
                };

                match cmd {
                    Ok(Cmd::Start(msg)) => {
                        status_text = msg;
                        active = true;
                        hidden = false;
                        start_time = std::time::Instant::now();
                        idx = 0;
                        last = std::time::Instant::now();

                        // Enter raw mode
                        let _ = enable_raw_mode();
                        // Hide cursor and draw initial spinner line
                        eprintln!("\x1b[?25l");
                        render_spinner_line(spinner_frames[idx], &status_text, 0);
                    }
                    Ok(Cmd::Write(s)) => {
                        if active {
                            eprint!("\r\x1b[2K");
                            println!("{}", s);
                            if !hidden {
                                let elapsed = start_time.elapsed().as_secs();
                                render_spinner_line(spinner_frames[idx], &status_text, elapsed);
                            } else {
                                let _ = io::stdout().flush();
                            }
                        } else {
                            println!("{}", s);
                        }
                    }
                    Ok(Cmd::Hide) => {
                        if active && !hidden {
                            eprint!("\r\x1b[2K");
                            eprint!("\x1b[?25h");
                            let _ = io::stdout().flush();
                            let _ = disable_raw_mode();
                            hidden = true;
                        }
                    }
                    Ok(Cmd::Show) => {
                        if active && hidden {
                            let _ = enable_raw_mode();
                            eprint!("\n\n\x1b[?25l");
                            let elapsed = start_time.elapsed().as_secs();
                            render_spinner_line(spinner_frames[idx], &status_text, elapsed);
                            hidden = false;
                        }
                    }
                    Ok(Cmd::Stop) => {
                        if active {
                            eprint!("\r\x1b[2K");
                            eprint!("\x1b[?25h");
                            let _ = io::stdout().flush();
                            let _ = disable_raw_mode();
                            active = false;
                            hidden = false;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }

                if active && !hidden {
                    // Poll for input (Ctrl+C)
                    while event::poll(Duration::from_millis(0)).unwrap_or(false) {
                        match event::read() {
                            Ok(Event::Key(key)) => {
                                if key.modifiers.contains(KeyModifiers::CONTROL)
                                    && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
                                {
                                    eprint!("\r\x1b[2K");
                                    eprint!("\x1b[?25h");
                                    // Notify UI
                                    let _ = ctrl_c_tx.send(());
                                }
                            }
                            Ok(_) => {}
                            Err(_) => {}
                        }
                    }

                    if last.elapsed() >= tick {
                        idx = (idx + 1) % spinner_frames.len();
                        let elapsed = start_time.elapsed().as_secs();
                        render_spinner_line(spinner_frames[idx], &status_text, elapsed);
                        last = std::time::Instant::now();
                    }
                }
            }
        });

        self.tx = Some(tx);
        self.handle = Some(handle);
        Ok(ctrl_c_rx)
    }

    /// Start the spinner with a message
    pub fn start(&mut self, message: Option<&str>) -> Result<()> {
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
        self.running = true;
        self.hidden = false;

        if let Some(tx) = &self.tx {
            let _ = tx.send(Cmd::Start(status_text));
        }

        Ok(())
    }

    /// Stop the active spinner if any
    pub fn stop(&mut self, message: Option<String>) -> Result<()> {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Cmd::Stop);
        }

        // Print trailing message if provided
        if let Some(msg) = message {
            println!("{}", msg);
        }

        self.running = false;
        self.message = None;
        self.hidden = false;
        Ok(())
    }

    pub fn write_ln(&mut self, message: impl ToString) -> Result<()> {
        let s = message.to_string();
        let normalized = s.replace('\n', "\n\x1b[0G");

        if let Some(tx) = &self.tx {
            let _ = tx.send(Cmd::Write(normalized));
        } else {
            println!("{}", normalized);
        }
        Ok(())
    }

    pub fn ewrite_ln(&mut self, message: impl ToString) -> Result<()> {
        self.hide()?;
        eprintln!("{}", message.to_string());
        self.show()?;
        Ok(())
    }

    /// Hide the spinner without resetting the timer.
    pub fn hide(&mut self) -> Result<()> {
        if self.running && !self.hidden {
            if let Some(tx) = &self.tx {
                let _ = tx.send(Cmd::Hide);
            }
            self.hidden = true;
        }
        Ok(())
    }

    /// Show a previously hidden spinner, keeping the elapsed time.
    pub fn show(&mut self) -> Result<()> {
        if self.running && self.hidden {
            if let Some(tx) = &self.tx {
                let _ = tx.send(Cmd::Show);
            }
            self.hidden = false;
        }
        Ok(())
    }
}

impl Drop for SpinnerManager {
    fn drop(&mut self) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Cmd::Stop);
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
