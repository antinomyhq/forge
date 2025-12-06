# Forge Shell Plugin: Rename Conversation

This plugin provides shell-based conversation renaming functionality for Forge.

## Installation

1. Copy this plugin directory to your Forge plugins folder
2. Source the plugin in your shell configuration

## Usage

### Shell Command
```bash
forge conversation rename <conversation-id> <new-title>
```

### Built-in Command (within Forge)
```
:rename <conversation-id> <new-title>
```

## Examples

```bash
# Rename conversation by ID
forge conversation rename 123e4567-e89b-12d3-a456-426614174000 "My New Title"

# Or use built-in command
:rename 123e4567-e89b-12d3-a456-426614174000 "My New Title"
```

## Implementation

The plugin consists of:
- `lib/actions/rename.sh` - Shell action handler
- `plugin.json` - Plugin configuration
- Integration with Forge's built-in command system

The shell handler parses UUID patterns and validates input before calling the Forge CLI.