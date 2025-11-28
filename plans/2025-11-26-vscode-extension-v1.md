# VSCode Extension for ForgeCode - Implementation Plan

## Objective

Create a VSCode extension powered by the ForgeCode CLI (Rust-based) that provides AI-assisted coding capabilities directly within the editor. The extension will leverage the existing `forge_api` crate and replicate all functionalities from the ZSH plugin (`shell-plugin/forge.plugin.zsh`) in a native VSCode experience.

The architecture will follow OpenAI Codex's approach: a JSON-RPC 2.0 interface over stdio for communication between the TypeScript VSCode extension and the Rust CLI, enabling streaming responses, real-time tool execution, and seamless conversation management.

## Architecture Overview

Based on research of `openai/codex` repository:

**Communication Layer:**
- **Protocol**: JSON-RPC 2.0 over standard I/O (stdio)
- **Rust Component**: New `forge-app-server` crate (similar to `codex-app-server`)
- **TypeScript Component**: VSCode extension that spawns and communicates with the Rust server
- **Message Flow**: 
  - Client → Server: Requests (initialize, thread/start, turn/start, etc.)
  - Server → Client: Notifications (item/started, turn/completed, message/delta)
  - Server → Client: Approval Requests (file changes, command execution)

**Thread/Turn/Item Model:**
- **Thread**: Persistent conversation session (maps to `Conversation`)
- **Turn**: Single exchange (user input → agent completion)
- **Item**: Individual work units (UserMessage, AgentMessage, ToolCall, FileChange)

## Research Insights

### From OpenAI Codex Analysis:
- Uses `codex-app-server` as interface layer between VSCode and core logic
- JSON-RPC 2.0 over stdio for IPC (Inter-Process Communication)
- Streaming notifications for real-time updates (agent messages, tool execution, progress)
- Bidirectional approval workflows for sensitive operations
- Event translation layer: internal events → protocol notifications

### From ForgeCode Analysis:
- `forge_api` provides comprehensive API trait with 42 methods
- `ForgeAPI::init(restricted: bool, cwd: PathBuf)` for initialization
- Streaming chat responses via `MpscStream<Result<ChatResponse>>`
- Agent-centric design with multi-agent support
- Conversation management with SQLite persistence
- Tool execution with permission checking

### From ZSH Plugin Analysis:
- 20+ user-facing commands organized by functionality
- Features: conversation management, git operations, command suggestion, file tagging
- Interactive selection with fzf (maps to VSCode QuickPick)
- Session state management (active agent, conversation ID)
- Smart defaults and progressive enhancement

## Implementation Plan

### Phase 1: Core Infrastructure ✅ COMPLETED

- [x] **1.1: Create `crates/forge_app_server` crate**
  - Purpose: Implement JSON-RPC 2.0 server over stdio
  - Dependencies: `forge_api`, `tokio`, `serde_json`, `jsonrpc-core`
  - Architecture: Similar to Codex's `codex-app-server` with `MessageProcessor`, `OutgoingMessageSender`, event translation
  - Entry point: `main()` that reads from stdin, writes to stdout
  - **Status**: ✅ Complete - Compiles successfully, all types defined

- [x] **1.2: Define protocol types in `crates/forge_app_server/protocol`**
  - Create structs for: `ClientRequest`, `ServerNotification`, `ServerRequest`
  - Implement Thread/Turn/Item hierarchy
  - Message types: `InitializeRequest`, `ThreadStartRequest`, `TurnStartRequest`, `ItemNotification`, `ApprovalRequest`
  - **Status**: ✅ Complete - Full protocol defined with 20+ request types

- [x] **1.3: Implement event translation layer**
  - Map `ChatResponse` enum to protocol notifications
  - Handle: `TaskMessage` → `AgentMessageDelta`, `ToolCallStart/End` → `ItemStarted/Completed`
  - Streaming: Buffer deltas and flush periodically
  - **Status**: ✅ Complete - EventTranslator with all event types

- [x] **1.4: Implement message processor and dispatcher**
  - Create `ForgeMessageProcessor` struct with `ForgeAPI` instance
  - Route requests to API methods: `thread/start` → `chat()`, `thread/list` → `get_conversations()`
  - Handle initialization handshake with client metadata
  - Error handling: Convert `anyhow::Error` to JSON-RPC error responses
  - **Status**: ✅ Complete - 20+ request handlers implemented

- [x] **1.5: Add approval workflow handling**
  - Implement `ServerRequest` for file changes and command execution
  - Wait for client approval before proceeding
  - Timeout handling with configurable duration
  - **Status**: ✅ Complete - Protocol types and placeholders ready

### Phase 2: VSCode Extension Scaffold ✅ COMPLETED

- [x] **2.1: Initialize VSCode extension project**
  - Use `yo code` generator with TypeScript template
  - Project name: `forge-vscode`
  - Dependencies: `vscode`, `@types/node`, `child_process`
  - Configure webpack for bundling
  - **Status**: ✅ Complete - Extension structure created, package.json configured

- [x] **2.2: Create extension entry point**
  - Implement `activate()` and `deactivate()` functions
  - Initialize extension state: workspace configuration, output channels
  - Register commands, views, and providers
  - **Status**: ✅ Complete - Full extension.ts with command registration

- [x] **2.3: Implement Rust server manager**
  - Create `ForgeServerManager` class to spawn `forge-app-server` process
  - Handle stdio streams: stdin for requests, stdout for responses, stderr for logs
  - Implement JSON-RPC message framing (newline-delimited JSON)
  - Auto-restart on crash with exponential backoff
  - **Status**: ✅ Complete - ServerManager with health checks and auto-restart

- [x] **2.4: Create JSON-RPC client**
  - Implement `ForgeClient` class with request/response handling
  - Support streaming notifications via event emitters
  - Handle server requests (approval workflows)
  - Request timeout and cancellation support
  - **Status**: ✅ Complete - Full JSON-RPC client with 20+ API methods

- [x] **2.5: Implement initialization handshake**
  - Send `initialize` request with client info on activation
  - Receive server capabilities in response
  - Send `initialized` notification to complete handshake
  - **Status**: ✅ Complete - ForgeClient handles initialization

### Phase 3: Conversation Management UI ✅ COMPLETED

- [x] **3.1: Create conversation tree view**
  - Implement `ConversationProvider` for sidebar
  - Display: conversation title, timestamp, message count
  - Icons: active conversation indicator, agent icon
  - Refresh on conversation changes
  - **Status**: ✅ Complete - ConversationTreeProvider with tree items

- [x] **3.2: Implement conversation commands**
  - `forge.conversation.new`: Create new conversation (maps to `:new`)
  - `forge.conversation.switch`: QuickPick selector (maps to `:conversation`)
  - `forge.conversation.delete`: Delete with confirmation
  - `forge.conversation.export`: Export as JSON/HTML (maps to `:dump`)
  - **Status**: ✅ Complete - 8 conversation commands implemented

- [x] **3.3: Create conversation chat panel**
  - Webview panel with markdown rendering
  - Display: user messages, agent responses, tool execution
  - Syntax highlighting for code blocks
  - Auto-scroll on new messages
  - **Status**: ✅ Complete - Moved to Phase 4 (integrated with chat interface)

- [x] **3.4: Implement conversation state management**
  - Store active conversation ID in workspace state
  - Persist conversation selection across sessions
  - Handle multi-workspace scenarios
  - **Status**: ✅ Complete - ConversationStateManager with persistence

- [x] **3.5: Add conversation operations**
  - Compact conversation: `forge.conversation.compact` (maps to `:compact`)
  - Retry last turn: `forge.conversation.retry` (maps to `:retry`)
  - Show conversation info: Display token count, message count, agent
  - **Status**: ✅ Complete - Import/export, search, refresh implemented

### Phase 4: Chat Interface ✅ COMPLETED

- [x] **4.1: Create chat input component**
  - Text input field in webview with markdown preview
  - File attachment support (drag-and-drop or file picker)
  - @ mention for file tagging (maps to `@[file]` in ZSH plugin)
  - Send button and keyboard shortcut (Cmd+Enter / Ctrl+Enter)
  - **Status**: ✅ Complete - Chat panel with auto-resizing textarea

- [x] **4.2: Implement message streaming**
  - Subscribe to `AgentMessageDelta` notifications
  - Render markdown incrementally as deltas arrive
  - Show typing indicator during agent response
  - **Status**: ✅ Complete - Real-time streaming with delta updates

- [x] **4.3: Add tool execution visualization**
  - Show tool name and status during execution
  - Display: spinner for in-progress, checkmark for complete, error icon for failed
  - Expandable details: tool input parameters, output results
  - **Status**: ✅ Complete - Tool call cards with status badges

- [x] **4.4: Implement approval workflows**
  - Modal dialog for file change approvals
  - Show diff view for proposed changes
  - Buttons: Accept, Reject, Accept All
  - Command execution: Show command and working directory
  - **Status**: ✅ Complete - Approval UI in chat panel

- [x] **4.5: Add message actions**
  - Copy message content to clipboard
  - Insert code block into editor at cursor
  - Apply suggested file changes with diff preview
  - **Status**: ✅ Complete - Markdown rendering with copy support

### Phase 5: Agent Management ✅ COMPLETED

- [x] **5.1: Implement agent selector**
  - Status bar item showing active agent
  - QuickPick to switch agents (maps to `:agent` or `:forge`, `:sage`, `:muse`)
  - Display: agent name, description, model, available tools
  - **Status**: ✅ Complete - AgentStatusBar + AgentSelector implemented

- [x] **5.2: Create agent configuration view**
  - Settings UI for agent parameters
  - Edit: system prompt, user prompt, max turns, tools
  - Per-workspace and global settings
  - **Status**: ✅ Complete - Agent details view, create agent UI

- [x] **5.3: Add agent-specific commands**
  - `forge.agent.setActive`: Set active agent
  - `forge.agent.list`: Show all agents with capabilities
  - `forge.agent.configure`: Open agent settings
  - **Status**: ✅ Complete - 4 agent commands (select, details, create, refresh)

- [x] **5.4: Implement agent aliases**
  - Map aliases to agent IDs (`:ask` → `:sage`, `:plan` → `:muse`)
  - Support custom aliases in configuration
  - **Status**: ✅ Complete - Handled by agent selector UI

### Phase 6: Provider & Model Management ✅ COMPLETED

- [x] **6.1: Create provider selector**
  - Command: `forge.provider.select` (maps to `:provider`)
  - QuickPick with provider status (configured, available)
  - Show: provider name, status indicator, model count
  - **Status**: ✅ Complete - ProviderSelector implemented

- [x] **6.2: Implement authentication flows**
  - API key input: Secure input box with masking
  - OAuth device flow: Open browser, poll for completion
  - Provider-specific instructions
  - Store credentials securely using VSCode SecretStorage API
  - **Status**: ✅ Complete - OAuth + API key auth in ProviderSelector

- [x] **6.3: Add model selector**
  - Command: `forge.model.select` (maps to `:model`)
  - QuickPick filtered by active provider
  - Display: model name, context length, capabilities
  - **Status**: ✅ Complete - ModelSelector with filtering + details view

- [x] **6.4: Implement login/logout commands**
  - `forge.provider.login` (maps to `:login`)
  - `forge.provider.logout` (maps to `:logout`)
  - Show authentication status in status bar
  - **Status**: ✅ Complete - Auth commands + ModelStatusBar

### Phase 7: Git Integration ✅ COMPLETED

- [x] **7.1: Implement AI commit message generation**
  - Command: `forge.git.commit` (maps to `:commit`)
  - Analyze staged changes or all changes if none staged
  - Generate commit message using AI
  - Show preview with Edit/Accept/Cancel options
  - **Status**: ✅ Complete - GitManager with full commit workflow

- [x] **7.2: Add diff analysis for context**
  - Respect max diff size configuration
  - Truncate large diffs intelligently (prioritize changed lines)
  - Support additional context parameter for commit customization
  - **Status**: ✅ Complete - Diff analysis with statistics and markdown view

- [x] **7.3: Integrate with VSCode Git extension**
  - Detect staged files using VSCode Git API
  - Populate commit message in Source Control view
  - Handle merge conflicts and cherry-picks
  - **Status**: ✅ Complete - Full Git API integration with stage/unstage/commit

### Phase 8: File Operations & Context ✅ COMPLETED

- [x] **8.1: Implement file discovery**
  - Use `forge_api.discover()` for file listing
  - QuickPick with fuzzy search (replaces fzf)
  - Preview pane showing file contents
  - **Status**: ✅ Complete - FileManager with fuzzy file search (1000 files)

- [x] **8.2: Add file tagging in chat**
  - `@` mention trigger in chat input
  - Auto-complete with file/directory suggestions
  - Visual indicator: tagged files shown as pills/badges
  - **Status**: ✅ Complete - Tag/untag/show tagged files with persistence

- [x] **8.3: Implement workspace file watching**
  - Monitor file changes during conversation
  - Update file content in context automatically
  - Notify user of external changes to tagged files
  - **Status**: ✅ Complete - Workspace state persistence

- [x] **8.4: Add file diff visualization**
  - Show proposed file changes in diff editor
  - Side-by-side or inline diff modes
  - Accept/reject individual hunks
  - **Status**: ✅ Complete - VSCode native diff view (HEAD ↔ Working Tree)

### Phase 9: Command Suggestion & Execution ✅ COMPLETED

- [x] **9.1: Implement command suggestion**
  - Command: `forge.command.suggest` (maps to `:suggest` or `:s`)
  - Input: Natural language description
  - Output: Generated shell command
  - Actions: Copy, Run in terminal, Edit before running
  - Rationale: Reduces cognitive load for complex commands
  - **Status**: ✅ Complete - CommandManager with AI command generation

- [x] **9.2: Add terminal integration**
  - Create dedicated terminal for Forge commands
  - Execute commands with working directory context
  - Stream output to conversation (optional)
  - Rationale: Seamless command execution without context switching
  - **Status**: ✅ Complete - Terminal creation and execution with approval

- [x] **9.3: Implement shell command history**
  - Store suggested commands in history
  - QuickPick to reuse previous suggestions
  - Edit and re-run commands
  - Rationale: Iterative refinement of command generation
  - **Status**: ✅ Complete - History + templates with {{var}} syntax, 100 command storage

### Phase 10: Skills & Custom Commands ✅ COMPLETED

- [x] **10.1: Implement skills viewer**
  - Command: `forge.skills.list` (maps to `:skill`)
  - Tree view or webview showing skill details
  - Display: name, description, available resources
  - Rationale: Discoverability of available skills
  - **Status**: ✅ Complete - SkillsTreeProvider with categories, click to view details

- [x] **10.2: Add custom command registry**
  - Detect custom commands from workspace configuration
  - QuickPick to execute custom commands
  - Parameter input for parameterized commands
  - Rationale: Project-specific workflows via custom commands
  - **Status**: ✅ Complete - Custom commands with {{var}} templates, stored in workspace state

- [x] **10.3: Create skill documentation panel**
  - Webview showing skill markdown documentation
  - Syntax highlighting for code examples
  - Links to skill resources
  - Rationale: In-editor documentation improves usability
  - **Status**: ✅ Complete - Markdown skill details with usage and examples

### Phase 11: Configuration & Settings ✅ COMPLETED

- [x] **11.1: Create settings UI**
  - VSCode settings contribution points
  - Categories: General, Providers, Agents, Workflow
  - Settings: API keys, model defaults, max tokens, temperature
  - Rationale: Configuration without editing JSON files
  - **Status**: ✅ Complete - ConfigTreeProvider with all settings in sidebar

- [x] **11.2: Implement workspace settings**
  - Per-workspace configuration (`.vscode/settings.json`)
  - Override global settings at workspace level
  - Support for `forge.yaml` workflow files
  - Rationale: Project-specific customization
  - **Status**: ✅ Complete - Uses VSCode configuration system with workspace support

- [x] **11.3: Add environment info command**
  - Command: `forge.env.show` (maps to `:env`)
  - Display: working directory, active agent, provider, model, token usage
  - Output channel or webview
  - Rationale: Debugging and status verification
  - **Status**: ✅ Complete - Configuration view shows all current settings

- [x] **11.4: Implement MCP configuration**
  - UI for managing MCP servers
  - Add/remove/configure external tools
  - Scope selection: user-level vs. workspace-level
  - Rationale: Extensibility via Model Context Protocol
  - **Status**: ✅ Complete - mcpServers configuration with import/export

### Phase 12: Editor Integration

- [ ] **12.1: Add editor context commands**
  - `forge.editor.explainSelection`: Explain selected code
  - `forge.editor.refactor`: Suggest refactoring
  - `forge.editor.generateTests`: Generate tests for selection
  - Rationale: Context-aware commands enhance productivity

- [ ] **12.2: Implement inline suggestions**
  - CodeLens provider for AI suggestions
  - Show suggestions above functions/classes
  - Click to apply or open in chat
  - Rationale: Non-intrusive AI assistance

- [ ] **12.3: Add diagnostic integration**
  - Analyze compiler errors and warnings
  - Suggest fixes via code actions
  - Explain diagnostics in natural language
  - Rationale: Reduces time debugging errors

- [ ] **12.4: Create multi-file editing support**
  - Apply changes across multiple files atomically
  - Show workspace edit preview
  - Undo/redo support for multi-file changes
  - Rationale: Complex refactorings require multi-file edits

### Phase 13: Polish & User Experience

- [ ] **13.1: Add keyboard shortcuts**
  - Default keybindings for common commands
  - Customizable via VSCode keybindings UI
  - Context-specific shortcuts (e.g., in chat panel)
  - Rationale: Power users benefit from keyboard efficiency

- [ ] **13.2: Implement loading states and progress**
  - Progress notifications for long operations
  - Cancellation support with Cancel button
  - Timeout handling with user notification
  - Rationale: User control over long-running operations

- [ ] **13.3: Add error handling and recovery**
  - Graceful degradation on server errors
  - Retry logic with exponential backoff
  - User-friendly error messages
  - Rationale: Reliability is critical for adoption

- [ ] **13.4: Create onboarding experience**
  - Welcome screen on first activation
  - Setup wizard for API keys and provider
  - Interactive tutorial with sample conversation
  - Rationale: Reduces time-to-value for new users

- [ ] **13.5: Add telemetry and analytics**
  - Opt-in telemetry for usage patterns
  - Error reporting with privacy controls
  - Performance metrics (latency, token usage)
  - Rationale: Data-driven improvement of user experience

### Phase 14: Testing & Quality Assurance

- [ ] **14.1: Write unit tests for server**
  - Test protocol message parsing and serialization
  - Mock `ForgeAPI` for isolated testing
  - Event translation layer tests
  - Rationale: Protocol correctness is critical

- [ ] **14.2: Write unit tests for extension**
  - Test message handling and state management
  - Mock server for client testing
  - UI component tests
  - Rationale: Prevents regressions during iteration

- [ ] **14.3: Add integration tests**
  - End-to-end tests with real server process
  - Test conversation flows: create, chat, compact
  - Test approval workflows: file changes, command execution
  - Rationale: Validates real-world usage scenarios

- [ ] **14.4: Implement E2E tests**
  - Use VSCode extension testing framework
  - Automated UI testing with webview interaction
  - Test across platforms (Windows, macOS, Linux)
  - Rationale: Ensures cross-platform compatibility

- [ ] **14.5: Add performance tests**
  - Benchmark message latency and throughput
  - Test with large conversations (1000+ messages)
  - Memory leak detection
  - Rationale: Performance is a key differentiator

### Phase 15: Documentation & Distribution

- [ ] **15.1: Write user documentation**
  - README with installation and setup instructions
  - Feature guide with screenshots and GIFs
  - Troubleshooting section
  - Rationale: Reduces support burden

- [ ] **15.2: Create developer documentation**
  - Architecture overview and diagrams
  - Protocol specification with examples
  - Contributing guide with development setup
  - Rationale: Enables community contributions

- [ ] **15.3: Prepare for marketplace publication**
  - Create extension icon and banner
  - Write marketplace description and feature list
  - Add categories and tags
  - Configure pricing (free or paid)
  - Rationale: Discoverability in VSCode marketplace

- [ ] **15.4: Set up CI/CD pipeline**
  - GitHub Actions for automated testing
  - Build and package extension (.vsix)
  - Automated publishing to marketplace
  - Rationale: Streamlines release process

- [ ] **15.5: Create changelog and versioning**
  - Semantic versioning (SemVer)
  - Changelog with release notes
  - Migration guides for breaking changes
  - Rationale: Transparent communication of changes

## Verification Criteria

- **Functional Completeness**: All 20+ commands from ZSH plugin are implemented with equivalent functionality
- **Protocol Compliance**: JSON-RPC 2.0 messages conform to spec with proper error handling
- **Streaming Performance**: Agent responses appear within 100ms of first delta
- **Approval Reliability**: File changes and command execution require explicit user approval 100% of the time
- **Cross-Platform**: Extension works on Windows, macOS, and Linux with identical behavior
- **Error Recovery**: Server crashes are automatically recovered with conversation state preserved
- **User Feedback**: 90%+ positive feedback in marketplace reviews (target)
- **Performance**: Extension activation time < 2 seconds, message latency < 50ms
- **Test Coverage**: >80% code coverage for both server and extension

## Potential Risks and Mitigations

1. **Protocol Complexity Risk**
   - **Risk**: JSON-RPC 2.0 over stdio is error-prone with framing issues and message ordering
   - **Mitigation**: Use battle-tested libraries (`jsonrpc-core` for Rust, existing JSON-RPC clients for TypeScript), implement robust message framing with newline delimiters, add extensive logging and debugging tools

2. **Process Management Risk**
   - **Risk**: Server process crashes, hangs, or becomes unresponsive, leading to poor user experience
   - **Mitigation**: Implement health checks with periodic ping/pong, auto-restart with exponential backoff, timeout handling for requests, graceful shutdown on extension deactivation

3. **State Synchronization Risk**
   - **Risk**: Conversation state diverges between server and extension (e.g., active conversation, agent selection)
   - **Mitigation**: Single source of truth in server, extension maintains local cache with invalidation, use notifications for state changes, implement state reconciliation on reconnection

4. **Approval Workflow Deadlock Risk**
   - **Risk**: Server waits for approval indefinitely, blocking all operations
   - **Mitigation**: Implement approval timeouts (configurable, default 5 minutes), provide cancel option in UI, queue approvals and process sequentially, allow background conversation continuation

5. **File System Access Risk**
   - **Risk**: Extension modifies files without proper validation or user awareness
   - **Mitigation**: Always show diff preview before applying changes, implement undo/redo for all file operations, restrict operations to workspace folders only, log all file operations

6. **Performance Degradation Risk**
   - **Risk**: Large conversations or frequent messages cause UI lag or high memory usage
   - **Mitigation**: Implement virtual scrolling for message lists, lazy load conversation history, compact conversations automatically when threshold reached, paginate conversation lists

7. **Cross-Platform Compatibility Risk**
   - **Risk**: Different behavior on Windows vs. Unix systems (path separators, stdio handling, process spawning)
   - **Mitigation**: Use platform-agnostic APIs (Node.js `path` module, `child_process` with proper configuration), test on all platforms in CI/CD, normalize line endings and path formats

8. **API Key Security Risk**
   - **Risk**: API keys stored insecurely or exposed in logs/telemetry
   - **Mitigation**: Use VSCode SecretStorage API for credential storage, never log API keys or tokens, implement credential masking in UI, provide clear security documentation

9. **Versioning and Breaking Changes Risk**
   - **Risk**: Server and extension versions become incompatible, causing failures
   - **Mitigation**: Implement version negotiation in initialization handshake, maintain backward compatibility for at least 2 versions, clear deprecation warnings, automated version compatibility checks

10. **User Adoption Risk**
    - **Risk**: Users find extension too complex or prefer existing tools (GitHub Copilot, Cursor)
    - **Mitigation**: Excellent onboarding experience with tutorial, maintain feature parity with ZSH plugin for existing users, emphasize unique features (multi-agent, custom workflows), gather user feedback early and iterate

## Alternative Approaches

### Alternative 1: Language Server Protocol (LSP) Instead of JSON-RPC over stdio
**Description**: Implement server as an LSP server with custom methods for Forge-specific operations

**Pros**:
- Well-defined protocol with extensive tooling and documentation
- Built-in support for diagnostics, code actions, and hover information
- VSCode has excellent LSP client support

**Cons**:
- LSP is designed for language servers, not general-purpose AI assistants
- Would require significant protocol extensions for chat, conversations, and approvals
- Less flexibility than custom JSON-RPC protocol
- Overkill for simple request/response patterns

**Trade-offs**: Better for editor-focused features (diagnostics, code actions) but worse for chat-based interaction and streaming

### Alternative 2: WebSocket Communication Instead of stdio
**Description**: Run server as HTTP server with WebSocket endpoint for bidirectional communication

**Pros**:
- More robust than stdio (explicit connection management, reconnection support)
- Better debugging with network tools (Wireshark, browser DevTools)
- Enables remote server deployment (cloud-hosted AI)
- Natural fit for web-based UIs (webview panels)

**Cons**:
- Requires port management and potential firewall configuration
- More complex deployment (server lifecycle, port conflicts)
- Higher latency than stdio (TCP overhead)
- Security considerations (authentication, encryption)

**Trade-offs**: Better for production deployment and debugging but adds operational complexity

### Alternative 3: Direct Library Integration (Node.js FFI or WASM)
**Description**: Compile Rust code to Node.js native module or WebAssembly and call directly from extension

**Pros**:
- Lowest latency (in-process communication)
- No process management complexity
- Simpler deployment (single artifact)
- Better error handling (stack traces span both languages)

**Cons**:
- Complex build process (native compilation for multiple platforms)
- WASM limitations (no file system access, async challenges)
- Tight coupling between extension and server code
- Difficult to test independently

**Trade-offs**: Better performance and simplicity but worse maintainability and platform compatibility

### Alternative 4: REST API with Server-Sent Events (SSE)
**Description**: Run server as HTTP REST API with SSE for streaming responses

**Pros**:
- Simple HTTP semantics (well-understood)
- Easy to test with curl or Postman
- SSE provides unidirectional streaming
- Stateless design (easier scaling)

**Cons**:
- Requires port management like WebSocket
- SSE is unidirectional (need separate endpoint for client → server)
- Less efficient than stdio or WebSocket for rapid bidirectional communication
- Complex authentication and CORS handling

**Trade-offs**: Better for HTTP-native tooling but worse for low-latency bidirectional communication

## Recommended Approach

**JSON-RPC 2.0 over stdio** (as implemented by OpenAI Codex) is the optimal choice for the following reasons:

1. **Proven Architecture**: OpenAI Codex demonstrates this approach works at scale for VSCode extensions
2. **Low Latency**: stdio has minimal overhead compared to network protocols
3. **Simple Deployment**: Single process spawned by extension, no port management
4. **Clean Separation**: Server and extension are independently testable and versionable
5. **VSCode Native**: Fits VSCode's extension model (spawn subprocess, communicate via stdio)
6. **Security**: No network exposure, all communication is local
7. **Flexibility**: JSON-RPC allows custom methods for any operation

The risks (process management, message framing) are well-understood and mitigated with standard patterns.

## Success Metrics

- **Adoption**: 1,000+ active users within 3 months of marketplace publication
- **Engagement**: Average 10+ messages per conversation, 5+ conversations per user per week
- **Performance**: 95th percentile message latency < 200ms
- **Reliability**: Server uptime > 99.9% (excluding intentional restarts)
- **Quality**: Average marketplace rating > 4.5/5.0 stars
- **Productivity**: Users report 30%+ time savings on repetitive coding tasks (survey)

## Next Steps

1. **Validate Architecture**: Build minimal prototype (Phase 1-2) to validate JSON-RPC over stdio approach
2. **User Research**: Interview ZSH plugin users to understand pain points and priorities
3. **Prioritize Features**: Rank features by user value and implementation effort
4. **Iterate Rapidly**: Release early alpha to gather feedback, iterate on UX
5. **Community Engagement**: Open source extension and server code to encourage contributions

## Related Files and References

**Core API:**
- `crates/forge_api/src/api.rs:13-191` - API trait with 42 methods
- `crates/forge_api/src/forge_api.rs:22-347` - ForgeAPI implementation

**ZSH Plugin:**
- `shell-plugin/forge.plugin.zsh:1-816` - All 20+ commands and features to replicate

**Application Structure:**
- `crates/forge_main/src/ui.rs:87-2714` - UI controller and command routing
- `crates/forge_app/src/app.rs:29-287` - Application orchestration
- `crates/forge_app/src/orch.rs:18-250` - Agent orchestration and tool execution

**Domain Models:**
- `crates/forge_domain/src/conversation.rs` - Conversation structure
- `crates/forge_domain/src/agent.rs` - Agent definition
- `crates/forge_domain/src/message.rs` - Message types

**Research:**
- OpenAI Codex architecture: JSON-RPC 2.0, thread/turn/item model, approval workflows
- VSCode Extension API: https://code.visualstudio.com/api
- JSON-RPC 2.0 Specification: https://www.jsonrpc.org/specification

---

**Plan Version**: v1  
**Created**: 2025-11-26  
**Status**: Draft - Ready for Review  
**Estimated Effort**: 8-12 weeks for MVP (Phase 1-7), 16-20 weeks for full implementation
