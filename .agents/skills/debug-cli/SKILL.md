---
name: debug-cli
description: Use when users need to debug, modify, or extend the code-forge application's CLI commands, argument parsing, or CLI behavior. This includes adding new commands, fixing CLI bugs, updating command options, or troubleshooting CLI-related issues.
---

# CLI Debug Skill

This skill provides a systematic workflow for debugging and verifying changes to the forge CLI application.

## Core Principles

1. **Always get latest docs first**: Run `--help` to see current commands and options
2. **Use `-p` for testing**: Test forge by giving it tasks with the `-p` flag
3. **Never commit**: This is for debugging only - don't commit changes
4. **Clone conversations**: When debugging conversation bugs, clone the source conversation before reproducing

## Workflow

### 1. Build the Application

Always build in debug mode after making changes:

```bash
cargo build
```

**Never** use `cargo build --release` for debugging - it's significantly slower and unnecessary for verification.

### 2. Get Latest Documentation

**Always** start by checking the latest help to understand current commands and options:

```bash
# Main help - do this first
./target/debug/forge --help

# Command-specific help
./target/debug/forge [COMMAND] --help

# Subcommand help
./target/debug/forge [COMMAND] [SUBCOMMAND] --help
```

### 3. Test with `-p` Flag

Use the `-p` flag to give forge a task to complete without interactive mode:

```bash
# Test with a simple prompt
./target/debug/forge -p "create a hello world rust program"

# Test with specific functionality
./target/debug/forge -p "read the README.md file and summarize it"

# Test with complex tasks
./target/debug/forge -p "analyze the code structure and suggest improvements"
```

### 4. Debug with Conversation Dumps

When debugging prompts or conversation issues, use `conversation dump` to export conversations. The command automatically creates a timestamped file:

```bash
# Dump conversation as JSON (creates: YYYY-MM-DD_HH-MM-SS-dump.json)
./target/debug/forge conversation dump <conversation-id>

# Export as HTML for human-readable format (creates: YYYY-MM-DD_HH-MM-SS-dump.html)
./target/debug/forge conversation dump --html <conversation-id>

# Use dumped JSON to reproduce issues
./target/debug/forge --conversation 2025-11-23_12-28-52-dump.json
```

### 5. Clone Before Reproducing Bugs

**Critical**: When a user provides a conversation with a bug, always clone it first:

```bash
# Clone the conversation
./target/debug/forge conversation clone <source-conversation-id>

# This creates a new conversation ID - use that for testing
./target/debug/forge --conversation-id <new-cloned-id>

# Keep cloning the source until the fix is verified
# Never modify the original conversation
```

**Why clone?**

- Preserves original bug evidence
- Allows multiple reproduction attempts
- Enables A/B testing of fixes
- Keeps source conversation clean

## Common Testing Patterns

### Test New Features

```bash
# Build and test new command
cargo build
./target/debug/forge --help  # Verify new command appears
./target/debug/forge new-command --help  # Check command docs
./target/debug/forge -p "test the new feature"
```

### Reproduce Reported Bugs

```bash
# 1. Dump the conversation (creates timestamped JSON file)
./target/debug/forge conversation dump <bug-conversation-id>

# 2. Clone it for testing (preserves original)
./target/debug/forge conversation clone <bug-conversation-id>

# 3. Reproduce with the cloned conversation
./target/debug/forge --conversation-id <cloned-id> -p "reproduce the issue"

# 4. After fix, verify with new clone
./target/debug/forge conversation clone <bug-conversation-id>
./target/debug/forge --conversation-id <new-clone-id> -p "verify fix"
```

### Test Edge Cases

```bash
# Test with missing arguments
./target/debug/forge command

# Test with invalid input
./target/debug/forge -p "invalid task with special chars: <>|&"

# Test with boundary values
./target/debug/forge -p "create a file with a very long name..."
```

### Debug Prompt Optimization

```bash
# 1. Dump conversation to analyze prompts (creates timestamped JSON)
./target/debug/forge conversation dump <id>

# 2. Review the conversation structure
cat 2025-11-23_12-28-52-dump.json | jq '.messages[] | {role, content}'

# 3. Export as HTML for easier reading
./target/debug/forge conversation dump --html <id>

# 4. Test modified prompts
./target/debug/forge -p "your optimized prompt here"
```

## Integration with Development Workflow

### After Code Changes

1. **Build**: `cargo build`
2. **Docs**: `./target/debug/forge --help` (verify documentation)
3. **Test**: `./target/debug/forge -p "relevant task"`
4. **Verify**: Check output matches expectations

### Debugging a Bug Report

1. **Clone**: `./target/debug/forge conversation clone <source-id>`
2. **Build**: `cargo build` (with potential fixes)
3. **Test**: `./target/debug/forge --conversation-id <cloned-id> -p "reproduce"`
4. **Iterate**: Repeat until verified
5. **Never commit** during debugging - only after full verification

## Quick Reference

```bash
# Standard debug workflow
cargo build
./target/debug/forge --help  # Always check docs first
./target/debug/forge -p "your test task"

# Dump conversation (creates timestamped file)
./target/debug/forge conversation dump <id>
# Output: 2025-11-23_12-28-52-dump.json

# Export as HTML for review
./target/debug/forge conversation dump --html <id>
# Output: 2025-11-23_12-28-52-dump.html

# Use dumped conversation
./target/debug/forge --conversation 2025-11-23_12-28-52-dump.json

# Clone and test bug
./target/debug/forge conversation clone <source-id>
./target/debug/forge --conversation-id <cloned-id> -p "reproduce bug"

# Debug prompts with jq (use actual filename)
cat 2025-11-23_12-28-52-dump.json | jq '.messages[] | {role, content}'

# Test with verbose output
./target/debug/forge --verbose -p "test task"
```

## Tips

- **Always `--help` first**: Get latest docs before testing
- **Use `-p` for testing**: Don't test interactively, use prompts
- **Clone conversations**: Never modify original bug conversations
- **Never commit**: This is for debugging only
- **Dump creates files**: `dump` automatically creates timestamped files (no `>` needed)
- **HTML exports**: Use `--html` flag for human-readable conversation views
- **Use relative paths**: Binary is at `./target/debug/forge` from project root
- **Check exit codes**: Use `echo $?` to verify exit codes
- **Watch for warnings**: Build warnings often indicate issues
