---
layout: page
title: Error Tracking
nav_order: 5
description: "Runtime error tracking in Code-Forge"
---

# Error Tracking

Code-Forge now includes error tracking to monitor issues that occur during AI-assisted development operations. This helps improve the reliability of AI-powered code generation, manipulation, and analysis tasks.

## Features

- **AI Operation Monitoring**
  - Track errors in code generation and manipulation
  - Monitor AI model integration issues
  - Capture tool execution failures
  - Track file system operation errors

## Usage

Track errors during AI operations:
```rust
use forge_tracker::track_error;

track_error(
    "CodeGenerationError",
    "Failed to generate code: Invalid syntax in template",
    "code_generator.generate_function"
).await?;
```

Track file system operation errors:
```rust
use forge_tracker::track_error_without_trace;

track_error_without_trace(
    "FileSystemError",
    "Failed to create file: Permission denied",
    "fs_tool.create_file"
).await?;
```

## Benefits

Error tracking helps improve Forge's AI assistance by:
- Identifying common failure patterns in AI operations
- Monitoring the reliability of different code manipulation tasks
- Helping prioritize improvements to AI models and tools
- Measuring the effectiveness of AI-related fixes

## Privacy

Error tracking respects Forge's tracking preferences. If tracking is disabled, no error information will be collected. 