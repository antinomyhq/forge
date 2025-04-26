---
layout: default
title: Custom Workflows
parent: Features
nav_order: 13
---

# Custom Workflows

For complex tasks, a single agent may not be sufficient. Forge allows you to create custom workflows with multiple specialized agents working together to accomplish sophisticated tasks.

## Creating Custom Workflows

You can configure your own workflows by creating a YAML file and pointing to it with the `-w` flag:

```bash
forge -w /path/to/your/workflow.yaml
```

## Configuration Loading and Precedence

Forge loads workflow configurations using the following precedence rules:

1. **Explicit Path**: When a path is provided with the `-w` flag, Forge loads that configuration directly without any merging
2. **Project Configuration**: If no explicit path is provided, Forge looks for `forge.yaml` in the current directory
3. **Default Configuration**: An embedded default configuration is always available as a fallback

When a project configuration exists in the current directory, Forge creates a merged configuration where:

- Project settings in `forge.yaml` take precedence over default settings
- Any settings not specified in the project configuration inherit from defaults

This approach allows you to customize only the parts of the configuration you need while inheriting sensible defaults for everything else.

## Workflow Configuration

A workflow consists of agents connected via events. Each agent has specific capabilities and can perform designated tasks.

### Event System

Agents communicate through events which they can publish and subscribe to:

**Built-in Events**

- `user_task_init` - Published when a new task is initiated
- `user_task_update` - Published when follow-up instructions are provided by the user

### Agent Tools

Each agent needs tools to perform tasks, configured in the `tools` field:

**Built-in Tools**

- `forge_tool_fs_read` - Read from the filesystem
- `forge_tool_fs_create` - Create or overwrite files
- `forge_tool_fs_remove` - Remove files
- `forge_tool_fs_search` - Search for patterns in files
- `forge_tool_fs_list` - List files in a directory
- `forge_tool_fs_info` - Get file metadata
- `forge_tool_process_shell` - Execute shell commands
- `forge_tool_process_think` - Perform internal reasoning
- `forge_tool_net_fetch` - Fetch data from the internet
- `forge_tool_event_dispatch` - Dispatch events to other agents
- `forge_tool_fs_patch` - Patch existing files

### Custom Commands

Forge allows you to define custom commands in your workflow configuration. These commands can be executed within the Forge CLI using the `/command_name` syntax.

For more detailed information on custom workflows, agent configuration options, templates, and examples, please see the [Enhanced Workflows](../enhanced-workflow.html) documentation.