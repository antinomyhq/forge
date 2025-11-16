---
id: "test-advanced"
title: "Advanced Test Agent"
description: "An advanced test agent with full configuration"
model: "claude-3-5-sonnet-20241022"
tool_supported: true
tools: ["fs_read", "fs_write", "shell"]
temperature: 0.7
top_p: 0.9
max_tokens: 2000
max_turns: 10
reasoning:
  enabled: true
  effort: "high"
  max_tokens: 1000
---

# Advanced Test Agent

This is an advanced test agent that demonstrates all configuration options available for agent definition.
