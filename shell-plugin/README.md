# Forge ZSH Plugin

A powerful ZSH plugin that provides intelligent command transformation, file tagging, conversation management, and customizable prompt integration for the Forge AI assistant.

## Plugin Files

- **`forge.plugin.zsh`** - Main plugin with command handling and completions
- **`forge-prompt.zsh`** - Optional: Prompt customization functions and integrations
- **`README.md`** - This file
- **`PROMPT_CUSTOMIZATION.md`** - Detailed guide for custom prompt themes

**Note:** The main plugin works standalone. Source `forge-prompt.zsh` separately if you want prompt integration.

## Features

- **Smart Command Transformation**: Convert `:command` syntax into forge executions
- **Agent Selection**: Tab completion for available agents using `:agent_name`
- **File Tagging**: Interactive file selection with `@[filename]` syntax
- **Syntax Highlighting**: Visual feedback for commands and tagged files
- **Conversation Continuity**: Automatic session management across commands
- **Interactive Completion**: Fuzzy finding for files and agents
- **Prompt Integration**: Show agent and model information in your prompt (Powerlevel10k, plain ZSH, Starship, Oh My Posh)
- **Customizable Prompts**: Public API for integrating Forge info into custom themes

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

### .forge Directory

The plugin creates a `.forge` directory in your current working directory (similar to `.git`) for temporary files:

- `FORGE_EDITMSG`: Temporary file used when opening an external editor with `:edit`

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

## Prompt Customization

The plugin provides optional prompt helper functions through `forge-prompt.zsh`.

### Loading the Functions

Add to your `~/.zshrc` **after** loading the main plugin:

```zsh
# Load main plugin
source /path/to/forge.plugin.zsh

# Optional: Load prompt helper functions
source /path/to/forge-prompt.zsh
```

This provides four helper functions: `forge_prompt_left()`, `forge_prompt_right()`, `forge_prompt_left_unstyled()`, and `forge_prompt_right_unstyled()`, plus Powerlevel9k/10k integration helpers (`prompt_forge_agent()` and `prompt_forge_model()`).

### Manual Integration

**Using Styled Functions (Recommended for ZSH):**
```zsh
PROMPT='$(forge_prompt_left)%F{blue}%~%f %# '
RPROMPT='$(forge_prompt_right)'
```

**Using Unstyled Functions (Custom Styling):**
```zsh
# Apply your own colors and formatting
PROMPT='%F{yellow}$(forge_prompt_left_unstyled)%f%F{blue}%~%f %# '
RPROMPT='%F{magenta}$(forge_prompt_right_unstyled)%f'
```

**Powerlevel10k/9k** (easiest integration):
```zsh
# Just add to your prompt elements - functions are already defined!
POWERLEVEL9K_LEFT_PROMPT_ELEMENTS=(... forge_agent dir vcs)
POWERLEVEL9K_RIGHT_PROMPT_ELEMENTS=(status forge_model time)
```

The plugin provides `prompt_forge_agent()` and `prompt_forge_model()` functions that work automatically with P10k's state system (idle/active).

**Powerlevel10k** (manual, if you want custom styling):
```zsh
function prompt_forge_agent() {
  local content=$(forge_prompt_left_unstyled)
  [[ -n "$content" ]] && p10k segment -t "$content"
}
# Then add 'forge_agent' to POWERLEVEL9K_LEFT_PROMPT_ELEMENTS
```

**Starship** (add to `~/.config/starship.toml`):
```toml
[custom.forge_agent]
command = "echo -n $_FORGE_ACTIVE_AGENT | tr '[:lower:]' '[:upper:]'"
when = '[ -n "$_FORGE_ACTIVE_AGENT" ]'
format = "[$output]($style) "
style = "bold white"
```

### Using Environment Variables Directly

You can skip `forge-prompt.zsh` entirely and use environment variables:

```zsh
# Just load the main plugin (no forge-prompt.zsh needed)
source /path/to/forge.plugin.zsh

# Access variables directly
PROMPT='%B${(U)_FORGE_ACTIVE_AGENT}%b %F{blue}%~%f %# '
```

Available variables:
- `$_FORGE_ACTIVE_AGENT` - Current agent (e.g., "forge", "sage")
- `$_FORGE_CONVERSATION_ID` - Active conversation UUID (empty if none)

