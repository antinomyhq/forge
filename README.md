[![CI Status](https://img.shields.io/github/actions/workflow/status/antinomyhq/forge/ci.yml?style=for-the-badge)](https://github.com/antinomyhq/forge/actions)
[![GitHub Release](https://img.shields.io/github/v/release/antinomyhq/forge?style=for-the-badge)](https://github.com/antinomyhq/forge/releases)

**A powerful AI-driven coding assistant integrated directly into your terminal** — designed to enhance your development workflow through intelligent assistance with coding tasks, debugging, and automation.

Forge is a comprehensive coding agent that integrates AI capabilities with your development environment, offering sophisticated assistance while maintaining the efficiency of your existing workflow.

- **Advanced Coding Agent** — An intelligent assistant capable of understanding, planning, and executing complex coding tasks
- **Performance Optimized** — Sub-50ms startup time ensures seamless integration into your workflow
- **Toolchain Integration** — Works with standard Unix tools and adapts to existing development environments
- **Contextual Awareness** — Analyzes project structure, files, and dependencies to provide relevant solutions
- **Natural Language Interface** — Convert specifications into functional code, tests, and documentation
- **Security-Focused** — Optional restricted shell mode for environments with elevated security requirements
- **Powered by Claude 3.7 Sonnet** — Leverages cutting-edge AI for advanced programming assistance

**Table of Contents**

- [Quick Install](#quick-install)
  - [Mac](#mac)
  - [Linux](#linux)
- [Get Started in 30 Seconds](#get-started-in-30-seconds)
- [See It In Action](#see-it-in-action)
- [Features](#features)
  - [Complete Coding Agent](#complete-coding-agent)
  - [Interactive AI Shell](#interactive-ai-shell)
  - [Enhanced Security](#enhanced-security)
  - [Autocomplete](#autocomplete)
  - [WYSIWYG Shell Experience](#wysiwyg-shell-experience)
  - [Seamless Control](#seamless-control)
- [Why Shell?](#why-shell)
- [❤️ Support Code Forge](#️-support-code-forge)

## Quick Install

### Mac

```bash
# One command to get started
brew tap antinomyhq/code-forge && brew install code-forge
```

### Linux

```bash
# Quick install with curl
curl -L https://raw.githubusercontent.com/antinomyhq/forge/main/install.sh | bash
```

## Get Started in 30 Seconds

1. Set up your API key (one-time step):

   ```bash
   # Create a .env file in your home directory
   echo "OPEN_ROUTER_KEY=<your-key-here>" > ~/.env
   ```

   _You can get a Key at [Open Router](https://openrouter.ai/)_

2. Launch Code Forge:
   ```bash
   forge
   ⚡ # Now just describe what you want to build or solve!
   ```

## See It In Action

Instead of manually coding tedious tasks, let Code Forge handle them:

```bash
⚡ forge Task 1
```

<!-- <Video here> -->

```bash
⚡ forge Task 2
```

<!-- <Video here> -->

```bash
⚡ forge Task 3
```

<!-- <Video here> -->

## Features

### Complete Coding Agent

Code Forge functions as a comprehensive development assistant with capabilities to:

- Write, refactor, and optimize code based on specifications
- Debug complex issues through systematic error analysis
- Generate test suites for existing codebases
- Document code and generate technical specifications
- Propose architectural improvements and optimizations

### Interactive AI Shell

Transform your command-line experience with natural language interaction while maintaining the power and flexibility of traditional shell commands.

### Enhanced Security

Code-Forge prioritizes security by providing a restricted shell mode (rbash) that limits potentially dangerous operations:

- **Flexible Security Options**: Choose between standard and restricted modes based on your needs
- **Restricted Mode**: Enable with `-r` flag to prevent potentially harmful operations
- **Standard Mode**: Uses regular shell by default (bash on Unix/Mac, cmd on Windows)
- **Security Controls**: Restricted mode prevents:
  - Changing directories
  - Setting/modifying environment variables
  - Executing commands with absolute paths
  - Modifying shell options

**Example**:

```bash
# Standard mode (default)
forge

# Restricted secure mode
forge -r
```

### Autocomplete

Boost your productivity with intelligent command completion:

- Type `@` and press Tab for contextual file/path completion
- Use Right Arrow to complete previously executed commands
- Access command history with Up Arrow
- Quick history search with Ctrl+R

### WYSIWYG Shell Experience

Enhance your interactive shell experience with WYSIWYG (What You See Is What You Get) integration. 'forge' now visualizes each command executed, complete with colorful formatting, allowing you to see command outputs just as if you were typing them directly into your terminal. This feature ensures clarity and enhances interaction, making every command visible in rich detail.

### Seamless Control

Stay in control of your shell environment with intuitive command handling:

- **Cancel with `CTRL+C`:** Gracefully interrupt ongoing operations, providing the flexibility to halt processes that no longer need execution.
- **Exit with `CTRL+D`:** Easily exit the shell session without hassle, ensuring you can quickly terminate your operations when needed.

## Why Shell?

There's a reason why the shell stood the test of time for all dev tools and still remains a cornerstone of development environments across the globe: it's fast, versatile, and seamlessly integrated with the system. The shell is where we navigate code, run tests, manage processes, and orchestrate our development environments, providing an unmatched level of control and productivity.

**The advantages of a shell-based AI assistant:**

Code Forge combines the power of AI with the command line to create a development experience that's greater than the sum of its parts:

- **Rich Tool Ecosystem**: The shell gives you immediate access to powerful Unix tools (grep, awk, sed, find) that LLMs already understand deeply. This means the AI can leverage `ripgrep` for code search, `jq` for JSON processing, `git` for version control, and hundreds of other battle-tested tools without reinventing them.

- **Context is Everything**: Your shell session already has your complete development context - current directory, project structure, environment variables, installed tools, and system state. This rich context makes the AI interactions more accurate and relevant.

- **Speed Matters**: Unlike IDEs and Web UI, Code Forge's shell is extremely light weight. This exceptional speed unlocks powerful capabilities that directly enhance your productivity: seamlessly get in and out of workflows, managing multiple feature developments in parallel, effortlessly coordinate across git worktrees, and instantly access AI assistance in any directory.

- **Tool Composition**: Unix philosophy teaches us to make tools that compose well. The AI can pipe commands together, combining tools like `find | xargs forge -p | grep "foo"` in ways that solve complex problems elegantly.

## ❤️ Support Code Forge

Your support drives Code-Forge's continued evolution! By starring our GitHub repository, you:

- Help others discover this powerful tool
- Motivate our development team
- Enable us to prioritize new features
- Strengthen our open-source community

Recent community feedback has helped us implement features like improved autocomplete, cross-platform optimization, and enhanced security features. Join our growing community of developers who are reshaping the future of AI-powered development!
