# Code-Forge 🛠️

A sophisticated AI-powered coding assistant platform built in Rust, designed to provide intelligent code generation, manipulation, and analysis capabilities through a modular and extensible architecture.

## ✨ Features

- 🤖 **AI-Powered Code Generation** - Advanced code generation and manipulation using modern AI models
- 🔍 **Smart Code Analysis** - Language-aware parsing and analysis for multiple programming languages
- 🛠️ **Extensive Tool System** - Rich set of development tools including file operations, shell commands, and code outline generation
- 💾 **Persistent Conversations** - Maintain context and history across coding sessions  
- 🔒 **Secure Operations** - Built-in security measures for file system and shell operations
- 🔌 **Extensible Architecture** - Modular design supporting easy addition of new features and languages

## 🚀 Setup

### Prerequisites

- Rust toolchain (1.75+)
- SQLite
- Tree-sitter (for code analysis)

### Installation

```bash
# Build the project
cargo build --release

# Run the server
cargo run --release
```

## 🏗️ Project Structure

```
code-forge/
├── crates/
│   ├── forge_main/        # CLI and main application logic
│   ├── forge_domain/      # Core domain models and interfaces
│   ├── forge_provider/    # AI provider integration
│   ├── forge_services/      # HTTP API and database management
│   ├── forge_tool/        # Tool implementations
│   └── forge_walker/      # File system operations
```

## 🛠️ Core Components

- **Domain Layer** (`forge_domain`) - Core business logic and interfaces
- **Provider Layer** (`forge_provider`) - AI model integration
- **Tool Layer** (`forge_tool`) - Development tools implementation
- **Server Layer** (`forge_services`) - API endpoints and persistence
- **Main Application** (`forge_main`) - CLI and application coordination

## 🔧 Configuration

The application requires several environment variables for proper operation:

```bash
# Required environment variables
DATABASE_URL="sqlite:path/to/database.db"
OPENROUTER_API_KEY="your-api-key"
```

## 📚 Documentation

Internal documentation:
- [Onboarding Guide](docs/onboarding.md)
- [Architecture Overview](docs/architecture.md)

## 🔒 Proprietary Software

This is proprietary software. All rights reserved.