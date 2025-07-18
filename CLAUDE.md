# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Forge is an AI-enhanced terminal development environment built in Rust. It provides a comprehensive coding agent that integrates AI capabilities with your development environment. The project follows a multi-crate workspace architecture with clearly separated concerns.

## Essential Commands

### Building and Testing
```bash
# Format code
cargo +nightly fmt --all

# Run linting and fixes
cargo +nightly clippy --fix --allow-staged --allow-dirty --workspace

# Run tests with snapshots
cargo insta test --accept --unreferenced=delete

# Combined verification (format + lint)
cargo +nightly fmt --all; cargo +nightly clippy --fix --allow-staged --allow-dirty --workspace
```

### Running the Application
```bash
# Build and run
cargo run

# Install and run via npm (development)
npx forgecode@latest

# Run with specific options
cargo run -- --help
cargo run -- --restricted  # restricted shell mode
cargo run -- --verbose     # verbose output
```

## Architecture Overview

### Crate Structure
The project uses a workspace with specialized crates:

- **forge_main**: CLI entry point and user interface
- **forge_app**: Core application logic, agent execution, and orchestration
- **forge_domain**: Domain models, types, and business logic
- **forge_services**: Service layer including tool services and business operations
- **forge_provider**: AI provider integrations (OpenAI, Anthropic, etc.)
- **forge_infra**: Infrastructure concerns (HTTP, filesystem, execution)
- **forge_api**: API layer and external interfaces
- **forge_display**: Output formatting and display logic
- **forge_fs**: Filesystem utilities and operations
- **forge_tracker**: Analytics and tracking
- **forge_ci**: CI/CD utilities and workflows

### Key Components

- **Agent System**: Multi-agent workflow execution with context management
- **Tool Registry**: Centralized tool registration and execution (forge_services/src/tools/registry.rs)
- **MCP Integration**: Model Context Protocol for external tool communication
- **Provider Abstraction**: Multi-provider AI model support
- **Context Management**: Intelligent context summarization and compaction

## Development Guidelines

### Error Handling
- Use `anyhow::Result` for error handling in services and repositories
- Create domain errors using `thiserror`
- Never implement `From` for converting domain errors, manually convert them

### Testing Standards
- All tests follow a three-step pattern: fixture → actual → expected
- Use `pretty_assertions::assert_eq!` for better error messages
- Tests are co-located with source code in the same file
- Use `insta` for snapshot testing: `cargo insta test --accept --unreferenced=delete`
- Fixtures should be generic and reusable

### Domain Types
- Use `derive_setters` with `strip_option` and `into` attributes on struct types

### Tool Development
- Tool descriptions must be extremely detailed (see docs/tool-guidelines.md)
- All tools must be registered in forge_services/src/tools/registry.rs
- Tool descriptions must not exceed 1024 characters
- Include when/when not to use, parameter meanings, and limitations

## Configuration

### forge.yaml
The main configuration file supports:
- Custom commands and rules
- Model selection and parameters
- Agent tool assignments
- Temperature and context limits

### Available Commands
- `fixme`: Find and fix FIXME comments
- `pr-description`: Update PR title and description with conventional commits
- `check`: Run lint and test commands to verify code readiness

## MCP (Model Context Protocol)
Configure external tool integrations via `.mcp.json` or `forge mcp` CLI commands for browser automation, API interactions, and custom service connections.

## Important Files
- `forge.yaml`: Main configuration
- `Cargo.toml`: Workspace definition
- `crates/forge_services/src/tools/registry.rs`: Tool registration
- `docs/tool-guidelines.md`: Tool development best practices