# Code-Forge ⚡

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg?style=for-the-badge)](https://opensource.org/licenses/Apache-2.0)
[![Rust](https://img.shields.io/badge/Rust-1.70%2B-orange.svg?style=for-the-badge)](https://www.rust-lang.org)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg?style=for-the-badge)](CONTRIBUTING.md)
[![CI Status](https://img.shields.io/github/actions/workflow/status/antinomyhq/forge/ci.yml?style=for-the-badge)](https://github.com/antinomyhq/forge/actions)
[![GitHub Release](https://img.shields.io/github/v/release/antinomyhq/forge?style=for-the-badge)](https://github.com/antinomyhq/forge/releases)
[![Last Commit](https://img.shields.io/github/last-commit/antinomyhq/forge?style=for-the-badge)](https://github.com/antinomyhq/forge/commits)
[![Open Issues](https://img.shields.io/github/issues/antinomyhq/forge?style=for-the-badge)](https://github.com/antinomyhq/forge/issues)
[![Open PRs](https://img.shields.io/github/issues-pr/antinomyhq/forge?style=for-the-badge)](https://github.com/antinomyhq/forge/pulls)
[![Repo Size](https://img.shields.io/github/repo-size/antinomyhq/forge?style=for-the-badge)](https://github.com/antinomyhq/forge)

Software engineers have been using shells (bash, zsh, fish) as their primary interface for decades because they're fast, powerful, and close to the system. The shell is our natural habitat - it's where we navigate code, run tests, manage processes, and orchestrate our development environment. That's why Code-Forge reimagines the shell as an AI-powered environment where you can interact with an intelligent agent using natural language.

Why a shell-based AI assistant makes sense:

- **Rich Tool Ecosystem**: The shell gives you immediate access to powerful Unix tools (grep, awk, sed, find) that LLMs already understand deeply. This means the AI can leverage `ripgrep` for code search, `jq` for JSON processing, `git` for version control, and hundreds of other battle-tested tools without reinventing them.

- **Context is Everything**: Your shell session already has your complete development context - current directory, project structure, environment variables, and system state. This rich context makes the AI interactions more accurate and relevant.

- **Speed Matters**: Code-Forge revolutionizes development workflows through its Rust-powered performance, delivering immediate value with sub-50ms startup times. This exceptional speed unlocks powerful capabilities that directly enhance your productivity: seamlessly manage multiple feature developments in parallel, effortlessly coordinate across git worktrees, and instantly access AI assistance in any directory. By eliminating the frustrating delays and context switches common to web-based AI assistants, Code-Forge keeps you in your flow state. The result? A development experience where your tools match your thinking speed, enabling faster iterations, smoother task transitions, and more efficient project management across all your development contexts.

- **Workflow Integration**: Engineers context-switch about 13.3 times per hour between tools (according to Microsoft research). A shell-based AI assistant stays in your existing workflow - no need to switch windows, lose context, or break your flow.

- **Tool Composition**: Unix philosophy teaches us to make tools that compose well. The AI can pipe commands together, combining tools like `find | xargs | sort | uniq` in ways that solve complex problems elegantly.

- **Ephemeral by Default**: Unlike chat interfaces that accumulate state, shell sessions are naturally ephemeral and reproducible. This matches how developers work - focused sessions for specific tasks, with reliable, reproducible environments.

Think of Code-Forge as your productivity amplifier, where natural communication meets system-level power. By eliminating the cognitive overhead of command memorization, it lets you focus on what matters - solving problems and building features. You gain immediate access to the entire Unix toolkit through simple, conversational requests, while sophisticated code analysis and generation capabilities accelerate your development workflow. This natural interaction layer transforms complicated system operations into intuitive conversations, making development more accessible and efficient without sacrificing any of the power that experienced developers expect.

## 📦 Getting Started

Code-Forge comes pre-installed on your system. Just open your terminal and run:

```bash
forge
```

## 💬 Example Conversations

<!-- VIDEO_DEMO -->
<div align="center">
  <a href="https://www.youtube.com/watch?v=PLACEHOLDER">
    <img src="https://img.youtube.com/vi/PLACEHOLDER/maxresdefault.jpg" alt="Code-Forge Demo" style="max-width:600px;">
  </a>
</div>

Here's how natural conversations with Code-Forge work:

```bash
$ forge
Welcome to Code-Forge! How can I help you today?

⚡ create a spec from github issue #237 and implement it
Fetching issue #237: "Add dark mode support with system preference detection"

Analyzing issue and creating specification...

📋 Technical Specification
------------------------
Feature: Dark Mode Implementation
Priority: High
Effort: Medium

Requirements:
1. Add system preference detection for dark mode
2. Create dark theme color palette
3. Implement theme switching mechanism
4. Add user preference override
5. Persist theme selection

Technical Details:
- Use prefers-color-scheme media query
- Theme switching without page reload
- CSS custom properties for colors
- LocalStorage for persistence
- Optional: Add transition animations

Acceptance Criteria:
- System preference detection works on load
- Manual theme switching works
- Theme persists across page reloads
- No UI flicker during theme switch
- Accessible color contrast ratios

Would you like me to implement this specification?

⚡ yes, please proceed
Creating implementation plan and executing...

1/5 Setting up theme detection...
✓ Added theme detection utility
✓ Implemented system preference hook
✓ Added initial tests

2/5 Creating theme definitions...
✓ Created dark theme palette
✓ Added CSS custom properties
✓ Documented color tokens

3/5 Building theme switcher...
✓ Added ThemeProvider component
✓ Implemented theme switching logic
✓ Created useTheme hook

4/5 Adding persistence...
✓ Implemented local storage
✓ Added migration for existing users
✓ Created persistence tests

5/5 Finalizing and testing...
✓ Added e2e tests
✓ Updated documentation
✓ Created PR #238

Implementation complete! Summary of changes:
- 8 new files created
- 12 files modified
- 23 tests added
- 100% test coverage maintained

Would you like me to open the pull request for review?

⚡ yes, and add tests for edge cases
Analyzing potential edge cases...

Adding tests for:
1. System preference changes while app is running
2. Network conditions during theme asset loading
3. Theme switch during form interaction
4. Multiple rapid theme toggles
5. SSR compatibility

✓ Created edge case test suite
✓ Updated PR #238 with additional tests
✓ Added performance benchmarks

Pull request is now ready for review:
https://github.com/your-repo/pulls/238

⚡ optimize the theme switching performance
Analyzing theme switching performance...

Current metrics:
- Switch time: 120ms
- Paint time: 85ms
- Layout shifts: 3

Running parallel optimizations:
1. Reducing CSS variables
2. Optimizing style recalculation
3. Adding containment boundaries

✓ Reduced switch time to 45ms
✓ Decreased paint time to 30ms
✓ Eliminated layout shifts

Changes pushed to PR #238. Would you like me to add the performance report?
```

## 🔧 Tools and Capabilities

### Code Analysis
```rust
// Instead of complex command-line flags, just ask naturally
⚡ show me an outline of this code
⚡ find all TODOs in the project
⚡ check if this code follows our style guide
```

### File Operations
```rust
// Natural language file operations
⚡ find all files containing the word "deprecated"
⚡ rename all test files to include the prefix "unit_"
⚡ create a backup of the config directory
```

### Shell Integration
```rust
// Combine AI with traditional shell power
⚡ optimize all images in the assets folder
⚡ set up a new React project with TypeScript
⚡ show me the largest files in this directory
```

## 👩‍💻 Contributing

For developers who want to contribute to Code-Forge:

### Development Setup

```bash
# Clone the repository
git clone https://github.com/antinomyhq/forge.git
cd forge

# Set up environment variables
cp .env.example .env
# Edit .env with your settings

# Build and run for development
cargo run

# Run tests
cargo test
```

### Project Structure

```
code-forge/
├── forge_domain    # Core domain models and interfaces
├── forge_tool      # Tool implementations and utilities
├── forge_app       # HTTP API and persistence layer
└── forge_provider  # AI provider integrations
```

### Key Components

- **Domain Layer**: Core business logic and interfaces
- **Tool Layer**: File operations, code analysis, shell integration
- **Provider Layer**: AI model integration and streaming responses
- **App Layer**: API endpoints and persistence management

### Development Workflow

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Run tests (`cargo test`)
5. Commit your changes (`git commit -m 'Add amazing feature'`)
6. Push to your fork (`git push origin feature/amazing-feature`)
7. Open a Pull Request

### Development Commands

```bash
# Run with development features
cargo run --features development

# Generate documentation
cargo doc --open

# Run specific test suite
cargo test --package forge_tool
```

## 📚 Documentation

- [User Guide](docs/user-guide.md)
- [Architecture Guide](docs/architecture.md)
- [Tool Development](docs/tools.md)
- [API Reference](docs/api.md)
- [Security Guide](docs/security.md)

## 🔐 Security

Code-Forge takes security seriously:

- Path validation and sanitization
- Command whitelisting
- Permission checks
- Content validation
- Safe file operations

## 📝 License

Copyright 2024 Code-Forge Contributors

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.