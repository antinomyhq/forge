mod console_writer;
mod spinner;

use anyhow::Result;
pub use console_writer::*;
pub use spinner::*;

/// Manages spinner functionality for the UI
pub struct SpinnerManager<W: Writer, S: Spinner> {
    spinner: S,
    writer: WriterWrapper<W>,
    message: Option<String>,
}

impl<W: Writer, S: Spinner> SpinnerManager<W, S> {
    pub fn new(writer: W, spinner: S) -> Self {
        Self { spinner, writer: WriterWrapper::new(writer), message: None }
    }

    // Starts the spinner
    pub fn start(&mut self, message: Option<&str>) -> Result<()> {
        let _ = self.spinner.start(message)?;
        self.message = message.map(|m| m.to_string());
        Ok(())
    }

    /// Stop the active spinner if any
    pub fn stop(&mut self, message: Option<String>) -> Result<()> {
        let _ = self.stop_internal(message, true)?;
        Ok(())
    }

    /// Stop the active spinner if any and prints the provided content.
    fn stop_internal(&mut self, message: Option<String>, new_line: bool) -> Result<()> {
        self.spinner.stop()?;
        // Then print the message if provided
        if let Some(msg) = message {
            if new_line {
                self.writer.writeln(&msg)?;
            } else {
                self.writer.write(&msg)?;
            }
        }
        self.message = None;
        Ok(())
    }

    // Writes the console with new line.
    pub fn write_ln(&mut self, message: impl ToString) -> Result<()> {
        let is_running = self.spinner.is_running();
        let prev_message = self.message.clone();
        self.stop(Some(message.to_string()))?;
        if is_running {
            self.start(prev_message.as_deref())?;
        }

        Ok(())
    }

    // Writes the console without new line.
    pub fn write(&mut self, message: impl ToString) -> Result<()> {
        let is_running = self.spinner.is_running();
        let prev_message = self.message.clone();
        self.stop_internal(Some(message.to_string()), false)?;
        if is_running {
            self.message = prev_message;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::io;

    impl Default for SpinnerManager<WriterWrapper<TestWriter>, TestSpinner> {
        fn default() -> Self {
            let fixture_writer = WriterWrapper::new(TestWriter::default());
            let fixture_spinner = TestSpinner::default();
            SpinnerManager::new(fixture_writer, fixture_spinner)
        }
    }

    // Test Writer implementation
    #[derive(Default)]
    struct TestWriter {
        content: String,
    }

    impl Writer for TestWriter {
        fn write(&mut self, s: &str) -> io::Result<()> {
            self.content.push_str(s);
            Ok(())
        }

        fn writeln(&mut self, s: &str) -> io::Result<()> {
            self.content.push_str(&format!("{}\n", s));
            Ok(())
        }
    }

    // Test Spinner implementation
    #[derive(Default)]
    struct TestSpinner {
        running: bool,
        started_message: Option<String>,
    }

    impl Spinner for TestSpinner {
        fn start(&mut self, message: Option<&str>) -> Result<()> {
            self.running = true;
            self.started_message = message.map(|m| m.to_string());
            Ok(())
        }

        fn stop(&mut self) -> Result<()> {
            self.running = false;
            self.started_message = None;
            Ok(())
        }

        fn is_running(&self) -> bool {
            self.running
        }
    }

    #[test]
    fn test_start_activates_spinner_with_message() {
        let mut fixture = SpinnerManager::default();
        let actual = fixture.start(Some("test message"));

        assert!(actual.is_ok());
        // assert on spinner.
        assert_eq!(fixture.spinner.is_running(), true);
        assert_eq!(fixture.message, Some("test message".to_string()));
        assert_eq!(
            fixture.spinner.started_message,
            Some("test message".to_string())
        );
        // writer shouldn't write anything.
        assert!(fixture.writer.message().is_none())
    }

    #[test]
    fn test_start_activates_spinner_without_message() {
        let mut fixture = SpinnerManager::default();
        let actual = fixture.start(None);

        assert!(actual.is_ok());
        // assert on spinner.
        assert_eq!(fixture.spinner.is_running(), true);
        assert_eq!(fixture.message, None);
        assert_eq!(fixture.spinner.started_message, None);
        // writer shouldn't write anything.
        assert!(fixture.writer.message().is_none())
    }

    #[test]
    fn test_stop_deactivates_spinner_with_message() {
        let mut fixture = SpinnerManager::default();
        fixture.start(Some("processing")).unwrap();

        let actual = fixture.stop(Some("completed".to_string()));

        // spinner shouldn't be running.
        assert!(actual.is_ok());
        assert_eq!(fixture.spinner.is_running(), false);
        assert_eq!(fixture.message, None);

        // writer should add new line.
        assert_eq!(fixture.writer.message(),Some(&"completed\n".to_string()))
    }

    #[test]
    fn test_stop_deactivates_spinner_without_message() {
        let mut fixture = SpinnerManager::default();
        fixture.start(Some("processing")).unwrap();

        let actual = fixture.stop(None);

        assert!(actual.is_ok());
        assert_eq!(fixture.spinner.is_running(), false);
        assert_eq!(fixture.message, None);

        // writer should add new line.
        assert_eq!(fixture.writer.message(), None);
    }

    #[test]
    fn test_write_ln_stops_spinner_writes_message_and_restarts() {
        let mut fixture = SpinnerManager::default();
        fixture.start(Some("processing")).unwrap();

        let actual = fixture.write_ln("output message");

        assert!(actual.is_ok());
        // when we write something, spinner should be stopped.
        assert_eq!(fixture.spinner.is_running(), true);
        assert_eq!(fixture.message, Some("processing".to_string()));
        assert_eq!(fixture.writer.message(), Some(&"output message\n".to_string()));
    }

    #[test]
    fn test_write_outputs_message_without_newline_and_stops_spinner() {
        let mut fixture = SpinnerManager::default();
        fixture.start(Some("processing")).unwrap();

        let actual = fixture.write("output message");

        assert!(actual.is_ok());
        assert_eq!(fixture.spinner.is_running(), false);
        assert_eq!(fixture.message, Some("processing".to_string()));
        assert_eq!(fixture.writer.message(), Some(&"output message".to_string()));
    }
}
