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
- `/model` - Select and set a specific model in your forge.yaml configuration
- `/dump` - Save the current conversation in JSON format to a file for reference
- `/act` - Switch to ACT mode (default), allowing Forge to execute commands and implement changes
- `/plan` - Switch to PLAN mode, where Forge analyzes and plans but doesn't modify files

## Native Shell Commands

Forge allows you to execute native shell commands directly from the CLI by prefixing them with `!`:

```
!ls -la
!git status
!npm install
```

These commands will be executed in your current working directory, and their output will be displayed in the console.

## Usage

These commands can be entered directly in the Forge CLI by typing the command name preceded by a forward slash (for built-in commands) or an exclamation mark (for native shell commands).

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

## Model Selection

The `/model` command allows you to interactively select from available AI models and set your preferred model in the project's forge.yaml configuration file:

```
/model
```

This will:
1. Display an interactive selection menu with all available models
2. Update the standard_model anchor in your forge.yaml file with your selection
3. Confirm the change with a success message

The model choice will persist between sessions as it's stored in your configuration file.

