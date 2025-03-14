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

![Code-Forge Demo](https://assets.antinomy.ai/images/forge_demo_2x.gif)

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
  - [Image Upload](#image-upload)
  - [WYSIWYG Shell Experience](#wysiwyg-shell-experience)
  - [Command Interruption](#command-interruption)
  - [Operation Modes](#operation-modes)
    - [ACT Mode (Default)](#act-mode-default)
    - [PLAN Mode](#plan-mode)
  - [Application Logs](#application-logs)
- [Provider Configuration](#provider-configuration)
  - [Supported Providers](#supported-providers)
  - [Custom Provider URLs](#custom-provider-urls)
- [Custom Workflows and Multi-Agent Systems](#custom-workflows-and-multi-agent-systems)
  - [Creating Custom Workflows](#creating-custom-workflows)
  - [Configuration Loading and Precedence](#configuration-loading-and-precedence)
  - [Workflow Configuration](#workflow-configuration)
    - [Event System](#event-system)
    - [Agent Tools](#agent-tools)
    - [Agent Configuration Options](#agent-configuration-options)
    - [Built-in Templates](#built-in-templates)
    - [Custom Commands](#custom-commands)
    - [Example Workflow Configuration](#example-workflow-configuration)
      - [Example 1: Using Event Value as Instructions](#example-1-using-event-value-as-instructions)
      - [Example 2: Using Event Value as Data in a Template](#example-2-using-event-value-as-data-in-a-template)
      - [Comparing the Two Approaches](#comparing-the-two-approaches)
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
   OPENROUTER_API_KEY=<Enter your Open Router Key>

   # Optional: Set a custom URL for OpenAI-compatible providers
   #OPENAI_URL=https://custom-openai-provider.com/v1
   ```

   _You can get a Key at [Open Router](https://openrouter.ai/)_

2. Launch Code Forge:

   ![Code-Forge Demo](https://assets.antinomy.ai/images/forge_demo_2x.gif)

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

- `/new` - Start a new task when you've completed your current one
- `/info` - View environment summary, logs folder location, and command history
- `/models` - List all available AI models with capabilities and context limits
- `/dump` - Save the current conversation in JSON format to a file for reference
- `/act` - Switch to ACT mode (default), allowing Forge to execute commands and implement changes
- `/plan` - Switch to PLAN mode, where Forge analyzes and plans but doesn't modify files

### Autocomplete

Boost your productivity with intelligent command completion:

- Type `@` and press Tab for contextual file/path completion
- Use Right Arrow to complete previously executed commands
- Access command history with Up Arrow
- Quick history search with Ctrl+R

### Image Upload

Easily incorporate images into your conversations:

- Use the `@` special character to tag and upload images directly in your messages
- Works with both relative and absolute paths:
  - Relative path: `@screenshots/bug.png` to include an image from a subfolder
  - Absolute path: `@/Users/username/Documents/diagrams/architecture.png`
- Perfect for sharing screenshots, diagrams, or any visual context relevant to your development tasks

### WYSIWYG Shell Experience

Enhance your interactive shell experience with WYSIWYG (What You See Is What You Get) integration. 'forge' now visualizes each command executed, complete with colorful formatting, allowing you to see command outputs just as if you were typing them directly into your terminal. This feature ensures clarity and enhances interaction, making every command visible in rich detail.

### Command Interruption

Stay in control of your shell environment with intuitive command handling:

- **Cancel with `CTRL+C`:** Gracefully interrupt ongoing operations, providing the flexibility to halt processes that no longer need execution.
- **Exit with `CTRL+D`:** Easily exit the shell session without hassle, ensuring you can quickly terminate your operations when needed.

### Operation Modes

Forge operates in two distinct modes to provide flexible assistance based on your needs:

#### ACT Mode (Default)

In ACT mode, which is the default when you start Forge, the assistant is empowered to directly implement changes to your codebase and execute commands:

- **Full Execution**: Forge can modify files, create new ones, and execute shell commands
- **Implementation**: Directly implements the solutions it proposes
- **Verification**: Performs verification steps to ensure changes work as intended
- **Best For**: When you want Forge to handle implementation details and fix issues directly

**Example**:

```bash
# Switch to ACT mode within a Forge session
/act
```

#### PLAN Mode

In PLAN mode, Forge analyzes and plans but doesn't modify your codebase:

- **Read-Only Operations**: Can only read files and run non-destructive commands
- **Detailed Analysis**: Thoroughly examines code, identifies issues, and proposes solutions
- **Structured Planning**: Provides step-by-step action plans for implementing changes
- **Best For**: When you want to understand what changes are needed before implementing them yourself

**Example**:

```bash
# Switch to PLAN mode within a Forge session
/plan
```

You can easily switch between modes during a session using the `/act` and `/plan` commands. PLAN mode is especially useful for reviewing potential changes before they're implemented, while ACT mode streamlines the development process by handling implementation details for you.

### Application Logs

Forge generates detailed JSON-formatted logs that help with troubleshooting and understanding the application's behavior. These logs provide valuable insights into system operations and API interactions.

**Log Location and Access**

Logs are stored in your application support directory with date-based filenames. The typical path looks like:

```bash
/Users/username/Library/Application Support/forge/logs/forge.log.YYYY-MM-DD
```

You can easily locate log files using the built-in command `/info`, which displays system information including the exact path to your log files.

**Viewing and Filtering Logs**

To view logs in real-time with automatic updates, use the `tail` command:

```bash
tail -f /Users/tushar/Library/Application Support/forge/logs/forge.log.2025-03-07
```

**Formatted Log Viewing with jq**

Since Forge logs are in JSON format, you can pipe them through `jq` for better readability:

```bash
tail -f /Users/tushar/Library/Application Support/forge/logs/forge.log.2025-03-07 | jq
```

This displays the logs in a nicely color-coded structure that's much easier to analyze, helping you quickly identify patterns, errors, or specific behavior during development and debugging.

## Provider Configuration

Forge supports multiple AI providers and allows custom configuration to meet your specific needs.

### Supported Providers

Forge automatically detects and uses your API keys from environment variables in the following priority order:

1. `FORGE_KEY` - Antinomy's provider (OpenAI-compatible)
2. `OPENROUTER_API_KEY` - Open Router provider (aggregates multiple models)
3. `OPENAI_API_KEY` - Official OpenAI provider
4. `ANTHROPIC_API_KEY` - Official Anthropic provider

To use a specific provider, set the corresponding environment variable in your `.env` file.

```bash
# Examples of different provider configurations (use only one)

# For Open Router (recommended, provides access to multiple models)
OPENROUTER_API_KEY=your_openrouter_key_here

# For official OpenAI
OPENAI_API_KEY=your_openai_key_here

# For official Anthropic
ANTHROPIC_API_KEY=your_anthropic_key_here

# For Antinomy's provider
FORGE_KEY=your_forge_key_here
```

### Custom Provider URLs

For OpenAI-compatible providers (including Open Router), you can customize the API endpoint URL by setting the `OPENAI_URL` environment variable:

```bash
# Custom OpenAI-compatible provider
OPENAI_API_KEY=your_api_key_here
OPENAI_URL=https://your-custom-provider.com/v1

# Or with Open Router but custom endpoint
OPENROUTER_API_KEY=your_openrouter_key_here
OPENAI_URL=https://alternative-openrouter-endpoint.com/v1
```

This is particularly useful when:

- Using self-hosted models with OpenAI-compatible APIs
- Connecting to enterprise OpenAI deployments
- Using proxy services or API gateways
- Working with regional API endpoints

## Custom Workflows and Multi-Agent Systems

For complex tasks, a single agent may not be sufficient. Forge allows you to create custom workflows with multiple specialized agents working together to accomplish sophisticated tasks.

### Creating Custom Workflows

You can configure your own workflows by creating a YAML file and pointing to it with the `-w` flag:

```bash
forge -w /path/to/your/workflow.yaml
```

### Configuration Loading and Precedence

Forge loads workflow configurations using the following precedence rules:

1. **Explicit Path**: When a path is provided with the `-w` flag, Forge loads that configuration directly without any merging
2. **Project Configuration**: If no explicit path is provided, Forge looks for `forge.yaml` in the current directory
3. **Default Configuration**: An embedded default configuration is always available as a fallback

When a project configuration exists in the current directory, Forge creates a merged configuration where:

- Project settings in `forge.yaml` take precedence over default settings
- Any settings not specified in the project configuration inherit from defaults

This approach allows you to customize only the parts of the configuration you need while inheriting sensible defaults for everything else.

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
- `project_rules` - (Optional) Clear instructions or guidelines that define the rules for the agent. This section ensures the agent follows the desired processes and complies with any specific conditions or constraints set.
- `tool_supported` - (Optional) Boolean flag that determines whether tools defined in the agent configuration are actually made available to the LLM. When set to `false`, tools are listed in the configuration but not included in AI model requests, causing the agent to format tool calls in XML rather than in the model's native format. Default: `true`.
- `system_prompt` - (Optional) Instructions for how the agent should behave. While optional, it's recommended to provide clear instructions for best results.
- `user_prompt` - (Optional) Format for user inputs. If not provided, the raw event value is used.

**Example Agent Configuration:**

```yaml
agents:
  - id: software-engineer
    model: gpt-4
    project_rules: always ensure you compile and run tests before presenting to user
    system_prompt: |
      You are a software engineer...
```

#### Built-in Templates

Forge provides templates to simplify system prompt creation:

- `system-prompt-engineer.hbs` - Template for engineering tasks
- `system-prompt-title-generator.hbs` - Template for generating descriptive titles
- `system-prompt-advocate.hbs` - Template for user advocacy and explanation
- `partial-tool-information.hbs` - Tool documentation for agents
- `partial-tool-examples.hbs` - Usage examples for tools

Use these templates with the syntax: `{{> name-of-the-template.hbs }}`

#### Custom Commands

Forge allows you to define custom commands in your workflow configuration. These commands can be executed within the Forge CLI using the `/command_name` syntax.

**Configuration Options:**

- `name` - The name of the command (used as `/name` in the CLI)
- `description` - A description of what the command does
- `value` - (Optional) A default prompt value that will be used if no arguments are provided when executing the command

**Example Custom Command Configuration:**

```yaml
commands:
  - name: commit
    description: Commit changes with a standard prefix
    value: |
      Understand the diff produced and commit using the 'conventional commit' standard

  - name: branch
    description: Create and checkout a new branch

  - name: pull-request
    description: Create a pull request with standard template
    value: |
      Understand the diff with respect to `main` and create a pull-request.
      Ensure it follows 'conventional commit' standard.
```

With this configuration, users can type `/commit` in the Forge CLI to execute the commit command with the default instructions for handling commits using the conventional commit standard. If specific instructions are needed, they can be provided as an argument: `/commit Create a detailed commit message for the login feature`. Commands without a default value like `/branch` require an argument to be provided: `/branch feature/new-auth`.

**How Custom Commands Work:**

**How Custom Commands Work With the Event System:**

When a custom command is executed in the Forge CLI, it follows a specific event flow:

1. **Command Execution** - User types a command like `/commit feat: add user authentication`
2. **Event Dispatch** - Forge dispatches an event with:
   - Name: The command name (e.g., `commit`)
   - Value: The provided argument or default value (e.g., `feat: add user authentication`)
3. **Agent Subscription** - Any agent that has subscribed to this event name receives the event
4. **Event Processing** - The agent processes the event according to its configuration

For an agent to respond to a custom command, it must explicitly subscribe to the event with the same name as the command in its configuration. The agent can then use conditional logic in its user prompt to handle different types of events appropriately.

#### Example Workflow Configuration

Forge provides two main approaches for handling custom command events in agents. Below are examples of both approaches.

##### Example 1: Using Event Value as Instructions

In this approach, the event value itself contains complete instructions that are passed directly to the agent:

```yaml
variables:
  models:
    advanced_model: &advanced_model anthropic/claude-3.7-sonnet
    efficiency_model: &efficiency_model anthropic/claude-3.5-haiku

commands:
  - name: commit
    description: Commit changes with a standard prefix
    value: |
      Understand the diff produced and commit using the 'conventional commit' standard

  - name: pull-request
    description: Create a pull request with standard template
    value: |
      Understand the diff with respect to `main` and create a pull-request.
      Ensure it follows 'conventional commit' standard.

agents:
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
      - commit # Subscribe to the commit command event
      - pull-request # Subscribe to the pull-request command event
    ephemeral: false
    tool_supported: true
    system_prompt: "{{> system-prompt-engineer.hbs }}"
    user_prompt: |
      <task>{{event.value}}</task>
```

In this example, the entire value from the event is passed directly as the task. The agent receives the complete instructions as they were defined in the command value or provided by the user.

##### Example 2: Using Event Value as Data in a Template

In this approach, the event value is used as data within a template that formats different tasks based on the event name:

```yaml
variables:
  models:
    advanced_model: &advanced_model anthropic/claude-3.7-sonnet
    efficiency_model: &efficiency_model anthropic/claude-3.5-haiku

commands:
  - name: commit
    description: Create a git commit with the provided message

  - name: pull-request
    description: Create a pull request with the provided title

agents:
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
      - commit # Subscribe to the commit command event
      - pull-request # Subscribe to the pull-request command event
    ephemeral: false
    tool_supported: true
    system_prompt: "{{> system-prompt-engineer.hbs }}"
    user_prompt: |
      {{#if (eq event.name "commit")}}
      <task>Create a git commit with the following message: {{event.value}}</task>
      {{else if (eq event.name "pull-request")}}
      <task>Create a pull request with the title: {{event.value}}</task>
      {{else}}
      <task>{{event.value}}</task>
      {{/if}}
```

This example, the event value is a simpler string that gets embedded within a template. The template uses Handlebars conditional logic (`{{#if (eq event.name "commit")}}`) to format different tasks based on the event name. The event value is used as data within these task templates.

##### Comparing the Two Approaches

**Approach 1: Event Value as Instructions**

- **Best for**: When the command itself represents a complete task or instruction set
- **Flexibility**: Users can provide detailed, multi-line instructions via the command
- **Implementation**: Simpler user_prompt template that just passes the event value through
- **Example use case**: Complex operations where instructions vary significantly

**Approach 2: Event Value as Data**

- **Best for**: When commands follow predictable patterns with varying data points
- **Structure**: More consistent task formatting across different command types
- **Implementation**: More complex user_prompt template with conditional logic
- **Example use case**: Standardized workflows like git operations with varying messages/titles

You can choose the approach that best fits your specific workflow needs. For simple command structures, Approach 2 provides more consistency, while Approach 1 offers greater flexibility for complex operations.

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
