# Agent Configuration Formats

The Forge agent loader now supports multiple configuration formats for defining custom agents:

## Supported Formats

### 1. Markdown (.md) - Original Format
Agents defined with YAML frontmatter and markdown content:

```markdown
---
id: "my-agent"
title: "My Agent"
description: "Description of what the agent does"
model: "claude-3-5-sonnet-20241022"
temperature: 0.7
tools: ["fs_read", "fs_write"]
---

# My Agent System Prompt

This is the system prompt content that will be used by the agent.
It can include markdown formatting and multiple lines.
```

### 2. YAML (.yaml, .yml) - New Format
Pure YAML configuration files:

```yaml
id: "my-agent"
title: "My Agent"
description: "Description of what the agent does"
model: "claude-3-5-sonnet-20241022"
temperature: 0.7
tools: ["fs_read", "fs_write"]
system_prompt: |
  # My Agent System Prompt
  
  This is the system prompt content that will be used by the agent.
  It can include markdown formatting and multiple lines.
```

### 3. JSON (.json) - New Format
JSON configuration files:

```json
{
  "id": "my-agent",
  "title": "My Agent", 
  "description": "Description of what the agent does",
  "model": "claude-3-5-sonnet-20241022",
  "temperature": 0.7,
  "tools": ["fs_read", "fs_write"],
  "system_prompt": "# My Agent System Prompt\n\nThis is the system prompt content that will be used by the agent.\nIt can include markdown formatting and multiple lines."
}
```

## Key Differences

- **Markdown format**: System prompt content comes from the markdown body after the frontmatter
- **YAML/JSON formats**: System prompt content is defined in the `system_prompt` field
- All formats support the same configuration options (temperature, tools, reasoning, etc.)
- The `system_prompt` field is required in YAML and JSON formats

## File Location

All agent configuration files should be placed in the agent directory (typically `~/.forge/agents/` or as configured by your environment).

## Migration

Existing markdown format agents continue to work without changes. You can now also create agents using YAML or JSON formats alongside your markdown agents.