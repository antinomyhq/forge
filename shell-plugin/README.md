#### Starting New Sessions

Clear current conversation context and start fresh:

```bash
:new
```

This will:

- Clear current conversation ID
- Show banner with helpful information
- Reset session state
- Display a confirmation message with timestamp

#### System Information

View system and project information:

```bash
:info
```

This displays:

- System information
- Project details
- Current configuration

#### Renaming Conversations

Rename an existing conversation with interactive selection:

```bash
:rename
# or use alias
:rn
```

This will:

- Display an interactive list of all conversations with preview
- Allow you to select a conversation to rename
- Prompt for a new title interactively
- Update conversation title
- Show confirmation message with new title

You can also rename a specific conversation by providing its ID:

```bash
:rename <conversation_id>
```

This is useful when you want to:

- Give conversations more descriptive names for easier identification
- Organize conversations by project or topic
- Update titles after conversation scope changes
- Create better context for conversation searching

#### Cloning Conversations

Create a copy of an existing conversation with interactive selection:

```bash
:clone
```

This will:

- Display an interactive list of all conversations with preview
- Allow you to select a conversation to clone
- Create a new conversation with the same content
- Automatically switch to the cloned conversation
- Show the cloned conversation content and details

You can also clone a specific conversation by providing its ID:

```bash
:clone <conversation_id>
```

This is useful when you want to:

- Create a backup before making significant changes
- Start a new conversation branch from an existing context
- Experiment with different approaches while preserving the original

#### Session Status

The plugin automatically displays session information including:

- Conversation ID when starting new sessions
- Active agent information
- New session confirmations with timestamps

## Syntax Highlighting

The plugin provides visual feedback through syntax highlighting:

- **Tagged Files** (`@[filename]`): Displayed in **green bold**
- **Agent Commands** (`:agent`): Agent names in **yellow bold**
- **Command Text**: Remaining text in **white bold**

## Configuration

Customize plugin behavior by setting these variables before loading the plugin:

```bash
# Custom forge binary location
export FORGE_BIN="/path/to/custom/forge"
```

### Available Configuration Variables

- `FORGE_BIN`: Path to forge executable (default: `forge`)
- Internal pattern matching for conversation syntax (`:`)
- New session command keyword: `:new` or `:n`

### Codebase Indexing

Sync your codebase for semantic search:

```bash
:sync
```

This will index the current directory for semantic code search.

### .forge Directory

The plugin creates a `.forge` directory in your current working directory (similar to `.git`) for temporary files:

- `FORGE_EDITMSG.md`: Temporary file used when opening an external editor with `:edit`

## Advanced Features

### Command History

All transformed commands are properly saved to ZSH history, allowing you to:

- Navigate command history with arrow keys
- Search previous forge commands with `Ctrl+R`
- Reuse complex commands with file tags

### Keyboard Shortcuts

- **Tab**: Interactive completion for files (`@`) and agents (`:`)
- **Enter**: Transform and execute `:commands`
- **Ctrl+C**: Interrupt running forge commands

#### Command Aliases

The plugin provides convenient aliases for commonly used commands:

- `:n` - Alias for `:new` (start new session)
- `:i` - Alias for `:info` (show system information)  
- `:rn` - Alias for `:rename` (rename conversation)

## Examples

### Basic Usage

```bash
: What's the weather like?
:sage Explain MVC pattern
:planner Help me structure this project
```

### With File Tagging

```bash
: Review this implementation @[src/auth.rs]
: Debug the issue in @[logs/error.log] @[config/app.yml]
```

### Session Flow

```bash
: I'm working on a Rust web API
: What are the best practices for error handling?
: Show me an example with @[src/errors.rs]
:info
:new
: New conversation starts here
```

### Codebase Indexing

```bash
# Sync current directory for semantic search
:sync
```