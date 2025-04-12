---
layout: default
title: Built-in Commands
parent: Features
nav_order: 4
---

# Built-in Commands

Forge offers several built-in commands to enhance your interaction:

- `/new` - Start a new task when you've completed your current one
- `/info` - View environment summary, logs folder location, and command history
- `/models` - List all available AI models with capabilities and context limits
- `/dump` - Save the current conversation in JSON format to a file for reference
- `/act` - Switch to ACT mode (default), allowing Forge to execute commands and implement changes
- `/plan` - Switch to PLAN mode, where Forge analyzes and plans but doesn't modify files
- `/compact` - Summarize the current conversation context to reduce token usage

## Usage

These commands can be entered directly in the Forge CLI by typing the command name preceded by a forward slash.

Example:
```
/info
```

This will display information about your environment including:
- Operating system details
- Current working directory
- Log file location
- Application version
- Available models