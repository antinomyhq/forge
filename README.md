<!--
Tone: Maintain a professional and informative tone throughout. Ensure that explanations are clear and technical terms are used appropriately to engage a technical audience.
Best Practices:
- Use consistent terminology and formatting for commands and examples.
- Clearly highlight unique aspects of 'forge' to distinguish it from other tools.
-->

[![CI Status](https://img.shields.io/github/actions/workflow/status/antinomyhq/forge/ci.yml?style=for-the-badge)](https://github.com/antinomyhq/forge/actions)
[![GitHub Release](https://img.shields.io/github/v/release/antinomyhq/forge?style=for-the-badge)](https://github.com/antinomyhq/forge/releases)
[![Discord](https://img.shields.io/discord/1044859667798568962?style=for-the-badge&cacheSeconds=120&logo=discord)](https://discord.gg/kRZBPpkgwq)

**Forge: AI-Enhanced Terminal Development Environment**

Forge is a comprehensive coding agent that integrates AI capabilities with your development environment, offering sophisticated assistance while maintaining the efficiency of your existing workflow.

- Advanced AI coding assistant with comprehensive understanding, planning, and execution of complex development tasks
- Lightning-fast performance with sub-50ms startup times
- Seamless integration with existing Unix tools and workflows
- Context-aware assistance that understands your development environment and workflows
- Natural language interface to powerful system operations
- Enhanced security features with optional restricted shell mode
- Multi-agent architecture that orchestrates specialized AI agents to solve complex problems collaboratively
- Powered by Claude 3.7 Sonnet for state-of-the-art AI capabilities

**Table of Contents**

- [Installation](#installation)
  - [Mac](#mac)
  - [Linux](#linux)
- [Get Started](#get-started)
- [Features](#features)
  - [Complete Coding Agent](#complete-coding-agent)
  - [Interactive AI Shell](#interactive-ai-shell)
  - [Enhanced Security](#enhanced-security)
  - [Built-in Commands](#built-in-commands)
  - [Autocomplete](#autocomplete)
  - [WYSIWYG Shell Experience](#wysiwyg-shell-experience)
  - [Command Interruption](#command-interruption)
- [Custom Workflows and Multi-Agent Systems](#custom-workflows-and-multi-agent-systems)
  - [Creating Custom Workflows](#creating-custom-workflows)
  - [Workflow Configuration](#workflow-configuration)
    - [Event System](#event-system)
    - [Agent Tools](#agent-tools)
    - [Agent Configuration Options](#agent-configuration-options)
    - [Built-in Templates](#built-in-templates)
    - [Example Workflow Configuration](#example-workflow-configuration)
- [Why Shell?](#why-shell)
- [Community](#community)
- [Support Us](#support-us)

## Installation

### Mac

Using Homebrew (macOS package manager):

```bash
# Add Code-Forge's package repository to Homebrew
brew tap antinomyhq/code-forge
# Install Code-Forge
brew install code-forge
```

### Linux

Choose either method to install:

```bash
# Using curl (common download tool)
curl -L https://raw.githubusercontent.com/antinomyhq/forge/main/install.sh | bash

# Or using wget (alternative download tool)
wget -qO- https://raw.githubusercontent.com/antinomyhq/forge/main/install.sh | bash
```

## Get Started

1. Create a `.env` file in your home directory with your API credentials:

   ```bash
   # Your API key for accessing AI models (see Environment Configuration section)
   OPEN_ROUTER_KEY=<Enter your Open Router Key>
   ```

   _You can get a Key at [Open Router](https://openrouter.ai/)_

2. Launch Code Forge:

   ![Code-Forge Demo](https://antinomy.ai/images/forge_demo_2x.gif)

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

Additional security features include:

- Direct API connection to Open Router without intermediate servers
- Local terminal operation for maximum control and data privacy

### Built-in Commands

Forge offers several built-in commands to enhance your interaction:

- `\new` - Start a new task when you've completed your current one
- `\info` - View environment summary, logs folder location, and command history
- `\models` - List all available AI models with capabilities and context limits
- `\dump` - Save the current conversation in JSON format to a file for reference

### Autocomplete

Boost your productivity with intelligent command completion:

- Type `@` and press Tab for contextual file/path completion
- Use Right Arrow to complete previously executed commands
- Access command history with Up Arrow
- Quick history search with Ctrl+R

### WYSIWYG Shell Experience

Enhance your interactive shell experience with WYSIWYG (What You See Is What You Get) integration. 'forge' now visualizes each command executed, complete with colorful formatting, allowing you to see command outputs just as if you were typing them directly into your terminal. This feature ensures clarity and enhances interaction, making every command visible in rich detail.

### Command Interruption

Stay in control of your shell environment with intuitive command handling:

- **Cancel with `CTRL+C`:** Gracefully interrupt ongoing operations, providing the flexibility to halt processes that no longer need execution.
- **Exit with `CTRL+D`:** Easily exit the shell session without hassle, ensuring you can quickly terminate your operations when needed.

## Custom Workflows and Multi-Agent Systems

For complex tasks, a single agent may not be sufficient. Forge allows you to create custom workflows with multiple specialized agents working together to accomplish sophisticated tasks.

### Creating Custom Workflows

You can configure your own workflows by creating a YAML file and pointing to it with the `-w` flag:

```bash
forge -w /path/to/your/workflow.yaml
```

### Workflow Configuration

A workflow consists of agents connected via events. Each agent has specific capabilities and can perform designated tasks.

#### Event System

Agents communicate through events which they can publish and subscribe to:

**Built-in Events**

- `user_task_init` - Published when a new task is initiated
- `user_task_update` - Published when follow-up instructions are provided by the user

#### Agent Tools

Each agent needs tools to perform tasks, configured in the `tools` field:

**Built-in Tools**

- `tool_forge_fs_read` - Read from the filesystem
- `tool_forge_fs_create` - Create or overwrite files
- `tool_forge_fs_remove` - Remove files
- `tool_forge_fs_search` - Search for patterns in files
- `tool_forge_fs_list` - List files in a directory
- `tool_forge_fs_info` - Get file metadata
- `tool_forge_process_shell` - Execute shell commands
- `tool_forge_process_think` - Perform internal reasoning
- `tool_forge_net_fetch` - Fetch data from the internet
- `tool_forge_event_dispatch` - Dispatch events to other agents
- `tool_forge_fs_patch` - Patch existing files

#### Agent Configuration Options

- `id` - Unique identifier for the agent
- `model` - AI model to use (from the `\models` list)
- `tools` - List of tools the agent can use
- `subscribe` - Events the agent listens to
- `ephemeral` - If true, agent is destroyed after task completion
- `system_prompt` - (Optional) Instructions for how the agent should behave. While optional, it's recommended to provide clear instructions for best results.
- `user_prompt` - (Optional) Format for user inputs. If not provided, the raw event value is used.

#### Built-in Templates

Forge provides templates to simplify system prompt creation:

- `system-prompt-engineer.hbs` - Template for engineering tasks
- `system-prompt-title-generator.hbs` - Template for generating descriptive titles
- `system-prompt-advocate.hbs` - Template for user advocacy and explanation
- `partial-tool-information.hbs` - Tool documentation for agents
- `partial-tool-examples.hbs` - Usage examples for tools

Use these templates with the syntax: `{{> name-of-the-template.hbs }}`

#### Example Workflow Configuration

```yaml
variables:
  models:
    advanced_model: &advanced_model anthropic/claude-3.7-sonnet
    efficiency_model: &efficiency_model anthropic/claude-3.5-haiku

agents:
  - id: title_generation_worker
    model: *efficiency_model
    tools:
      - tool_forge_event_dispatch
    subscribe:
      - user_task_init
    system_prompt: "{{> system-prompt-title-generator.hbs }}"
    user_prompt: <technical_content>{{event.value}}</technical_content>

  - id: developer
    model: *advanced_model
    tools:
      - tool_forge_fs_read
      - tool_forge_fs_create
      - tool_forge_fs_remove
      - tool_forge_fs_patch
      - tool_forge_process_shell
      - tool_forge_net_fetch
      - tool_forge_fs_search
    subscribe:
      - user_task_init
      - user_task_update
    ephemeral: false
    system_prompt: "{{> system-prompt-engineer.hbs }}"
    user_prompt: |
      <task>{{event.value}}</task>
```

This example workflow creates two agents:

1. A title generation worker that creates meaningful titles for user conversations
2. A developer agent that can perform comprehensive file and system operations

## Why Shell?

There's a reason why the shell has stood the test of time for development tools and remains a cornerstone of development environments across the globe: it's fast, versatile, and seamlessly integrated with the system. The shell is where developers navigate code, run tests, manage processes, and orchestrate development environments, providing an unmatched level of control and productivity.

**Why a shell-based AI assistant like Code-Forge makes sense:**

- **Rich Tool Ecosystem**: The shell gives you immediate access to powerful Unix tools (grep, awk, sed, find) that LLMs already understand deeply. This means the AI can leverage `ripgrep` for code search, `jq` for JSON processing, `git` for version control, and hundreds of other battle-tested tools without reinventing them.

- **Context is Everything**: Your shell session already has your complete development context - current directory, project structure, environment variables, installed tools, and system state. This rich context makes the AI interactions more accurate and relevant.

- **Speed Matters**: Unlike IDEs and Web UIs, Code Forge's shell is extremely lightweight. This exceptional speed unlocks powerful capabilities that directly enhance your productivity: seamlessly get in and out of workflows, manage multiple feature developments in parallel, effortlessly coordinate across git worktrees, and instantly access AI assistance in any directory.

- **Tool Composition**: Unix philosophy teaches us to make tools that compose well. The AI can pipe commands together, combining tools like `find | xargs forge -p | grep "foo"` in ways that solve complex problems elegantly.

## Community

Join our vibrant Discord community to connect with other Code-Forge users and contributors, get help with your projects, share ideas, and provide feedback!

[![Discord](https://img.shields.io/discord/1044859667798568962?style=for-the-badge&cacheSeconds=120&logo=discord)](https://discord.gg/kRZBPpkgwq)

## Support Us

Your support drives Code-Forge's continued evolution! By starring our GitHub repository, you:

- Help others discover this powerful tool
- Motivate our development team
- Enable us to prioritize new features
- Strengthen our open-source community

Recent community feedback has helped us implement features like improved autocomplete, cross-platform optimization, and enhanced security features. Join our growing community of developers who are reshaping the future of AI-powered development!
