use std::io;

/// Custom writer trait for console operations
pub trait Writer {
    /// Write a string to the writer and flush
    fn write(&mut self, s: &str) -> io::Result<()>;
    /// Write a string with newline to the writer and flush
    fn writeln(&mut self, s: &str) -> io::Result<()>;
}

#[derive(Default)]
pub struct StdoutWriter;
impl Writer for StdoutWriter {
    fn write(&mut self, s: &str) -> io::Result<()> {
        use std::io::Write;
        write!(io::stdout(), "{s}")?;
        let _ = io::stdout().flush();
        Ok(())
    }

    fn writeln(&mut self, s: &str) -> io::Result<()> {
        use std::io::Write;
        writeln!(io::stdout(), "{s}")?;
        let _ = io::stdout().flush();
        Ok(())
    }
}

/// A console writer that handles proper formatting by tracking cursor position
pub struct WriterWrapper<W: Writer> {
    message: Option<String>,
    writer: W,
}

impl Default for WriterWrapper<StdoutWriter> {
    fn default() -> Self {
        Self { message: None, writer: StdoutWriter }
    }
}

impl<W: Writer> Writer for WriterWrapper<W> {
    fn write(&mut self, s: &str) -> io::Result<()> {
        self.writer.write(s)?;
        self.message = Some(s.into());
        Ok(())
    }

    fn writeln(&mut self, s: &str) -> io::Result<()> {
        if self.is_new_line_required() {
            self.writer.writeln("")?;
        }
        self.writer.writeln(s)?;
        self.message = Some(format!("{s}\n"));
        Ok(())
    }
}

impl<W: Writer> WriterWrapper<W> {
    pub fn new(writer: W) -> Self {
        Self { message: None, writer }
    }

    /// Checks if new line is required or not.
    fn is_new_line_required(&self) -> bool {
        self.message
            .as_ref()
            .is_some_and(|message| !message.ends_with('\n'))
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    impl<W: Writer> WriterWrapper<W> {
        pub fn message(&self) -> Option<&String> {
            self.message.as_ref()
        }
    }

    #[derive(Default)]
    struct MockWriter {
        content: String,
    }
    impl Writer for MockWriter {
        fn write(&mut self, s: &str) -> io::Result<()> {
            self.content.push_str(s);
            Ok(())
        }

        fn writeln(&mut self, s: &str) -> io::Result<()> {
            self.content.push_str(s);
            self.content.push_str("\n");
            Ok(())
        }
    }

    #[test]
    fn test_is_new_line_required_when_no_message() {
        let fixture = WriterWrapper::new(MockWriter::default());
        let actual = fixture.is_new_line_required();
        let expected = false;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_is_new_line_required_when_message_ends_with_newline() {
        let mut fixture = WriterWrapper::new(MockWriter::default());
        fixture.message = Some("hello\n".to_string());
        let actual = fixture.is_new_line_required();
        let expected = false;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_is_new_line_required_when_message_does_not_end_with_newline() {
        let mut fixture = WriterWrapper::new(MockWriter::default());
        fixture.message = Some("hello".to_string());
        let actual = fixture.is_new_line_required();
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_write_outputs_correct_content() {
        let mut fixture = WriterWrapper::new(MockWriter::default());
        fixture.write("test message").unwrap();
        let actual = fixture.message().unwrap();
        let expected = "test message".to_string();
        assert_eq!(*actual, expected);
    }

    #[test]
    fn test_writeln_outputs_correct_content() {
        let mut fixture = WriterWrapper::new(MockWriter::default());
        fixture.writeln("test message").unwrap();
        let actual = fixture.message().unwrap();
        let expected = "test message\n".to_string();
        assert_eq!(*actual, expected);
    }
}
