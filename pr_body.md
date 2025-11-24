## Summary
Add support for `:conversation clone` command in shell plugin to provide consistent cloning functionality between CLI and interactive modes.

## Changes Made
- **Clone Current Conversation**: Support for `:conversation clone` to clone the active conversation
- **Clone Specific Conversation**: Support for `:conversation clone <id>` to clone a specific conversation  
- **Error Handling**: Proper error messages when no active conversation exists
- **Visual Feedback**: Progress indicator and success/error messages with appropriate colors
- **Automatic Switching**: Automatically switches to the cloned conversation
- **Backward Compatibility**: Existing `:conversation` and `:conversation <id>` behavior preserved

## Key Features

### ✅ **Dual Mode Support**
- **`:conversation clone`** - Clone current conversation (requires active conversation)
- **`:conversation clone <id>`** - Clone specific conversation by ID

### ✅ **Error Handling**
- **No Active Conversation**: "No active conversation to clone. Start a conversation first or use :conversation to select one"
- **Clone Failure**: Shows forge command output for debugging
- **ID Extraction Error**: Handles cases where new conversation ID cannot be extracted

### ✅ **User Experience**
- **Progress Indicator**: Shows "⏳ Cloning conversation..." during operation
- **Success Message**: "✓ Conversation cloned and switched to [new-id]"
- **Source Info**: "└─ From: [source-id]" for clarity
- **Conversation Info**: Displays conversation details after successful clone

### ✅ **Integration with Forge CLI**
- **Command Execution**: Uses `forge conversation clone <id>` for actual cloning
- **ID Extraction**: Parses new conversation ID from forge output using regex
- **State Management**: Updates `_FORGE_CONVERSATION_ID` to new conversation

## Usage Examples

**Clone Current Conversation:**
```
:conversation clone
⏳ Cloning conversation abc123...
✓ Conversation cloned and switched to def456
└─ From: abc123
```

**Clone Specific Conversation:**
```
:conversation clone xyz789
⏳ Cloning conversation xyz789...
✓ Conversation cloned and switched to def456
└─ From: xyz789
```

**Error Case:**
```
:conversation clone
✗ No active conversation to clone. Start a conversation first or use :conversation to select one
```

## Testing
- ✅ Syntax validation: Plugin loads without errors
- ✅ Command parsing: Correctly detects `clone` subcommand
- ✅ Error scenarios: Proper handling of missing conversation
- ✅ Integration: Works with forge CLI backend
- ✅ Backward compatibility: Existing functionality preserved

This implementation provides a consistent cloning experience across both CLI (`forge conversation clone <id>`) and interactive (`:conversation clone`) interfaces.

Co-Authored-By: ForgeCode <noreply@forgecode.dev>