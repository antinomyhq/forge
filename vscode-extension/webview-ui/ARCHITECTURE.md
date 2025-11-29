# ForgeCode VSCode Extension - React Migration

This document describes the React-based webview architecture for the ForgeCode VSCode extension.

## Architecture Overview

```
┌─────────────────────────────────────────┐
│   VSCode Extension (TypeScript/Node)    │
│   - Extension Host (src/)               │
│   - ServerManager (stdio)               │
│   - JsonRpcClient (stdio)               │
│   - WebviewProvider                      │
└──────────────────┬──────────────────────┘
                   │ postMessage
┌──────────────────▼──────────────────────┐
│   React App (webview-ui/)               │
│   - Onion Architecture                  │
│   - Effect-TS for side effects          │
│   - Domain, Application, Infrastructure │
│   - Presentation (React Components)     │
└──────────────────┬──────────────────────┘
                   │ JSON-RPC (postMessage)
┌──────────────────▼──────────────────────┐
│        forge-app-server (Rust)          │
│        JSON-RPC over stdio              │
└─────────────────────────────────────────┘
```

## Directory Structure

```
vscode-extension/
├── src/                      # Extension host code (TypeScript/Node.js)
│   ├── extension.ts         # Main extension entry point
│   ├── webview/
│   │   └── provider.ts      # WebviewProvider for React app
│   ├── server/
│   │   ├── manager.ts       # Server process manager
│   │   └── client.ts        # JSON-RPC client (stdio)
│   └── controller.ts        # Main controller
│
└── webview-ui/              # React application
    ├── src/
    │   ├── domain/          # Domain layer (business logic)
    │   │   ├── models/      # Effect Schema models
    │   │   ├── services/    # Domain services
    │   │   └── events/      # Domain events
    │   ├── application/     # Application layer
    │   │   ├── useCases/    # Use case implementations
    │   │   └── state/       # State management (Effect-TS)
    │   ├── infrastructure/  # Infrastructure layer
    │   │   └── rpc/         # JSON-RPC via postMessage
    │   ├── presentation/    # Presentation layer
    │   │   ├── components/  # React components
    │   │   ├── hooks/       # React hooks
    │   │   └── pages/       # Page components
    │   └── shared/          # Shared utilities
    └── dist/                # Built React app (generated)
```

## Communication Flow

### 1. Extension Host → Server (stdio)
```typescript
// src/server/client.ts
const rpcClient = new JsonRpcClient(stdin, stdout);
const result = await rpcClient.request('startThread', params);
```

### 2. Extension Host ↔ React Webview (postMessage)
```typescript
// Extension → Webview
webviewProvider.postMessage({ 
  jsonrpc: "2.0", 
  id: "123", 
  result: data 
});

// Webview → Extension
vscode.postMessage({ 
  jsonrpc: "2.0", 
  id: "456", 
  method: "sendMessage", 
  params: {...} 
});
```

### 3. React App Infrastructure (postMessage Client)
```typescript
// webview-ui/src/infrastructure/rpc/JsonRpcService.ts
const vscode = acquireVsCodeApi();

// Send request
vscode.postMessage({ jsonrpc: "2.0", id, method, params });

// Receive response using window.addEventListener
window.addEventListener('message', (event) => {
  const message = event.data;
  // Handle response or notification
});
```

**Important:** VSCode webviews use `window.addEventListener('message', ...)` to receive messages, not a method on the VSCode API object.

## Onion Architecture Layers

### Domain Layer (`webview-ui/src/domain/`)
- **Models**: Type-safe domain models using Effect Schema
  - `Message`, `Conversation`, `AgentConfig`, `FileContext`, `ToolExecution`, `StreamDelta`
- **Services**: Pure business logic
  - `MessageValidationService`, `ConversationService`, `StreamingService`
- **Events**: Domain events
  - `MessageSentEvent`, `ConversationStartedEvent`, `StreamDeltaReceivedEvent`

### Application Layer (`webview-ui/src/application/`)
- **Use Cases**: Application-specific business rules
  - `SendMessageUseCase`, `StartConversationUseCase`, `LoadConversationsUseCase`, `SubscribeToStreamUseCase`
- **State Management**: Effect-TS based state service
  - Immutable state with `Effect.Ref`
  - Reactive updates with `Effect.Queue`
  - State subscriptions with `Stream`

### Infrastructure Layer (`webview-ui/src/infrastructure/`)
- **RPC Client**: JSON-RPC over VSCode postMessage API
  - Type-safe requests/responses
  - Timeout management
  - Notification streaming

### Presentation Layer (`webview-ui/src/presentation/`)
- **React Components**: UI components
  - `ChatContainer`, `MessageList`, `MessageItem`, `InputBox`, `Sidebar`
- **React Hooks**: Effect-TS integration
  - `useRuntime` - Effect runtime provider
  - `useEffectHook` - Execute Effects with loading/error states
  - `useStateSelector` - Subscribe to state slices
  - `useDispatch` - Dispatch state actions

## Effect-TS Integration

### Why Effect-TS?
- **Type Safety**: Fully typed side effects
- **Composability**: Build complex operations from simple ones
- **Resource Management**: Automatic cleanup with scoped effects
- **Error Handling**: Structured error handling with typed errors

### Key Patterns

#### 1. Service Definition
```typescript
export interface JsonRpcService {
  readonly request: <A, E = never>(
    method: string,
    params: unknown
  ) => Effect.Effect<A, RpcError | E>;
}
```

#### 2. Service Implementation
```typescript
export const JsonRpcServiceLive = Layer.scoped(
  JsonRpcService,
  Effect.gen(function* () {
    // Setup resources
    return JsonRpcService.of({
      request: (method, params) => Effect.gen(function* () {
        // Implementation
      })
    });
  })
);
```

#### 3. React Integration
```typescript
const { data, loading, error } = useEffectHook(() =>
  SendMessageUseCase.execute(text, files)
);
```

## Build Process

### Development
```bash
# Watch extension code
npm run watch

# Watch React app (in another terminal)
npm run watch:webview
```

### Production Build
```bash
# Build everything
npm run vscode:prepublish

# This runs:
# 1. npm run compile        → Builds extension TypeScript
# 2. npm run compile:webview → Builds React app
```

### Build Output
- Extension: `out/` directory
- React App: `webview-ui/dist/` directory

## Key Features Implemented

### ✅ Domain Models with Effect Schema
- Runtime validation
- Type inference
- Serialization/deserialization

### ✅ Type-Safe RPC Communication
- JSON-RPC 2.0 protocol
- Request/response correlation
- Notification streaming
- Timeout handling

### ✅ Immutable State Management
- Effect Ref for state storage
- Effect Queue for updates
- Stream-based subscriptions
- Reducer pattern for mutations

### ✅ React Component Library
- Chat interface components
- Sidebar for conversations
- Input box with file reference support
- Streaming indicators

### ✅ Server-First Architecture
- No client-side persistence
- Server is single source of truth
- Real-time sync via notifications

## VSCode Webview CSP (Content Security Policy)

The webview uses a strict CSP:
```typescript
default-src 'none';
style-src ${cspSource} 'unsafe-inline';
script-src 'nonce-${nonce}';
font-src ${cspSource};
img-src ${cspSource} data:;
```

All scripts are loaded with a nonce for security.

## File References

The React app supports `@[file/path]` syntax for file references:
```
Can you review @[src/main.ts] and @[tests/unit.test.ts]?
```

These are parsed client-side and resolved server-side.

## Next Steps

### Phase 7: Migration Strategy
- [ ] Migrate existing webview UI to React components
- [ ] Update controller to work with new message format
- [ ] Implement backward compatibility layer

### Phase 8: Testing
- [ ] Unit tests for domain services
- [ ] Integration tests for use cases
- [ ] Component tests with React Testing Library
- [ ] E2E tests with VSCode extension testing

### Phase 9: Performance Optimization
- [ ] Code splitting
- [ ] Lazy loading components
- [ ] Optimize bundle size
- [ ] Implement virtual scrolling for messages

### Phase 10: Deployment
- [ ] CI/CD pipeline
- [ ] Automated testing
- [ ] Release automation
- [ ] Marketplace publishing

## Development Guidelines

### Adding a New Feature

1. **Domain Layer**: Define models and services
2. **Application Layer**: Create use case
3. **Infrastructure Layer**: Add RPC method if needed
4. **Presentation Layer**: Create React components
5. **Wire Everything**: Connect in App.tsx

### Testing Strategy

```typescript
// Domain service test
const service = new MessageValidationService();
const result = service.validate(message);
assert(Effect.runSync(result));

// Use case test
const useCase = new SendMessageUseCase(mockRpc);
const result = useCase.execute("test", []);
// Test result
```

## Resources

- [Effect-TS Documentation](https://effect.website)
- [VSCode Extension API](https://code.visualstudio.com/api)
- [JSON-RPC 2.0 Spec](https://www.jsonrpc.org/specification)
- [React Documentation](https://react.dev)
- [Onion Architecture](https://jeffreypalermo.com/2008/07/the-onion-architecture-part-1/)

## License

MIT
