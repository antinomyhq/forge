use std::io::{self, Write};

/// A console writer that handles proper formatting by tracking cursor position
pub struct ConsoleWriter<W: Write> {
    writer: W,
    message: Option<String>,
}

impl<W: Write> ConsoleWriter<W> {
    pub fn new(writer: W) -> Self {
        Self { writer, message: None }
    }

    /// Writes text followed by a newline, ensuring proper formatting
    pub fn writeln(&mut self, message: impl AsRef<str>) -> io::Result<()> {
        if self.is_new_line_required() {
            writeln!(self.writer)?;
        }
        writeln!(self.writer, "{}", message.as_ref())?;
        self.writer.flush()?;
        self.message = Some(format!("{}\n", message.as_ref()));
        Ok(())
    }

    /// Writes text without a newline
    pub fn write(&mut self, message: impl AsRef<str>) -> io::Result<()> {
        write!(self.writer, "{}", message.as_ref())?;
        self.writer.flush()?;
        self.message = Some(message.as_ref().to_string());
        Ok(())
    }

    /// Checks if new line is required or not.
    fn is_new_line_required(&self) -> bool {
        self.message
            .as_ref()
            .is_some_and(|message| !message.ends_with('\n'))
    }
}

impl ConsoleWriter<io::Stdout> {
    pub fn stdout() -> Self {
        Self::new(io::stdout())
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    impl ConsoleWriter<Vec<u8>> {
        pub fn vec() -> ConsoleWriter<Vec<u8>> {
            Self::new(Vec::new())
        }
    }

    impl<W: Write> ConsoleWriter<W> {
        pub fn with_message<S: Into<String>>(mut self, message: S) -> Self {
            self.message = Some(message.into());
            self
        }
    }

    #[test]
    fn test_is_new_line_required_when_no_message() {
        let fixture = ConsoleWriter::vec();
        let actual = fixture.is_new_line_required();
        let expected = false;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_is_new_line_required_when_message_ends_with_newline() {
        let fixture = ConsoleWriter::vec().with_message("hello\n");
        let actual = fixture.is_new_line_required();
        let expected = false;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_is_new_line_required_when_message_does_not_end_with_newline() {
        let fixture = ConsoleWriter::vec().with_message("hello");
        let actual = fixture.is_new_line_required();
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_write_updates_message_state() {
        let mut fixture = ConsoleWriter::vec();
        fixture.write("test message").unwrap();
        let actual = fixture.message;
        let expected = Some("test message".to_string());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_write_outputs_correct_content() {
        let mut fixture = ConsoleWriter::vec();
        fixture.write("test message").unwrap();
        let actual = String::from_utf8(fixture.writer).unwrap();
        let expected = "test message".to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_writeln_updates_message_state_with_newline() {
        let mut fixture = ConsoleWriter::vec();
        fixture.writeln("test message").unwrap();
        let actual = fixture.message;
        let expected = Some("test message\n".to_string());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_writeln_outputs_correct_content() {
        let mut fixture = ConsoleWriter::vec();
        fixture.writeln("test message").unwrap();
        let actual = String::from_utf8(fixture.writer).unwrap();
        let expected = "test message\n".to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_writeln_adds_empty_line_when_needed() {
        let mut fixture = ConsoleWriter::vec().with_message("previous");
        fixture.writeln("new message").unwrap();
        let actual = String::from_utf8(fixture.writer).unwrap();
        let expected = "\nnew message\n".to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_writeln_does_not_add_empty_line_when_not_needed() {
        let mut fixture = ConsoleWriter::vec().with_message("previous\n");
        fixture.writeln("new message").unwrap();
        let actual = String::from_utf8(fixture.writer).unwrap();
        let expected = "new message\n".to_string();
        assert_eq!(actual, expected);
    }
}
