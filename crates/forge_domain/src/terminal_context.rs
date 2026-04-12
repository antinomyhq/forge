/// A single command entry captured by the shell plugin.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TerminalCommand {
    /// The command text as entered by the user.
    pub command: String,
    /// The exit code produced by the command.
    pub exit_code: i32,
    /// Unix timestamp (seconds since epoch) when the command was run.
    pub timestamp: u64,
}

/// Structured terminal context captured by the shell plugin.
///
/// Each field corresponds to one of the environment variables exported by the
/// zsh plugin before invoking forge:
/// - `FORGE_TERM_COMMANDS`   — colon-separated command strings
/// - `FORGE_TERM_EXIT_CODES` — colon-separated exit codes
/// - `FORGE_TERM_TIMESTAMPS` — colon-separated Unix timestamps
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
pub struct TerminalContext {
    /// Ordered list of recent commands, from oldest to newest.
    pub commands: Vec<TerminalCommand>,
}

impl TerminalContext {
    /// Creates a new `TerminalContext` from parallel vectors of command data.
    ///
    /// All three slices must have the same length; entries at the same index
    /// are combined into a single [`TerminalCommand`].  If the lengths differ,
    /// the shortest slice determines how many entries are produced.
    pub fn new(commands: Vec<String>, exit_codes: Vec<i32>, timestamps: Vec<u64>) -> Self {
        let entries = commands
            .into_iter()
            .zip(exit_codes)
            .zip(timestamps)
            .map(|((command, exit_code), timestamp)| TerminalCommand {
                command,
                exit_code,
                timestamp,
            })
            .collect();
        Self { commands: entries }
    }

    /// Returns `true` if there are no recorded commands.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Renders the terminal context as an XML element using [`forge_template::Element`].
    ///
    /// Each command is represented as an `<entry>` child with nested
    /// `<command>`, `<exit_code>`, and `<timestamp>` elements.
    pub fn render(&self) -> forge_template::Element {
        use forge_template::Element;
        Element::new("terminal_context").append(self.commands.iter().map(|cmd| {
            Element::new("entry")
                .append(Element::new("command").text(&cmd.command))
                .append(Element::new("exit_code").text(cmd.exit_code.to_string()))
                .append(Element::new("timestamp").text(cmd.timestamp.to_string()))
        }))
    }
}
