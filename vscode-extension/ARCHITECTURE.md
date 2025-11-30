# VSCode Extension Architecture

## Overview

The VSCode extension acts as a bridge between the Forge Server (Rust) and the Webview UI (React):

```
┌─────────────────────────────────────────────────────────────┐
│                     VSCode Extension Host                    │
│                                                              │
│  ┌────────────┐         ┌────────────┐        ┌──────────┐ │
│  │ Controller │◄───────►│ ForgeServer│◄──────►│ ForgeAPI │ │
│  │            │ JSON-RPC│   (Rust)   │        │  (Rust)  │ │
│  └──────┬─────┘  stdio  └────────────┘        └──────────┘ │
│         │                                                    │
│         │                                                    │
│         ▼                                                    │
│  ┌────────────────┐                                         │
│  │WebviewProvider │                                         │
│  │                │                                         │
│  └───────┬────────┘                                         │
│          │ postMessage                                      │
└──────────┼──────────────────────────────────────────────────┘
           │
           ▼
   ┌───────────────────┐
   │   Webview (React) │
   │                   │
   │  - Effect-TS      │
   │  - State Mgmt     │
   │  - UI Components  │
   └───────────────────┘
```

## Message Flow

### 1. User Input (Webview → Server)

```
User types message
  ↓
Webview React Component
  ↓
VscodeRpcService.sendMessage()
  ↓
window.postMessage({ jsonrpc: '2.0', method: 'chat/sendMessage', params: {text} })
  ↓
WebviewProvider.handleWebviewMessage()
  ↓
Controller.handleSendMessage()
  ↓
ForgeServer (turn/start request over stdio)
```

### 2. Server Response (Server → Webview)

```
ForgeServer sends chat/event notification
  ↓
Controller receives via stdio
  ↓
Controller.handleChatEvent()
  ↓
WebviewProvider.postMessage({ jsonrpc: '2.0', method: 'stream/delta', params: {...} })
  ↓
Webview JsonRpcService receives
  ↓
Effect-TS stream processes
  ↓
React UI updates
```

## Communication Protocols

### Extension ↔ Server (JSON-RPC 2.0 over stdio)

**Defined by**: `forge_app_server/src/protocol/`

**Client → Server Requests**:
- `initialize` - Start session
- `thread/start` - New conversation
- `turn/start` - Send message
- `turn/cancel` - Cancel ongoing turn
- `model/list` - Get models
- `agent/list` - Get agents
- `model/set` - Change model
- `agent/set` - Change agent

**Server → Client Notifications**:
- `chat/event` - Primary event stream (TaskMessage, TaskReasoning, ToolCallStart, etc.)
- `turn/started` - Turn began
- `turn/completed` - Turn finished

### Extension ↔ Webview (postMessage)

**Defined by**: This extension's internal protocol

**Webview → Extension**:
```typescript
{
  jsonrpc: '2.0',
  method: 'chat/sendMessage' | 'model/change' | 'agent/change' | 'turn/cancel',
  params: {...}
}
```

**Extension → Webview**:
```typescript
{
  jsonrpc: '2.0',
  method: 'stream/start' | 'stream/delta' | 'stream/end' | 'models/list' | 'agents/list' | 'header/update',
  params: {...}
}
```

## Key Components

### Controller (`src/controller.ts`)
- Manages server connection
- Translates between server protocol and webview protocol
- Handles server notifications (chat/event)
- Sends requests to server

### WebviewProvider (`src/webview/provider.ts`)
- Manages webview lifecycle
- Forwards messages between controller and webview
- No business logic - just a message router

### Webview JsonRpcService (`webview-ui/src/infrastructure/rpc/JsonRpcService.ts`)
- Receives notifications from extension
- Provides Effect-TS stream of notifications
- Sends requests to extension

### ChatStateService (`webview-ui/src/application/state/ChatStateService.ts`)
- Manages React application state
- Processes notifications from JsonRpcService
- Provides reactive state streams

## Protocol Translation

The Controller is responsible for translating between protocols:

### Server `chat/event` → Webview Notifications

| Server Event (ChatResponse) | Webview Notification |
|----------------------------|----------------------|
| `TaskMessage{PlainText}` | `stream/delta{delta}` |
| `TaskMessage{Markdown}` | `stream/delta{delta}` |
| `TaskReasoning{content}` | `reasoning/show{text}` |
| `ToolCallStart{...}` | `tool/callStart{...}` |
| `ToolCallEnd{...}` | `tool/callEnd{...}` |
| `Usage{...}` | `header/update{tokens, cost}` |
| `TaskComplete` | `stream/end{}` |

### Webview Methods → Server Requests

| Webview Method | Server Request |
|---------------|----------------|
| `chat/sendMessage` | `turn/start` |
| `model/change` | `model/set` |
| `agent/change` | `agent/set` |
| `turn/cancel` | `turn/cancel` |
| `models/request` | `model/list` |
| `agents/request` | `agent/list` |

## Important Notes

1. **Webview does NOT talk to server directly** - Always goes through Controller
2. **Controller is the translator** - Maps between server protocol and webview protocol
3. **WebviewProvider is dumb** - Just routes messages, no logic
4. **Server protocol is canonical** - Defined in Rust, generated TypeScript types
5. **Webview protocol is internal** - Can be changed as needed for UI convenience
