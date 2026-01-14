<h1 align="center">⚒️ Forge: AI-Enhanced Terminal Development Environment</h1>
<p align="center">A comprehensive coding agent that integrates AI capabilities with your development environment</p>

<p align="center"><code>npx forgecode@latest</code></p>

[![CI Status](https://img.shields.io/github/actions/workflow/status/antinomyhq/forge/ci.yml?style=for-the-badge)](https://github.com/antinomyhq/forge/actions)
[![GitHub Release](https://img.shields.io/github/v/release/antinomyhq/forge?style=for-the-badge)](https://github.com/antinomyhq/forge/releases)
[![Discord](https://img.shields.io/discord/1044859667798568962?style=for-the-badge&cacheSeconds=120&logo=discord)](https://discord.gg/kRZBPpkgwq)
[![CLA assistant](https://cla-assistant.io/readme/badge/antinomyhq/forge?style=for-the-badge)](https://cla-assistant.io/antinomyhq/forge)

![Code-Forge Demo](https://assets.antinomy.ai/images/forge_demo_2x.gif)

---

<details>
<summary><strong>Table&nbsp;of&nbsp;Contents</strong></summary>

- [Quickstart](#quickstart)
- [Usage Examples](#usage-examples)
- [Why Forge?](#why-forge)
- [Command-Line Options](#command-line-options)
- [Advanced Configuration](#advanced-configuration)
  - [Provider Configuration](#provider-configuration)
    - [Managing Provider Credentials](#managing-provider-credentials)
    - [Deprecated: Environment Variables](#deprecated-environment-variables)
  - [forge.yaml Configuration Options](#forgeyaml-configuration-options)
  - [Environment Variables](#environment-variables)
  - [MCP Configuration](#mcp-configuration)
  - [Example Use Cases](#example-use-cases)
  - [Usage in Multi-Agent Workflows](#usage-in-multi-agent-workflows)
- [Documentation](#documentation)
- [Community](#community)
- [Support Us](#support-us)

</details>

---

## Quickstart

To get started with Forge, run the command below:

```bash
npx forgecode@latest
```

On first run, Forge will guide you through setting up your AI provider credentials using the interactive login flow. Alternatively, you can configure providers beforehand:

```bash
# Configure your provider credentials interactively
forge provider login

# Then start Forge
forge
```

That's it! Forge is now ready to assist you with your development tasks.

## Usage Examples

Forge can be used in different ways depending on your needs. Here are some common usage patterns:

<details>
<summary><strong>Code Understanding</strong></summary>

```
> Can you explain how the authentication system works in this codebase?
```

Forge will analyze your project's structure, identify authentication-related files, and provide a detailed explanation of the authentication flow, including the relationships between different components.

</details>

<details>
<summary><strong>Implementing New Features</strong></summary>

```
> I need to add a dark mode toggle to our React application. How should I approach this?
```

Forge will suggest the best approach based on your current codebase, explain the steps needed, and even scaffold the necessary components and styles for you.

</details>

<details>
<summary><strong>Debugging Assistance</strong></summary>

```
> I'm getting this error: "TypeError: Cannot read property 'map' of undefined". What might be causing it?
```

Forge will analyze the error, suggest potential causes based on your code, and propose different solutions to fix the issue.

</details>

<details>
<summary><strong>Code Reviews</strong></summary>

```
> Please review the code in src/components/UserProfile.js and suggest improvements
```

Forge will analyze the code, identify potential issues, and suggest improvements for readability, performance, security, and maintainability.

</details>

<details>
<summary><strong>Learning New Technologies</strong></summary>

```
> I want to integrate GraphQL into this Express application. Can you explain how to get started?
```

Forge will provide a tailored tutorial on integrating GraphQL with Express, using your specific project structure as context.

</details>

<details>
<summary><strong>Database Schema Design</strong></summary>

```
> I need to design a database schema for a blog with users, posts, comments, and categories
```

Forge will suggest an appropriate schema design, including tables/collections, relationships, indexes, and constraints based on your project's existing database technology.

</details>

<details>
<summary><strong>Refactoring Legacy Code</strong></summary>

```
> Help me refactor this class-based component to use React Hooks
```

Forge can help modernize your codebase by walking you through refactoring steps and implementing them with your approval.

</details>

<details>
<summary><strong>Git Operations</strong></summary>

```
> I need to merge branch 'feature/user-profile' into main but there are conflicts
```

Forge can guide you through resolving git conflicts, explaining the differences and suggesting the best way to reconcile them.

</details>

## Why Forge?

Forge is designed for developers who want to enhance their workflow with AI assistance while maintaining full control over their development environment.

- **Zero configuration** - Just add your API key and you're ready to go
- **Seamless integration** - Works right in your terminal, where you already work
- **Multi-provider support** - Use OpenAI, Anthropic, or other LLM providers
- **Secure by design** - Your code stays on your machine
- **Open-source** - Transparent, extensible, and community-driven

Forge helps you code faster, solve complex problems, and learn new technologies without leaving your terminal.

## Command-Line Options

Here's a quick reference of Forge's command-line options:

| Option                          | Description                                                |
| ------------------------------- | ---------------------------------------------------------- |
| `-p, --prompt <PROMPT>`         | Direct prompt to process without entering interactive mode |
| `-c, --command <COMMAND>`       | Path to a file containing initial commands to execute      |
| `-w, --workflow <WORKFLOW>`     | Path to a file containing the workflow to execute          |
| `-e, --event <EVENT>`           | Dispatch an event to the workflow                          |
| `--conversation <CONVERSATION>` | Path to a file containing the conversation to execute      |
| `-r, --restricted`              | Enable restricted shell mode for enhanced security         |
| `--verbose`                     | Enable verbose output mode                                 |
| `-h, --help`                    | Print help information                                     |
| `-V, --version`                 | Print version                                              |

## Advanced Configuration

### Provider Configuration

Forge supports multiple AI providers. The recommended way to configure providers is using the interactive login command:

```bash
forge provider login
```

This will:

1. Show you a list of available providers
2. Guide you through entering the required credentials

#### Managing Provider Credentials

```bash
# Login to a provider (add or update credentials)
forge provider login

# Remove provider credentials
forge provider logout

# List supported providers
forge provider list
```

#### Deprecated: Environment Variables

> **⚠️ DEPRECATED**: Using `.env` files for provider configuration is deprecated and will be removed in a future version. Please use `forge provider login` instead.

For backward compatibility, Forge still supports environment variables. On first run, any credentials found in environment variables will be automatically migrated to file-based storage.

<details>
<summary><strong>Legacy Environment Variable Setup (Deprecated)</strong></summary>

<details>
<summary><strong>OpenRouter</strong></summary>

```bash
# .env
OPENROUTER_API_KEY=<your_openrouter_api_key>
```

</details>

<details>
<summary><strong>Requesty</strong></summary>

```bash
# .env
REQUESTY_API_KEY=<your_requesty_api_key>
```

</details>

<details>
<summary><strong>x-ai</strong></summary>

```bash
# .env
XAI_API_KEY=<your_xai_api_key>
```

</details>

<details>
<summary><strong>z.ai</strong></summary>

```bash
# .env
ZAI_API_KEY=<your_zai_api_key>

# Or for coding plan subscription
ZAI_CODING_API_KEY=<your_zai_coding_api_key>
```

</details>

<details>
<summary><strong>Cerebras</strong></summary>

```bash
# .env
CEREBRAS_API_KEY=<your_cerebras_api_key>
```

</details>

<details>
<summary><strong>IO Intelligence</strong></summary>

```bash
# .env
IO_INTELLIGENCE_API_KEY=<your_io_intelligence_api_key>
```

```yaml
# forge.yaml
model: meta-llama/Llama-3.3-70B-Instruct
```

</details>

<details>
<summary><strong>OpenAI</strong></summary>

```bash
# .env
OPENAI_API_KEY=<your_openai_api_key>
```

```yaml
# forge.yaml
model: o3-mini-high
```

</details>

<details>
<summary><strong>Anthropic</strong></summary>

```bash
# .env
ANTHROPIC_API_KEY=<your_anthropic_api_key>
```

```yaml
# forge.yaml
model: claude-3.7-sonnet
```

</details>

<details>
<summary><strong>Google Vertex AI</strong></summary>

**Setup Instructions:**

1. **Install Google Cloud CLI** and authenticate:

   ```bash
   gcloud auth login
   gcloud config set project YOUR_PROJECT_ID
   ```

2. **Get your authentication token**:

   ```bash
   gcloud auth print-access-token
   ```

3. **Use the token when logging in via Forge**:

   ```bash
   forge provider login
   # Select Google Vertex AI and enter your credentials
   ```

**Legacy `.env` setup:**

```bash
# .env
PROJECT_ID=<your_project_id>
LOCATION=<your_location>
VERTEX_AI_AUTH_TOKEN=<your_auth_token>
```

```yaml
# forge.yaml
model: google/gemini-2.5-pro
```

**Available Models:**
- Claude models: `claude-sonnet-4@20250514`
- Gemini models: `gemini-2.5-pro`, `gemini-2.0-flash`

Use the `/model` command in Forge CLI to see all available models.

</details>

<details>
<summary><strong>OpenAI-Compatible Providers</strong></summary>

```bash
# .env
OPENAI_API_KEY=<your_provider_api_key>
OPENAI_URL=<your_provider_url>
```

```yaml
# forge.yaml
model: <provider-specific-model>
```

</details>

<details>
<summary><strong>Groq</strong></summary>

```bash
# .env
OPENAI_API_KEY=<your_groq_api_key>
OPENAI_URL=https://api.groq.com/openai/v1
```

```yaml
# forge.yaml
model: deepseek-r1-distill-llama-70b
```

</details>

<details>
<summary><strong>Amazon Bedrock</strong></summary>

To use Amazon Bedrock models with Forge, you'll need to first set up the [Bedrock Access Gateway](https://github.com/aws-samples/bedrock-access-gateway):

1. **Set up Bedrock Access Gateway**:

   - Follow the deployment steps in the [Bedrock Access Gateway repo](https://github.com/aws-samples/bedrock-access-gateway)
   - Create your own API key in Secrets Manager
   - Deploy the CloudFormation stack
   - Note your API Base URL from the CloudFormation outputs

2. **Configure in Forge**:

   ```bash
   forge provider login
   # Select OpenAI-compatible provider and enter your Bedrock Gateway details
   ```

**Legacy `.env` setup:**

```bash
# .env
OPENAI_API_KEY=<your_bedrock_gateway_api_key>
OPENAI_URL=<your_bedrock_gateway_base_url>
```

```yaml
# forge.yaml
model: anthropic.claude-3-opus
```

</details>

</details>

---

### forge.yaml Configuration Options

### Environment Variables

Forge supports several environment variables for advanced configuration and fine-tuning. These can be set in your `.env` file or system environment.

<details>
<summary><strong>Retry Configuration</strong></summary>

Control how Forge handles retry logic for failed requests:

```bash
# .env
FORGE_RETRY_INITIAL_BACKOFF_MS=1000    # Initial backoff time in milliseconds (default: 1000)
FORGE_RETRY_BACKOFF_FACTOR=2           # Multiplier for backoff time (default: 2)
FORGE_RETRY_MAX_ATTEMPTS=3             # Maximum retry attempts (default: 3)
FORGE_SUPPRESS_RETRY_ERRORS=false      # Suppress retry error messages (default: false)
FORGE_RETRY_STATUS_CODES=429,500,502   # HTTP status codes to retry (default: 429,500,502,503,504)
```

</details>

<details>
<summary><strong>HTTP Configuration</strong></summary>

Fine-tune HTTP client behavior for API requests:

```bash
# .env
FORGE_HTTP_CONNECT_TIMEOUT=30              # Connection timeout in seconds (default: 30)
FORGE_HTTP_READ_TIMEOUT=900                # Read timeout in seconds (default: 900)
FORGE_HTTP_POOL_IDLE_TIMEOUT=90            # Pool idle timeout in seconds (default: 90)
FORGE_HTTP_POOL_MAX_IDLE_PER_HOST=5        # Max idle connections per host (default: 5)
FORGE_HTTP_MAX_REDIRECTS=10                # Maximum redirects to follow (default: 10)
FORGE_HTTP_USE_HICKORY=false               # Use Hickory DNS resolver (default: false)
FORGE_HTTP_TLS_BACKEND=default             # TLS backend: "default" or "rustls" (default: "default")
FORGE_HTTP_MIN_TLS_VERSION=1.2             # Minimum TLS version: "1.0", "1.1", "1.2", "1.3"
FORGE_HTTP_MAX_TLS_VERSION=1.3             # Maximum TLS version: "1.0", "1.1", "1.2", "1.3"
FORGE_HTTP_ADAPTIVE_WINDOW=true            # Enable HTTP/2 adaptive window (default: true)
FORGE_HTTP_KEEP_ALIVE_INTERVAL=60          # Keep-alive interval in seconds (default: 60, use "none"/"disabled" to disable)
FORGE_HTTP_KEEP_ALIVE_TIMEOUT=10           # Keep-alive timeout in seconds (default: 10)
FORGE_HTTP_KEEP_ALIVE_WHILE_IDLE=true      # Keep-alive while idle (default: true)
FORGE_HTTP_ACCEPT_INVALID_CERTS=false      # Accept invalid certificates (default: false) - USE WITH CAUTION
FORGE_HTTP_ROOT_CERT_PATHS=/path/to/cert1.pem,/path/to/cert2.crt  # Paths to root certificate files (PEM, CRT, CER format), multiple paths separated by commas
```

> **⚠️ Security Warning**: Setting `FORGE_HTTP_ACCEPT_INVALID_CERTS=true` disables SSL/TLS certificate verification, which can expose you to man-in-the-middle attacks. Only use this in development environments or when you fully trust the network and endpoints.

</details>

<details>
<summary><strong>API Configuration</strong></summary>

Override default API endpoints:

```bash
# .env
FORGE_API_URL=https://api.forgecode.dev  # Custom Forge API URL (default: https://api.forgecode.dev)
```

</details>

<details>
<summary><strong>Tool Configuration</strong></summary>

Configuring the tool calls settings:

```bash
# .env
FORGE_TOOL_TIMEOUT=300         # Maximum execution time in seconds for a tool before it is terminated to prevent hanging the session. (default: 300)
FORGE_MAX_IMAGE_SIZE=262144    # Maximum image file size in bytes for read_image operations (default: 262144 - 256 KB)
FORGE_DUMP_AUTO_OPEN=false     # Automatically open dump files in browser (default: false)
FORGE_DEBUG_REQUESTS=/path/to/debug/requests.json  # Write debug HTTP request files to specified path (supports absolute and relative paths)
```

</details>

<details>
<summary><strong>ZSH Plugin Configuration</strong></summary>

Configure the ZSH plugin behavior:

```bash
# .env
FORGE_BIN=forge                    # Command to use for forge operations (default: "forge")
```

The `FORGE_BIN` environment variable allows you to customize the command used by the ZSH plugin when transforming `#` prefixed commands. If not set, it defaults to `"forge"`.

</details>

<details>
<summary><strong>System Configuration</strong></summary>

System-level environment variables (usually set automatically):

```bash
# .env
FORGE_MAX_SEARCH_RESULT_BYTES=101024   # Maximum bytes for search results (default: 101024 - 10 KB)
FORGE_HISTORY_FILE=/path/to/history    # Custom path for Forge history file (default: uses system default location)
FORGE_BANNER="Your custom banner text" # Custom banner text to display on startup (default: Forge ASCII art)
FORGE_SHOW_TASK_STATS=true             # Show task stats such as file changes, token usage etc. after completion (default: true)
FORGE_MAX_CONVERSATIONS=100            # Maximum number of conversations to show in list (default: 100)
FORGE_STREAMING_OUTPUT=true            # Enable streaming markdown output rendering (default: true). When enabled, markdown is rendered incrementally as it arrives. When disabled, uses batch rendering.
SHELL=/bin/zsh                         # Shell to use for command execution (Unix/Linux/macOS)
COMSPEC=cmd.exe                        # Command processor to use (Windows)
```

</details>

The `forge.yaml` file supports several advanced configuration options that let you customize Forge's behavior.

<details>
<summary><strong>Custom Rules</strong></summary>

Add your own guidelines that all agents should follow when generating responses.

```yaml
# forge.yaml
custom_rules: |
  1. Always add comprehensive error handling to any code you write.
  2. Include unit tests for all new functions.
  3. Follow our team's naming convention: camelCase for variables, PascalCase for classes.
```

</details>

<details>
<summary><strong>Commands</strong></summary>

Define custom commands as shortcuts for repetitive prompts:

```yaml
# forge.yaml
commands:
  - name: "refactor"
    description: "Refactor selected code"
    prompt: "Please refactor this code to improve readability and performance"
```

</details>

<details>
<summary><strong>Model</strong></summary>

Specify the default AI model to use for all agents in the workflow.

```yaml
# forge.yaml
model: "claude-3.7-sonnet"
```

</details>

<details>
<summary><strong>Max Walker Depth</strong></summary>

Control how deeply Forge traverses your project directory structure when gathering context.

```yaml
# forge.yaml
max_walker_depth: 3 # Limit directory traversal to 3 levels deep
```

</details>

<details>
<summary><strong>Temperature</strong></summary>

Adjust the creativity and randomness in AI responses. Lower values (0.0-0.3) produce more focused, deterministic outputs, while higher values (0.7-2.0) generate more diverse and creative results.

```yaml
# forge.yaml
temperature: 0.7 # Balanced creativity and focus
```

</details>
<details>
<summary><strong>Tool Max Failure Limit</strong></summary>

Control how many times a tool can fail before Forge forces completion to prevent infinite retry loops. This helps avoid situations where an agent gets stuck repeatedly trying the same failing operation.

```yaml
# forge.yaml
max_tool_failure_per_turn: 3 # Allow up to 3 failures per tool before forcing completion
```

Set to a higher value if you want more retry attempts, or lower if you want faster failure detection.

</details>

<details>
<summary><strong>Max Requests Per Turn</strong></summary>

Limit the maximum number of requests an agent can make in a single conversation turn. This prevents runaway conversations and helps control API usage and costs.

```yaml
# forge.yaml
max_requests_per_turn: 50 # Allow up to 50 requests per turn
```

When this limit is reached, Forge will:

- Ask you if you wish to continue
- If you respond with 'Yes', it will continue the conversation
- If you respond with 'No', it will end the conversation

</details>

---

<details>
<summary><strong>Model Context Protocol (MCP)</strong></summary>

The MCP feature allows AI agents to communicate with external tools and services. This implementation follows Anthropic's [Model Context Protocol](https://docs.anthropic.com/en/docs/claude-code/tutorials#set-up-model-context-protocol-mcp) design.

### MCP Configuration

Configure MCP servers using the CLI:

```bash
# List all MCP servers
forge mcp list

# Add a new server
forge mcp add

# Add a server using JSON format
forge mcp add-json

# Get server details
forge mcp get

# Remove a server
forge mcp remove
```

Or manually create a `.mcp.json` file with the following structure:

```json
{
  "mcpServers": {
    "server_name": {
      "command": "command_to_execute",
      "args": ["arg1", "arg2"],
      "env": { "ENV_VAR": "value" }
    },
    "another_server": {
      "url": "http://localhost:3000/events"
    }
  }
}
```

MCP configurations are read from two locations (in order of precedence):

1. Local configuration (project-specific)
2. User configuration (user-specific)

### Example Use Cases

MCP can be used for various integrations:

- Web browser automation
- External API interactions
- Tool integration
- Custom service connections

### Usage in Multi-Agent Workflows

MCP tools can be used as part of multi-agent workflows, allowing specialized agents to interact with external systems as part of a collaborative problem-solving approach.

</details>

---

## Documentation

For comprehensive documentation on all features and capabilities, please visit the [documentation site](https://github.com/antinomyhq/forge/tree/main/docs).

---

## Community

Join our vibrant Discord community to connect with other Forge users and contributors, get help with your projects, share ideas, and provide feedback!

[![Discord](https://img.shields.io/discord/1044859667798568962?style=for-the-badge&cacheSeconds=120&logo=discord)](https://discord.gg/kRZBPpkgwq)

---

## Support Us

Your support drives Forge's continued evolution! By starring our GitHub repository, you:

- Help others discover this powerful tool 🔍
- Motivate our development team 💪
- Enable us to prioritize new features 🛠️
- Strengthen our open-source community 🌱
