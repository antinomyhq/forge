# VSCode Extension - Simplified Cline-inspired Interface

## Objective

Build a minimal, working VSCode extension for ForgeCode that focuses on core chat functionality similar to Cline, with incremental feature additions. The extension will use the existing `forge-app-server` for backend communication via JSON-RPC over stdio, with **auto-generated TypeScript types from Rust**.

**Key Principles:**
- Start with minimal working chat interface
- Incremental feature additions
- Auto-generate types from Rust (no manual sync)
- Focus on user experience over features
- Learn from Cline's architecture but keep it simple

## Research Summary

Based on Cline architecture analysis:
- **Three-layer architecture**: Extension → Core (Controller) → Backend (forge-app-server)
- **WebView for UI**: React-based chat interface with VSCode WebView
- **gRPC/Protocol Buffers**: Type-safe communication (we'll use JSON-RPC + generated types)
- **Controller pattern**: Central orchestrator for state and communication
- **Task-based execution**: Manage AI conversation loops with tool execution
- **State management**: Persist conversations and settings

## UI Design Specification (Based on Cline)

### Layout Structure

```
┌─────────────────────────────────────────┐
│  Navbar (optional, top)                 │
├─────────────────────────────────────────┤
│                                         │
│  Main Content Area (Chat Messages)      │
│  - Scrollable                           │
│  - Auto-scroll to bottom                │
│  - Message rows with fade-in animation  │
│                                         │
├─────────────────────────────────────────┤
│  Footer (fixed bottom)                  │
│  - Action buttons                       │
│  - Input area                           │
└─────────────────────────────────────────┘
```

### CSS Grid Layout

```css
.chat-layout {
  display: grid;
  grid-template-rows: 1fr auto;  /* Main content + Footer */
  overflow: hidden;
  height: 100vh;
}

.main-content {
  display: flex;
  flex-direction: column;
  overflow: hidden;
  grid-row: 1;
}

.footer {
  grid-row: 2;
  padding: 8px 16px;
  background: var(--vscode-editor-background);
  border-top: 1px solid var(--vscode-panel-border);
}
```

### Message Types & Styling

#### User Messages
- **Color**: `#8B949E` (gray)
- **Background**: `var(--vscode-input-background)`
- **Border**: `1px solid var(--vscode-input-border)`
- **Border Radius**: `6px`
- **Padding**: `12px 16px`
- **Alignment**: Right side
- **Icon**: User avatar or `$(person)` icon

#### AI Messages (Assistant)
- **Color**: `#E5E5E5` (white/light gray)
- **Background**: Transparent or `var(--vscode-editor-background)`
- **Border**: None or subtle
- **Padding**: `12px 16px`
- **Alignment**: Left side
- **Icon**: AI avatar or `$(sparkle)` icon
- **Markdown rendering**: Supported

#### Tool Messages
Different colors for different tool types:
- **File Read**: `#F0C674` (beige) - with `$(file)` icon
- **File Edit/Create**: `#58A6FF` (blue) - with `$(edit)` icon
- **Terminal Command**: `#F85149` (red) - with `$(terminal)` icon
- **Browser Action**: `#BC8CFF` (purple) - with `$(browser)` icon
- **Success/Complete**: `#56D364` (green) - with `$(check)` icon

Tool message structure:
```css
.tool-message {
  border-radius: 6px;
  border: 1px solid var(--vscode-editorGroup-border);
  background-color: rgba(var(--tool-color-rgb), 0.1);
  overflow: visible;
  transition: all 0.3s ease-in-out;
}
```

### Input Area Design

```css
.input-area {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.textarea {
  background-color: var(--vscode-input-background);
  color: var(--vscode-input-foreground);
  border: 1px solid var(--vscode-input-border);
  border-radius: 6px;
  padding: 12px;
  resize: none;
  font-family: var(--vscode-font-family);
  font-size: 13px;
  line-height: 1.5;
  outline: none;
}

.textarea:focus {
  outline: 1px solid var(--vscode-focusBorder);
  outline-offset: -1px;
}

.textarea::placeholder {
  color: var(--vscode-input-placeholderForeground);
}
```

Auto-resize behavior:
- **Min height**: `40px` (1-2 lines)
- **Max height**: `200px` (before scroll)
- **Auto-expand**: As user types

### Button Styles

#### Primary Action Button (Send, Approve)
```css
.button-primary {
  background: var(--vscode-button-background);
  color: var(--vscode-button-foreground);
  border: none;
  border-radius: 4px;
  padding: 8px 16px;
  font-size: 13px;
  cursor: pointer;
}

.button-primary:hover {
  background: var(--vscode-button-hoverBackground);
}

.button-primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
```

#### Secondary Action Button (Cancel, Reject)
```css
.button-secondary {
  background: var(--vscode-button-secondaryBackground);
  color: var(--vscode-button-secondaryForeground);
  border: none;
  border-radius: 4px;
  padding: 8px 16px;
  font-size: 13px;
}

.button-secondary:hover {
  background: var(--vscode-button-secondaryHoverBackground);
}
```

#### Icon Buttons (Add Files, Settings)
```css
.button-icon {
  background: transparent;
  color: var(--vscode-foreground);
  border: none;
  padding: 6px;
  cursor: pointer;
  border-radius: 4px;
}

.button-icon:hover {
  background: var(--vscode-toolbar-hoverBackground);
}
```

### Code Blocks in Messages

```css
.code-block {
  background: rgba(127, 127, 127, 0.1);
  border: 1px solid var(--vscode-editorGroup-border);
  border-radius: 3px;
  overflow: auto;
  margin: 8px 0;
}

.code-block pre {
  padding: 12px;
  margin: 0;
  font-family: var(--vscode-editor-font-family);
  font-size: 12px;
  line-height: 1.4;
}
```

### Message Animations

#### Fade In (for new messages)
```css
@keyframes fadeIn {
  from {
    opacity: 0;
    transform: translateY(10px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.message-row {
  animation: fadeIn 0.3s ease-in-out;
}
```

#### Streaming Indicator (while AI is typing)
```css
@keyframes pulse {
  0%, 100% {
    opacity: 0.5;
  }
  50% {
    opacity: 1;
  }
}

.streaming-indicator {
  animation: pulse 1.5s infinite;
}
```

### Action Buttons Layout

```
┌─────────────────────────────────────────┐
│ [Approve] [Reject] [Cancel]             │
│                                         │
│ Or inline with message:                 │
│ ┌─────────────────────────────────────┐ │
│ │ Tool wants to edit file.rs          │ │
│ │ [Approve] [Reject] [View Diff]      │ │
│ └─────────────────────────────────────┘ │
└─────────────────────────────────────────┘
```

Action buttons positioned:
- **Bottom of screen**: For global actions (stop task, new task)
- **Inline with messages**: For approval workflows (approve file change, approve command)

### Welcome Screen (No Active Task)

```
┌─────────────────────────────────────────┐
│                                         │
│         ╭─────────────────────╮         │
│         │  ForgeCode Logo     │         │
│         ╰─────────────────────╯         │
│                                         │
│   Welcome to ForgeCode                  │
│   Start a conversation with AI          │
│                                         │
│   Recent Tasks:                         │
│   • Task 1 - 2 hours ago                │
│   • Task 2 - Yesterday                  │
│                                         │
└─────────────────────────────────────────┘
```

### Task Header (Active Task)

```
┌─────────────────────────────────────────┐
│ Agent: Forge  │  Model: Claude 3.5      │
│ Tokens: 1.2K / 200K                     │
│ Cost: $0.02                             │
├─────────────────────────────────────────┤
```

Shows:
- Active agent name
- Active model name
- Token usage (current / limit)
- Estimated cost (if available)

### Navbar (Optional)

```
┌─────────────────────────────────────────┐
│ [Settings] [History] [New Task] [Stop]  │
└─────────────────────────────────────────┘
```

Icon buttons with tooltips:
- **Settings**: Open configuration
- **History**: View past conversations
- **New Task**: Start new conversation
- **Stop**: Stop current task

### Responsive Behavior

- **Minimum width**: 320px (sidebar minimum)
- **Maximum width**: Flexible (sidebar can be resized)
- **Mobile/small screens**: 
  - Stack action buttons vertically
  - Reduce padding
  - Smaller font sizes

### Theme Integration

All colors use VSCode CSS variables to support:
- ✅ Light themes
- ✅ Dark themes
- ✅ High contrast themes
- ✅ Custom themes

Key variables:
- `--vscode-foreground`
- `--vscode-editor-background`
- `--vscode-input-background`
- `--vscode-input-foreground`
- `--vscode-input-border`
- `--vscode-button-background`
- `--vscode-focusBorder`
- `--vscode-editorGroup-border`

### Accessibility

- ✅ Keyboard navigation (Tab, Enter, Esc)
- ✅ Screen reader support (ARIA labels)
- ✅ Focus indicators (visible outline)
- ✅ High contrast mode support
- ✅ Semantic HTML elements

## Implementation Plan

### Phase 1: Auto-Generated Types & Foundation (Priority: CRITICAL)

- [ ] 1.1: Set up TypeScript type generation from Rust
  - Install and configure `typeshare` crate in `forge_app_server`
  - Add `#[typeshare]` annotations to all protocol types (ClientRequest, ServerNotification, Thread, Turn, Item, etc.)
  - Create build script to generate TypeScript definitions to `vscode-extension/src/generated/types.ts`
  - Add npm script to regenerate types: `npm run generate-types`
  - Verify types are generated correctly and import in TypeScript

- [ ] 1.2: Clean extension structure
  - Reset package.json with minimal configuration
  - Set up basic tsconfig.json
  - Create simple directory structure:
    ```
    src/
      extension.ts        (entry point)
      controller.ts       (orchestrator)
      serverManager.ts    (stdio communication)
      webviewProvider.ts  (chat UI)
      generated/          (auto-generated types)
    webview/
      index.html          (chat interface)
      main.js             (webview script)
      style.css           (styling)
    ```

- [ ] 1.3: Basic server communication
  - Create ServerManager to spawn forge-app-server process
  - Implement JSON-RPC request/response handling over stdio
  - Handle initialize handshake
  - Add connection health checks

### Phase 2: Minimal Chat Interface (Priority: HIGH)

- [ ] 2.1: Create HTML structure (webview/index.html)
  - CSS Grid layout with main content area and footer
  - Message container with auto-scroll
  - Input area with auto-resizing textarea
  - Action buttons container
  - Use VSCode codicons for icons
  - Load marked.js for Markdown rendering
  - Include nonce-based CSP for security

- [ ] 2.2: Implement CSS styling (webview/style.css)
  - Grid layout: `grid-template-rows: 1fr auto`
  - Message styles:
    - User messages: right-aligned, gray background (#8B949E)
    - AI messages: left-aligned, transparent background
    - Tool messages: color-coded by type (read, write, shell, etc.)
  - Input area:
    - Background: `var(--vscode-input-background)`
    - Border radius: 6px
    - Focus outline: `var(--vscode-focusBorder)`
  - Button styles:
    - Primary: `var(--vscode-button-background)`
    - Secondary: `var(--vscode-button-secondaryBackground)`
    - Icon buttons with hover effects
  - Animations:
    - fadeIn for new messages (0.3s)
    - pulse for streaming indicator (1.5s)
  - Responsive: minimum width 320px

- [ ] 2.3: Implement webview logic (webview/main.js)
  - Message rendering function:
    - Parse message type (user, assistant, tool)
    - Render with appropriate styling
    - Convert markdown to HTML with marked.js
    - Apply syntax highlighting to code blocks
  - Auto-scroll to bottom on new messages
  - Textarea auto-resize (40px-200px)
  - Send message on Ctrl+Enter or button click
  - Handle postMessage for extension communication
  - State management for messages array

- [ ] 2.4: WebviewProvider implementation (src/webviewProvider.ts)
  - Generate HTML with CSP nonce
  - Load CSS and JS with proper webview URIs
  - Manage webview lifecycle:
    - Create webview on first show
    - Preserve on hide (don't recreate)
    - Dispose properly on close
  - Message passing:
    - Receive messages from webview (user input, button clicks)
    - Send messages to webview (AI responses, tool calls, state updates)
  - State preservation across hide/show

- [ ] 2.5: Controller for orchestration (src/controller.ts)
  - Single source of truth for application state
  - Coordinate between ServerManager and WebviewProvider
  - Handle message flow:
    1. User sends message (webview → controller)
    2. Controller sends to server (controller → server)
    3. Server streams response (server → controller)
    4. Controller updates webview (controller → webview)
  - Manage active task/conversation
  - Error handling and recovery:
    - Server connection errors
    - Protocol errors
    - Timeout handling
  - State synchronization

- [ ] 2.6: Basic chat functionality
  - Send user message flow:
    - Validate input (non-empty)
    - Add to message history
    - Send to server via thread/start or turn/start
    - Disable input while waiting
  - Receive AI response:
    - Handle `AgentMessageDelta` notifications
    - Append delta to current message
    - Update UI in real-time
    - Show streaming indicator
  - Display messages:
    - User messages with timestamp
    - AI messages with markdown formatting
    - Tool calls with color-coded badges
    - Completion/error messages
  - UI states:
    - Idle: Ready for input
    - Sending: Disabled input, show spinner
    - Streaming: Show typing indicator
    - Error: Show error message, enable retry

### Phase 3: Essential Features (Priority: MEDIUM)

- [ ] 3.1: Conversation management
  - List recent conversations in sidebar tree view
  - Create new conversation
  - Switch between conversations
  - Delete conversations

- [ ] 3.2: Tool execution visualization
  - Show tool calls in chat (read, write, patch, shell, etc.)
  - Display tool execution status (running, completed, failed)
  - Show tool results inline

- [ ] 3.3: Approval workflows
  - Intercept file change requests from server
  - Show approval dialog with diff preview
  - Send approval/rejection back to server
  - Handle command execution approvals similarly

### Phase 4: Configuration & Settings (Priority: MEDIUM)

- [ ] 4.1: Settings UI in webview
  - Model selection
  - Agent selection
  - Provider configuration (API keys via VSCode secrets)
  - Basic preferences (auto-approve read-only operations, etc.)

- [ ] 4.2: Status bar integration
  - Show active agent and model
  - Connection status indicator
  - Quick access to settings

### Phase 5: Enhanced UX (Priority: LOW)

- [ ] 5.1: File context
  - "@-mention" files in chat (e.g., @src/main.rs)
  - Right-click file → "Add to Forge Context"
  - Show tagged files in sidebar

- [ ] 5.2: Git integration
  - Generate commit messages
  - Show diff before committing
  - Stage/unstage files

- [ ] 5.3: Command palette integration
  - Quick commands for common actions
  - Keyboard shortcuts
  - Command history

### Phase 6: Polish & Optimization (Priority: LOW)

- [ ] 6.1: Error handling
  - Graceful degradation
  - Clear error messages
  - Auto-reconnect on server crash

- [ ] 6.2: Performance
  - Message virtualization for long conversations
  - Lazy loading of old messages
  - Debounce user input

- [ ] 6.3: Accessibility
  - Keyboard navigation
  - Screen reader support
  - High contrast theme support

## Verification Criteria

### Phase 1 Complete When:
- ✅ Running `npm run generate-types` creates TypeScript types from Rust
- ✅ Extension activates without errors
- ✅ Server spawns and responds to initialize request
- ✅ Types are imported and used in TypeScript without errors

### Phase 2 Complete When:
- ✅ Webview opens with chat interface
- ✅ User can type and send a message
- ✅ Message appears in chat history
- ✅ Server response streams back in real-time
- ✅ No protocol errors in logs

### Phase 3 Complete When:
- ✅ User can create multiple conversations
- ✅ Conversations persist across VSCode restarts
- ✅ Tool calls are visible in chat
- ✅ File changes require approval before execution

### Phase 4 Complete When:
- ✅ User can change model/agent from UI
- ✅ Settings persist in VSCode storage
- ✅ Status bar shows current configuration

### Phase 5 Complete When:
- ✅ User can @-mention files
- ✅ Git commit message generation works
- ✅ Command palette has all major actions

### Phase 6 Complete When:
- ✅ Extension handles 1000+ message conversations smoothly
- ✅ Server crashes don't lose user data
- ✅ Extension passes VSCode accessibility audit

## Technical Decisions

### Type Generation: typeshare vs ts-rs

**Choice: typeshare**

Rationale:
- Better support for complex Rust types (enums, generics)
- Generates TypeScript/Flow/Kotlin/Swift/Go
- Active maintenance
- Works with serde
- Simple attribute macros: `#[typeshare]`

Alternative considered: `ts-rs`
- Issue: Doesn't support Uuid out of the box
- Requires custom implementations for many types
- More complex setup

### Communication: JSON-RPC vs gRPC

**Choice: JSON-RPC over stdio (existing)**

Rationale:
- Already implemented in forge-app-server
- Simpler than gRPC for this use case
- No additional dependencies
- Easy to debug (plain JSON)

Alternative considered: gRPC (like Cline)
- Better for complex streaming scenarios
- Type-safe by design
- Overkill for current needs

### UI Framework: Vanilla JS vs React

**Choice: Start with Vanilla JS, migrate to React later**

Rationale:
- Phase 1-2: Vanilla JS for speed (no build setup)
- Phase 3+: Migrate to React for complex state management
- Keeps initial implementation simple
- Webview build tooling can be added incrementally

## Potential Risks and Mitigations

### 1. Type Generation Complexity
**Risk**: typeshare might not handle all Rust types correctly (Uuid, custom types, etc.)

**Mitigation**:
- Create custom type mappings for problematic types
- Use newtype pattern for Uuid → string mapping
- Add integration tests to verify type correctness
- Fallback: Manual type definitions with clear documentation

### 2. Server Communication Reliability
**Risk**: stdio communication might be unreliable or hard to debug

**Mitigation**:
- Add comprehensive logging on both sides
- Implement heartbeat/ping mechanism
- Auto-restart server on crash
- Buffer messages during reconnection

### 3. Webview State Management
**Risk**: Complex state management in webview without framework

**Mitigation**:
- Start with simple message passing
- Migrate to React when state complexity grows
- Use VSCode state API for persistence
- Document state transitions clearly

### 4. Protocol Mismatches
**Risk**: Client and server protocol might drift despite type generation

**Mitigation**:
- Auto-generate types on every server build
- Add protocol version checking
- Integration tests for all request/response pairs
- Document breaking changes clearly

## Alternative Approaches

### Approach 1: Full React from Day 1
**Pros**: Better for complex UI, type-safe state management
**Cons**: Slower initial development, more build complexity
**Decision**: Start simple, migrate later when needed

### Approach 2: Use Existing VSCode Extension Frameworks
**Pros**: Less boilerplate, common patterns
**Cons**: Less control, potential lock-in
**Decision**: Build custom for learning and flexibility

### Approach 3: Separate Frontend Build
**Pros**: Modern tooling (Vite, esbuild), better DX
**Cons**: More complexity, longer build times
**Decision**: Add in Phase 5 when UI complexity justifies it

## Success Metrics

### MVP Success (Phase 1-2)
- Extension activates in < 2 seconds
- Chat message round-trip < 500ms (excluding AI response time)
- Zero protocol errors during normal operation
- Types auto-generate without manual intervention

### Feature Complete (Phase 3-4)
- All ZSH plugin commands available in extension
- Conversation management works reliably
- Tool execution with approval flow complete
- Settings UI functional

### Production Ready (Phase 5-6)
- 100+ conversations performant
- < 100MB memory usage
- Accessibility score > 90%
- No critical bugs in 2 weeks of testing

## Implementation Notes

### Type Generation Setup

```toml
# crates/forge_app_server/Cargo.toml
[dependencies]
typeshare = "1.0"

[build-dependencies]
typeshare = "1.0"
```

```rust
// Add to all protocol types
#[derive(Serialize, Deserialize)]
#[typeshare]
pub struct ClientRequest { ... }
```

```javascript
// package.json
{
  "scripts": {
    "generate-types": "cd ../crates/forge_app_server && cargo build && typeshare . --lang=typescript --output-file=../../vscode-extension/src/generated/types.ts"
  }
}
```

### Incremental Development

1. **Week 1**: Phase 1 (types + foundation)
2. **Week 2**: Phase 2 (chat interface)
3. **Week 3**: Phase 3 (essential features)
4. **Week 4**: Phase 4 (settings)
5. **Week 5+**: Phase 5-6 (enhancements)

Each phase should be fully functional and usable before moving to the next.

## Dependencies

### NPM Packages (Minimal)
```json
{
  "dependencies": {
    "marked": "^11.0.0"  // Markdown rendering
  },
  "devDependencies": {
    "@types/vscode": "^1.85.0",
    "@types/node": "^20.0.0",
    "typescript": "^5.3.0"
  }
}
```

### Rust Crates (forge_app_server)
```toml
[dependencies]
typeshare = "1.0"  // Type generation

[build-dependencies]
typeshare = "1.0"
```

## File Structure (Final)

```
vscode-extension/
├── package.json
├── tsconfig.json
├── src/
│   ├── extension.ts           # Entry point, activation
│   ├── controller.ts           # Orchestrator (like Cline)
│   ├── serverManager.ts        # Spawn/manage forge-app-server
│   ├── webviewProvider.ts      # Manage chat webview
│   ├── conversationManager.ts  # Conversation state
│   ├── approvalManager.ts      # File/command approvals
│   └── generated/
│       └── types.ts            # Auto-generated from Rust
├── webview/
│   ├── index.html             # Chat UI
│   ├── main.js                # Webview logic
│   └── style.css              # Styling
└── media/
    └── icons/                 # Extension icons
```

## Next Steps

1. **Immediate**: Start Phase 1.1 - Set up typeshare and generate initial types
2. **Then**: Implement Phase 1.2-1.3 - Basic server communication
3. **Test**: Verify types work correctly with real server
4. **Document**: Update this plan with learnings and adjustments

---

**This plan prioritizes working software over comprehensive features. Each phase delivers value and can be tested independently.**
