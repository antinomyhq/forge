# Forge ZSH Plugin

A powerful ZSH plugin that provides intelligent command transformation, file tagging, and conversation management for the Forge AI assistant.

## Features

- **Smart Command Transformation**: Convert `:command` syntax into forge executions
- **Agent Selection**: Tab completion for available agents using `:agent_name`
- **File Tagging**: Interactive file selection with `@[filename]` syntax
- **Syntax Highlighting**: Visual feedback for commands and tagged files
- **Conversation Continuity**: Automatic session management across commands
- **Interactive Completion**: Fuzzy finding for files and agents

## Prerequisites

Before using this plugin, ensure you have the following tools installed:

- **fzf** - Command-line fuzzy finder
- **fd** - Fast file finder (alternative to find)
- **forge** - The Forge CLI tool

### Installation of Prerequisites

```bash
# macOS (using Homebrew)
brew install fzf fd

# Ubuntu/Debian
sudo apt install fzf fd-find

# Arch Linux
sudo pacman -S fzf fd
```

## Usage

### Starting a Conversation

Begin any command with `:` followed by your prompt:

```bash
: Get the current time
```

This automatically starts a new conversation with the default Forge agent.

### Using Specific Agents

Specify an agent by name after the colon:

```bash
:sage How does caching work in this system?
:muse Create a deployment strategy for my app
```

**Tab Completion**: Type `:` followed by partial agent name and press `TAB` for interactive selection.

### File Tagging

Tag files in your commands using the `@[filename]` syntax:

```bash
: Review this code @[src/main.rs]
: Explain the configuration in @[config.yaml]
```

**Interactive Selection**: Type `@` and press `TAB` to search and select files interactively using fuzzy finder.

### Conversation Continuity

Commands within the same session maintain context:

```bash
# First command
: My project uses React and TypeScript

# Second command (remembers previous context)
: How can I optimize the build process?
```

The plugin automatically manages conversation IDs to maintain context across related commands.

### Session Management

#### Starting New Sessions

Clear the current conversation context and start fresh:

```bash
:new
# or use the alias
:n
```

This will:

- Clear the current conversation ID
- Show the banner with helpful information
- Reset the session state
- Display a confirmation message with timestamp

#### System Information

View system and project information:

```bash
:info
# or use the alias
:i
```

This displays:

- System information
- Project details
- Current configuration

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

Customize the plugin behavior by setting these variables before loading the plugin:

```bash
# Custom forge binary location
export FORGE_BIN="/path/to/custom/forge"
```

### Available Configuration Variables

- `FORGE_BIN`: Path to the forge executable (default: `forge`)
- Internal pattern matching for conversation syntax (`:`)
- New session command keyword: `:new` or `:n`

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

## Examples

### Basic Usage

```bash
: What's the weather like?
:sage Explain the MVC pattern
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
