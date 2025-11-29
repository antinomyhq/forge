# Plan: Convert VSCode Extension to React Application with Onion Architecture

**Created:** 2025-11-29  
**Status:** Draft  
**Version:** 1.0

---

## Executive Summary

This plan outlines the conversion of the existing VSCode extension to a standalone React application following onion architecture principles with Effect-TS as the core functional programming foundation. The new architecture will provide proper separation of concerns, type-safe effects, and scalable state management.

---

## Current State Analysis

### Existing Architecture
- **Type:** VSCode Extension with webview-based UI
- **Language:** TypeScript
- **UI:** Vanilla HTML/CSS/JavaScript in webviews
- **State Management:** Imperative, component-level state holders
- **Communication:** JSON-RPC over stdio with Rust backend
- **Dependencies:** Minimal (only @vscode/codicons)

### Key Components
1. **Controller** (`src/controller.ts`) - Central orchestration layer
2. **JsonRpcClient** (`src/server/client.ts`) - JSON-RPC 2.0 protocol handler
3. **ServerManager** (`src/server/manager.ts`) - Process lifecycle manager
4. **ChatWebviewProvider** (`src/webview/provider.ts`) - Chat UI webview
5. **SettingsWebviewProvider** (`src/settings/webviewProvider.ts`) - Settings UI
6. **ConversationTreeProvider** (`src/conversation/treeProvider.ts`) - Conversation list
7. **FileContextManager** (`src/file/contextManager.ts`) - File tagging system
8. **Generated Types** (`src/generated/*`) - Auto-generated from Rust

### Current Communication Flow
```
User → Webview → Provider → Controller → JsonRpcClient → ServerManager → Rust Server
```

---

## Target Architecture

### Onion Architecture Layers

```
┌─────────────────────────────────────────────────────────┐
│                    Presentation Layer                    │
│  - React Components (UI)                                 │
│  - View Models                                           │
│  - React Hooks for Effect-TS integration                │
└─────────────────────────────────────────────────────────┘
                           ↓
┌─────────────────────────────────────────────────────────┐
│                   Application Layer                      │
│  - Use Cases / Application Services                      │
│  - Command Handlers                                      │
│  - Query Handlers                                        │
│  - Effect-TS Program Definitions                         │
└─────────────────────────────────────────────────────────┘
                           ↓
┌─────────────────────────────────────────────────────────┐
│                     Domain Layer                         │
│  - Domain Models (Effect Data types)                     │
│  - Domain Services                                       │
│  - Business Rules                                        │
│  - Domain Events                                         │
└─────────────────────────────────────────────────────────┘
                           ↓
┌─────────────────────────────────────────────────────────┐
│                  Infrastructure Layer                    │
│  - JSON-RPC Client Implementation (WebSocket)            │
│  - Server Process Manager (optional for web)             │
│  - Effect Layers (Services)                              │
└─────────────────────────────────────────────────────────┘
```

### Effect-TS Integration Strategy

All asynchronous operations, side effects, and error handling will use Effect-TS:

- **Effect<Success, Error, Requirements>** - For all async operations
- **Layer** - For dependency injection
- **Schema** - For runtime validation and type safety
- **Stream** - For streaming responses
- **Fiber** - For concurrent operations
- **Ref** - For mutable state management
- **Hub** - For pub/sub messaging

---

## Phase 1: Foundation Setup

### 1.1 Project Initialization

**Goal:** Set up new React application with Effect-TS foundation

**Tasks:**
- [ ] Create new React application using Vite
  - Use TypeScript template
  - Configure for modern ES modules
- [ ] Install core dependencies:
  ```json
  {
    "effect": "^3.9.0",
    "@effect/schema": "^0.75.0",
    "@effect/platform": "^0.68.0",
    "@effect/platform-browser": "^0.48.0",
    "react": "^18.3.0",
    "react-dom": "^18.3.0"
  }
  ```
- [ ] Install UI dependencies:
  ```json
  {
    "tailwindcss": "^3.4.0",
    "@headlessui/react": "^2.1.0",
    "@heroicons/react": "^2.1.0",
    "react-router-dom": "^6.26.0"
  }
  ```
- [ ] Configure TypeScript for strict mode + Effect-TS
  ```json
  {
    "compilerOptions": {
      "strict": true,
      "noUncheckedIndexedAccess": true,
      "exactOptionalPropertyTypes": true,
      "moduleResolution": "bundler"
    }
  }
  ```
- [ ] Set up directory structure following onion architecture
- [ ] Configure path aliases for clean imports

**Directory Structure:**
```
src/
├── domain/
│   ├── models/
│   ├── services/
│   └── events/
├── application/
│   ├── useCases/
│   ├── commands/
│   └── queries/
├── infrastructure/
│   └── rpc/
├── presentation/
│   ├── components/
│   ├── hooks/
│   ├── pages/
│   └── viewModels/
└── shared/
    ├── types/
    ├── utils/
    └── constants/
```

**Acceptance Criteria:**
- Application runs with hot module replacement
- Effect-TS imports work correctly
- TypeScript compilation has zero errors
- Directory structure matches onion architecture

---

## Phase 2: Domain Layer Implementation

### 2.1 Domain Models with Effect Schema

**Goal:** Define type-safe domain models using Effect Schema

**Tasks:**
- [ ] Create `Message` domain model
  ```typescript
  import { Schema as S } from "@effect/schema";
  import { Effect, Data } from "effect";

  export class MessageId extends S.Class<MessageId>("MessageId")({
    value: S.String.pipe(S.uuid())
  }) {}

  export class Message extends S.Class<Message>("Message")({
    id: MessageId,
    content: S.String,
    role: S.Literal("user", "assistant", "system"),
    timestamp: S.Date,
    status: S.Literal("pending", "completed", "failed"),
    metadata: S.optional(S.Record(S.String, S.Unknown))
  }) {}
  ```

- [ ] Create `Conversation` domain model
  ```typescript
  export class ConversationId extends S.Class<ConversationId>("ConversationId")({
    value: S.String.pipe(S.uuid())
  }) {}

  export class Conversation extends S.Class<Conversation>("Conversation")({
    id: ConversationId,
    threadId: S.String,
    messages: S.Array(Message),
    agent: S.String,
    model: S.String,
    createdAt: S.Date,
    updatedAt: S.Date
  }) {}
  ```

- [ ] Create `AgentConfig` domain model
  ```typescript
  export class AgentConfig extends S.Class<AgentConfig>("AgentConfig")({
    name: S.String,
    model: S.String,
    provider: S.String,
    maxTokens: S.Number,
    temperature: S.Number.pipe(S.between(0, 2))
  }) {}
  ```

- [ ] Create `FileContext` domain model
  ```typescript
  export class FileContext extends S.Class<FileContext>("FileContext")({
    filePath: S.String,
    content: S.String,
    language: S.String,
    isTagged: S.Boolean
  }) {}
  ```

- [ ] Create `ToolExecution` domain model
  ```typescript
  export class ToolExecution extends S.Class<ToolExecution>("ToolExecution")({
    id: S.String,
    toolName: S.String,
    status: S.Literal("started", "completed", "failed"),
    startTime: S.Date,
    endTime: S.optional(S.Date),
    result: S.optional(S.Unknown)
  }) {}
  ```

- [ ] Create `StreamDelta` domain model for streaming responses
  ```typescript
  export class StreamDelta extends S.Class<StreamDelta>("StreamDelta")({
    type: S.Literal("content", "reasoning", "tool"),
    content: S.String,
    timestamp: S.Date
  }) {}
  ```

**Acceptance Criteria:**
- All domain models use Effect Schema
- Models have proper validation rules
- Models are immutable (using Data.Class)
- Runtime type checking works correctly
- All models export both type and schema

### 2.2 Domain Services

**Goal:** Implement pure business logic as domain services

**Tasks:**
- [ ] Create `MessageValidationService`
  ```typescript
  export class MessageValidationService {
    static validateMessage(message: string): Effect.Effect<string, ValidationError> {
      return Effect.gen(function* () {
        if (message.trim().length === 0) {
          yield* Effect.fail(new ValidationError({ reason: "Message cannot be empty" }));
        }
        if (message.length > 10000) {
          yield* Effect.fail(new ValidationError({ reason: "Message too long" }));
        }
        return message.trim();
      });
    }
  }
  ```

- [ ] Create `ConversationService`
  ```typescript
  export class ConversationService {
    static addMessage(
      conversation: Conversation,
      message: Message
    ): Effect.Effect<Conversation, DomainError> {
      return Effect.gen(function* () {
        // Validate message
        yield* MessageValidationService.validateMessage(message.content);
        
        // Create updated conversation
        return new Conversation({
          ...conversation,
          messages: [...conversation.messages, message],
          updatedAt: new Date()
        });
      });
    }

    static calculateTokenUsage(
      conversation: Conversation
    ): Effect.Effect<number, never> {
      return Effect.succeed(
        conversation.messages.reduce((sum, msg) => sum + msg.content.length, 0)
      );
    }
  }
  ```

- [ ] Create `StreamingService` for handling deltas
  ```typescript
  export class StreamingService {
    static mergeDeltas(
      deltas: ReadonlyArray<StreamDelta>
    ): Effect.Effect<string, never> {
      return Effect.succeed(
        deltas.map(d => d.content).join("")
      );
    }
  }
  ```

**Acceptance Criteria:**
- Services contain only pure business logic
- All methods return Effect types
- No infrastructure dependencies
- Full test coverage with Effect.runPromise

### 2.3 Domain Events

**Goal:** Define domain events for state changes

**Tasks:**
- [ ] Create event base class
  ```typescript
  export abstract class DomainEvent extends S.Class<DomainEvent>("DomainEvent")({
    id: S.String.pipe(S.uuid()),
    occurredAt: S.Date,
    aggregateId: S.String
  }) {}
  ```

- [ ] Create specific events
  ```typescript
  export class MessageSentEvent extends DomainEvent {
    readonly _tag = "MessageSentEvent";
    constructor(
      public readonly messageId: string,
      public readonly conversationId: string
    ) { super(); }
  }

  export class ConversationStartedEvent extends DomainEvent {
    readonly _tag = "ConversationStartedEvent";
    constructor(
      public readonly conversationId: string,
      public readonly agent: string
    ) { super(); }
  }

  export class StreamDeltaReceivedEvent extends DomainEvent {
    readonly _tag = "StreamDeltaReceivedEvent";
    constructor(
      public readonly delta: StreamDelta,
      public readonly conversationId: string
    ) { super(); }
  }
  ```

**Acceptance Criteria:**
- Events are immutable
- Events have discriminated unions (_tag)
- Events carry all necessary data
- Events use Schema for validation

---

## Phase 3: Infrastructure Layer Implementation

### 3.1 JSON-RPC Client with Effect-TS

**Goal:** Reimplement JSON-RPC client using Effect-TS primitives

**Tasks:**
- [ ] Define RPC service interface
  ```typescript
  import { Context, Effect, Layer, Stream } from "effect";

  export interface JsonRpcService {
    readonly request: <A, E>(
      method: string,
      params: unknown
    ) => Effect.Effect<A, RpcError | E>;
    
    readonly notifications: Stream.Stream<JsonRpcNotification, never>;
  }

  export const JsonRpcService = Context.GenericTag<JsonRpcService>("JsonRpcService");
  ```

- [ ] Implement WebSocket-based RPC client
  ```typescript
  export const JsonRpcServiceLive = Layer.effect(
    JsonRpcService,
    Effect.gen(function* () {
      const hub = yield* Hub.unbounded<JsonRpcNotification>();
      const requests = yield* Ref.make(new Map<string, Deferred<unknown>>());
      
      // WebSocket connection
      const ws = yield* Effect.acquireRelease(
        Effect.sync(() => new WebSocket("ws://localhost:8080/rpc")),
        (ws) => Effect.sync(() => ws.close())
      );

      // Handle incoming messages
      yield* Effect.async<void>((resume) => {
        ws.onmessage = (event) => {
          const message = JSON.parse(event.data);
          if ("id" in message) {
            // Response
            requests.get().then(map => {
              const deferred = map.get(message.id);
              if (deferred) {
                Deferred.complete(deferred, Effect.succeed(message.result));
              }
            });
          } else {
            // Notification
            Hub.publish(hub, message);
          }
        };
      }).pipe(Effect.fork);

      return {
        request: <A, E>(method: string, params: unknown) =>
          Effect.gen(function* () {
            const id = yield* Effect.sync(() => crypto.randomUUID());
            const deferred = yield* Deferred.make<A>();
            
            yield* Ref.update(requests, map => map.set(id, deferred));
            
            yield* Effect.sync(() => {
              ws.send(JSON.stringify({ jsonrpc: "2.0", id, method, params }));
            });
            
            return yield* Deferred.await(deferred).pipe(
              Effect.timeout(Duration.seconds(30)),
              Effect.catchTag("TimeoutException", () => 
                Effect.fail(new RpcTimeoutError({ method }))
              )
            );
          }),
        
        notifications: Stream.fromHub(hub)
      };
    })
  );
  ```

- [ ] Create RPC method wrappers
  ```typescript
  export const startThread = (): Effect.Effect<
    { threadId: string },
    RpcError,
    JsonRpcService
  > =>
    Effect.gen(function* () {
      const rpc = yield* JsonRpcService;
      return yield* rpc.request("thread/start", {});
    });

  export const sendMessage = (
    threadId: string,
    message: string
  ): Effect.Effect<void, RpcError, JsonRpcService> =>
    Effect.gen(function* () {
      const rpc = yield* JsonRpcService;
      yield* rpc.request("turn/start", { threadId, message });
    });
  ```

**Acceptance Criteria:**
- All RPC operations return Effect types
- Connection management uses Effect resource safety
- Notifications use Effect Stream
- Proper error handling with typed errors
- Automatic reconnection logic

### 3.2 No File Service Needed - Server Handles Attachments

**Server Already Supports Attachments:**

The Rust server already has a complete attachment system:
- `Attachment::parse_all()` - Parses `@[file/path]` syntax from messages
- `AttachmentService` - Resolves file paths and reads content
- `turn/start` accepts `files: Option<Vec<String>>` parameter

**Current Protocol** (`turn/start`):
```typescript
{
  thread_id: string,
  turn_id: string,
  message: string,
  files?: string[]  // ✓ Already supported!
}
```

**React App Approach:**
- User types message with `@[file/path]` syntax
- Client parses `@[...]` references (simple regex)
- Sends both `message` and `files` array to server
- Server handles file reading and attachment resolution
- No client-side file tagging/storage needed

**Implementation:**
```typescript
// Simple client-side parser (no Effect service needed)
export const parseFileReferences = (text: string): string[] => {
  const regex = /@\[([^\]]+)\]/g;
  const matches: string[] = [];
  let match;
  
  while ((match = regex.exec(text)) !== null) {
    matches.push(match[1]);
  }
  
  return matches;
};

// Use case becomes even simpler
export class SendMessageUseCase {
  static execute(
    threadId: string,
    content: string
  ): Effect.Effect<void, ApplicationError, JsonRpcService> {
    return Effect.gen(function* () {
      const validated = yield* MessageValidationService.validateMessage(content);
      const files = parseFileReferences(validated);
      
      const rpc = yield* JsonRpcService;
      yield* rpc.request("turn/start", {
        threadId,
        turnId: crypto.randomUUID(),
        message: validated,
        files: files.length > 0 ? files : undefined
      });
    });
  }
}
```

**Acceptance Criteria:**
- Client parses `@[file]` syntax with simple regex
- Sends file list with message
- Server resolves and reads files
- No client-side file management needed

**Note:** The VSCode extension's `FileContextManager` is unnecessary complexity. The server already does everything needed.

### 3.3 Server Process Manager

**Goal:** Adapt existing server manager to Effect-TS

**Tasks:**
- [ ] Define server service interface
  ```typescript
  export interface ServerService {
    readonly start: () => Effect.Effect<void, ServerError>;
    readonly stop: () => Effect.Effect<void, ServerError>;
    readonly restart: () => Effect.Effect<void, ServerError>;
    readonly status: Effect.Effect<ServerStatus, never>;
  }

  export const ServerService = Context.GenericTag<ServerService>("ServerService");
  ```

- [ ] Implement server manager with Effect
  ```typescript
  export const ServerServiceLive = Layer.scoped(
    ServerService,
    Effect.gen(function* () {
      const statusRef = yield* Ref.make<ServerStatus>("stopped");
      const processRef = yield* Ref.make<Option<ChildProcess>>(Option.none());

      const startServer = Effect.gen(function* () {
        const process = yield* Effect.tryPromise({
          try: () => spawn("forge-app-server", [], { stdio: "pipe" }),
          catch: (error) => new ServerError({ cause: error })
        });

        yield* Ref.set(processRef, Option.some(process));
        yield* Ref.set(statusRef, "running");

        // Handle process exit
        yield* Effect.async<void>((resume) => {
          process.on("exit", () => {
            Ref.set(statusRef, "stopped");
            resume(Effect.void);
          });
        }).pipe(Effect.fork);
      });

      const stopServer = Effect.gen(function* () {
        const process = yield* Ref.get(processRef);
        yield* pipe(
          process,
          Option.match({
            onNone: () => Effect.void,
            onSome: (proc) =>
              Effect.sync(() => {
                proc.kill("SIGTERM");
              })
          })
        );
        yield* Ref.set(statusRef, "stopped");
      });

      // Ensure cleanup on scope exit
      yield* Effect.addFinalizer(() => stopServer);

      return {
        start: () => startServer,
        stop: () => stopServer,
        restart: () => Effect.gen(function* () {
          yield* stopServer;
          yield* Effect.sleep(Duration.seconds(1));
          yield* startServer;
        }),
        status: Ref.get(statusRef)
      };
    })
  );
  ```

**Acceptance Criteria:**
- Server lifecycle managed with Effect scopes
- Automatic cleanup on scope exit
- Restart with proper delay
- Status tracking with Ref

---

## Phase 4: Application Layer Implementation

### 4.1 Use Cases / Application Services

**Goal:** Implement business workflows as use cases

**Tasks:**
- [ ] Create `SendMessageUseCase`
  ```typescript
  export class SendMessageUseCase {
    static execute(
      threadId: string,
      content: string
    ): Effect.Effect<void, ApplicationError, JsonRpcService> {
      return Effect.gen(function* () {
        // Validate message
        const validated = yield* MessageValidationService.validateMessage(content);
        
        // Parse file references from message (e.g., @[src/file.ts])
        const files = parseFileReferences(validated);
        
        // Send to server (server handles file reading and attachment resolution)
        const rpc = yield* JsonRpcService;
        yield* rpc.request("turn/start", {
          threadId,
          turnId: crypto.randomUUID(),
          message: validated,
          files: files.length > 0 ? files : undefined
        });
        
        // Server will send notifications for updates
        // No local persistence needed - server is source of truth
      });
    }
  }
  
  // Helper function (no Effect needed - pure function)
  function parseFileReferences(text: string): string[] {
    const regex = /@\[([^\]]+)\]/g;
    const matches: string[] = [];
    let match;
    
    while ((match = regex.exec(text)) !== null) {
      matches.push(match[1]);
    }
    
    return matches;
  }
  ```

- [ ] Create `StartConversationUseCase`
  ```typescript
  export class StartConversationUseCase {
    static execute(): Effect.Effect<string, ApplicationError, JsonRpcService> {
      return Effect.gen(function* () {
        const rpc = yield* JsonRpcService;
        
        // Start thread on server
        const { threadId } = yield* rpc.request<{ threadId: string }>("thread/start", {});
        
        // Return threadId - server handles persistence
        return threadId;
      });
    }
  }
  ```

- [ ] Create `LoadConversationsUseCase`
  ```typescript
  export class LoadConversationsUseCase {
    static execute(): Effect.Effect<
      ReadonlyArray<{ threadId: string; title?: string; updatedAt?: Date }>,
      ApplicationError,
      JsonRpcService
    > {
      return Effect.gen(function* () {
        const rpc = yield* JsonRpcService;
        
        // Fetch thread list from server (server has all data)
        const threads = yield* rpc.request<Array<{ threadId: string; title?: string; updatedAt?: string }>>(
          "thread/list",
          {}
        );
        
        // Parse dates and return
        return threads.map(t => ({
          ...t,
          updatedAt: t.updatedAt ? new Date(t.updatedAt) : undefined
        }));
      });
    }
  }
  ```

- [ ] Create `SubscribeToStreamUseCase`
  ```typescript
  export class SubscribeToStreamUseCase {
    static execute(
      conversationId: string,
      onDelta: (delta: StreamDelta) => void
    ): Effect.Effect<void, ApplicationError, JsonRpcService> {
      return Effect.gen(function* () {
        const rpc = yield* JsonRpcService;
        
        yield* Stream.runForEach(
          rpc.notifications.pipe(
            Stream.filter(notif => notif.method === "agentMessage/delta"),
            Stream.map(notif => new StreamDelta({
              type: "content",
              content: notif.params.delta,
              timestamp: new Date()
            }))
          ),
          (delta) => Effect.sync(() => onDelta(delta))
        ).pipe(Effect.fork);
      });
    }
  }
  ```

**Acceptance Criteria:**
- Each use case represents one business workflow
- Use cases orchestrate domain services and infrastructure
- All operations use Effect for composition
- Proper error handling with typed errors
- Use cases are testable with mock layers

### 4.2 Command Handlers

**Goal:** Implement CQRS command pattern

**Tasks:**
- [ ] Define command types
  ```typescript
  export type SendMessageCommand = Data.TaggedEnum<{
    SendMessage: { conversationId: string; content: string };
    ApproveChange: { approvalId: string; decision: "accept" | "reject" };
    ChangeModel: { conversationId: string; modelId: string };
  }>();
  ```

- [ ] Implement command handler
  ```typescript
  export class CommandHandler {
    static handle(
      command: SendMessageCommand
    ): Effect.Effect<void, ApplicationError, AppServices> {
      return command._tag === "SendMessage"
        ? SendMessageUseCase.execute(command.conversationId, command.content).pipe(
            Effect.asVoid
          )
        : command._tag === "ApproveChange"
        ? ApproveChangeUseCase.execute(command.approvalId, command.decision).pipe(
            Effect.asVoid
          )
        : ChangeModelUseCase.execute(command.conversationId, command.modelId).pipe(
            Effect.asVoid
          );
    }
  }
  ```

**Acceptance Criteria:**
- Commands are immutable data structures
- Handler uses pattern matching
- Commands map to use cases
- Full type safety

### 4.3 Query Handlers

**Goal:** Implement CQRS query pattern

**Tasks:**
- [ ] Define query types
  ```typescript
  export type AppQuery = Data.TaggedEnum<{
    GetConversation: { id: string };
    ListConversations: {};
    GetAgents: {};
    GetModels: { agentId: string };
  }>();
  ```

- [ ] Implement query handler
  ```typescript
  export class QueryHandler {
    static handle<A>(
      query: AppQuery
    ): Effect.Effect<A, ApplicationError, AppServices> {
      return query._tag === "GetConversation"
        ? GetConversationQuery.execute(query.id)
        : query._tag === "ListConversations"
        ? ListConversationsQuery.execute()
        : query._tag === "GetAgents"
        ? GetAgentsQuery.execute()
        : GetModelsQuery.execute(query.agentId);
    }
  }
  ```

**Acceptance Criteria:**
- Queries are immutable
- Queries don't modify state
- Queries use read-optimized paths
- Results are cached where appropriate

---

## Phase 5: State Management with Effect-TS

### 5.1 Application State Layer

**Goal:** Create centralized state management using Effect

**Tasks:**
- [ ] Define application state
  ```typescript
  export interface AppState {
    readonly conversations: ReadonlyArray<Conversation>;
    readonly activeConversationId: Option<string>;
    readonly streamingState: StreamingState;
    readonly ui: UIState;
  }

  export interface StreamingState {
    readonly isStreaming: boolean;
    readonly currentDelta: string;
  }

  export interface UIState {
    readonly sidebarOpen: boolean;
    readonly settingsOpen: boolean;
    readonly theme: "light" | "dark";
  }
  ```

- [ ] Create state service
  ```typescript
  export interface StateService {
    readonly state: Ref<AppState>;
    readonly subscribe: <A>(
      selector: (state: AppState) => A
    ) => Stream.Stream<A, never>;
    readonly dispatch: (action: StateAction) => Effect.Effect<void, never>;
  }

  export const StateService = Context.GenericTag<StateService>("StateService");
  ```

- [ ] Implement state service with Ref + Hub
  ```typescript
  export const StateServiceLive = Layer.effect(
    StateService,
    Effect.gen(function* () {
      const stateRef = yield* Ref.make<AppState>({
        conversations: [],
        activeConversationId: Option.none(),
        streamingState: { isStreaming: false, currentDelta: "" },
        ui: { sidebarOpen: true, settingsOpen: false, theme: "dark" }
      });

      const hub = yield* Hub.unbounded<AppState>();

      // Publish state changes to hub
      yield* Effect.forever(
        Ref.get(stateRef).pipe(
          Effect.flatMap(state => Hub.publish(hub, state)),
          Effect.delay(Duration.millis(16)) // 60fps
        )
      ).pipe(Effect.fork);

      return {
        state: stateRef,
        
        subscribe: <A>(selector: (state: AppState) => A) =>
          Stream.fromHub(hub).pipe(
            Stream.map(selector),
            Stream.changes
          ),
        
        dispatch: (action: StateAction) =>
          Ref.update(stateRef, state => reducer(state, action))
      };
    })
  );
  ```

- [ ] Create reducer function
  ```typescript
  export const reducer = (state: AppState, action: StateAction): AppState => {
    switch (action._tag) {
      case "ConversationAdded":
        return {
          ...state,
          conversations: [...state.conversations, action.conversation]
        };
      
      case "MessageAdded":
        return {
          ...state,
          conversations: state.conversations.map(conv =>
            conv.id.value === action.conversationId
              ? { ...conv, messages: [...conv.messages, action.message] }
              : conv
          )
        };
      
      case "StreamDeltaReceived":
        return {
          ...state,
          streamingState: {
            isStreaming: true,
            currentDelta: state.streamingState.currentDelta + action.delta
          }
        };
      
      case "StreamEnded":
        return {
          ...state,
          streamingState: { isStreaming: false, currentDelta: "" }
        };
      
      default:
        return state;
    }
  };
  ```

**Acceptance Criteria:**
- State is immutable (Ref provides controlled mutation)
- State changes are broadcasted via Hub
- Subscriptions use Stream for reactive updates
- Reducer is pure function
- State updates are type-safe

### 5.2 React Integration

**Goal:** Connect React components to Effect state

**Tasks:**
- [ ] Create Effect runtime context
  ```typescript
  export const EffectRuntimeContext = React.createContext<Runtime.Runtime<AppServices> | null>(null);

  export const EffectRuntimeProvider: React.FC<{ children: React.ReactNode }> = ({
    children
  }) => {
    const [runtime, setRuntime] = React.useState<Runtime.Runtime<AppServices> | null>(null);

    React.useEffect(() => {
      const layer = Layer.mergeAll(
        JsonRpcServiceLive,
        StateServiceLive
      );

      const runtime = Effect.runSync(Layer.toRuntime(layer));
      setRuntime(runtime);

      return () => {
        Runtime.dispose(runtime);
      };
    }, []);

    return (
      <EffectRuntimeContext.Provider value={runtime}>
        {runtime ? children : <div>Loading...</div>}
      </EffectRuntimeContext.Provider>
    );
  };
  ```

- [ ] Create `useEffect` hook
  ```typescript
  export const useEffectHook = <A, E>(
    effect: Effect.Effect<A, E, AppServices>,
    deps: React.DependencyList = []
  ): [Option<A>, Option<E>, boolean] => {
    const runtime = React.useContext(EffectRuntimeContext);
    const [data, setData] = React.useState<Option<A>>(Option.none());
    const [error, setError] = React.useState<Option<E>>(Option.none());
    const [loading, setLoading] = React.useState(true);

    React.useEffect(() => {
      if (!runtime) return;

      setLoading(true);
      
      const fiber = Effect.runFork(
        effect.pipe(
          Effect.tap(value => Effect.sync(() => setData(Option.some(value)))),
          Effect.tapError(err => Effect.sync(() => setError(Option.some(err)))),
          Effect.ensuring(Effect.sync(() => setLoading(false)))
        ),
        { runtime }
      );

      return () => {
        Fiber.interrupt(fiber);
      };
    }, deps);

    return [data, error, loading];
  };
  ```

- [ ] Create `useStateSelector` hook
  ```typescript
  export const useStateSelector = <A>(
    selector: (state: AppState) => A
  ): A => {
    const runtime = React.useContext(EffectRuntimeContext);
    const [value, setValue] = React.useState<A>(() => {
      // Initial value
      const state = Effect.runSync(
        StateService.pipe(
          Effect.flatMap(service => Ref.get(service.state))
        ),
        { runtime }
      );
      return selector(state);
    });

    React.useEffect(() => {
      if (!runtime) return;

      const fiber = Effect.runFork(
        StateService.pipe(
          Effect.flatMap(service =>
            Stream.runForEach(
              service.subscribe(selector),
              (newValue) => Effect.sync(() => setValue(newValue))
            )
          )
        ),
        { runtime }
      );

      return () => {
        Fiber.interrupt(fiber);
      };
    }, [selector]);

    return value;
  };
  ```

- [ ] Create `useDispatch` hook
  ```typescript
  export const useDispatch = () => {
    const runtime = React.useContext(EffectRuntimeContext);
    
    return React.useCallback(
      (action: StateAction) => {
        if (!runtime) return;
        
        Effect.runFork(
          StateService.pipe(
            Effect.flatMap(service => service.dispatch(action))
          ),
          { runtime }
        );
      },
      [runtime]
    );
  };
  ```

**Acceptance Criteria:**
- Runtime is created once and shared
- Hooks properly cleanup on unmount
- State updates trigger re-renders
- Effects are cancelable
- No memory leaks

---

## Phase 6: Presentation Layer Implementation

### 6.1 Component Architecture

**Goal:** Build React component tree

**Directory Structure:**
```
src/presentation/
├── components/
│   ├── chat/
│   │   ├── ChatContainer.tsx
│   │   ├── MessageList.tsx
│   │   ├── MessageItem.tsx
│   │   ├── InputBox.tsx
│   │   └── StreamingIndicator.tsx
│   ├── sidebar/
│   │   ├── Sidebar.tsx
│   │   ├── ConversationList.tsx
│   │   └── ConversationItem.tsx
│   ├── settings/
│   │   ├── SettingsPanel.tsx
│   │   ├── AgentSelector.tsx
│   │   └── ModelSelector.tsx
│   ├── common/
│   │   ├── Button.tsx
│   │   ├── Input.tsx
│   │   ├── Modal.tsx
│   │   └── Loading.tsx
│   └── layout/
│       ├── AppLayout.tsx
│       └── Header.tsx
├── hooks/
│   ├── useConversation.ts
│   ├── useMessages.ts
│   ├── useStreaming.ts
│   └── useSettings.ts
└── pages/
    ├── ChatPage.tsx
    └── SettingsPage.tsx
```

### 6.2 Core Components

**Tasks:**
- [ ] Implement `ChatContainer`
  ```typescript
  export const ChatContainer: React.FC = () => {
    const conversationId = useStateSelector(state =>
      state.activeConversationId.pipe(Option.getOrNull)
    );
    
    const messages = useStateSelector(state =>
      state.conversations
        .find(c => c.id.value === conversationId)
        ?.messages ?? []
    );
    
    const streamingState = useStateSelector(state => state.streamingState);
    const dispatch = useDispatch();
    
    const handleSendMessage = (content: string) => {
      if (!conversationId) return;
      
      const effect = SendMessageUseCase.execute(conversationId, content);
      
      Effect.runFork(effect, {
        onSuccess: (message) => {
          dispatch({ _tag: "MessageAdded", conversationId, message });
        },
        onFailure: (error) => {
          console.error("Failed to send message:", error);
        }
      });
    };
    
    return (
      <div className="flex flex-col h-full">
        <MessageList messages={messages} />
        {streamingState.isStreaming && (
          <StreamingIndicator delta={streamingState.currentDelta} />
        )}
        <InputBox onSend={handleSendMessage} disabled={streamingState.isStreaming} />
      </div>
    );
  };
  ```

- [ ] Implement `MessageList`
  ```typescript
  export const MessageList: React.FC<{ messages: ReadonlyArray<Message> }> = ({
    messages
  }) => {
    const scrollRef = React.useRef<HTMLDivElement>(null);
    
    React.useEffect(() => {
      scrollRef.current?.scrollIntoView({ behavior: "smooth" });
    }, [messages]);
    
    return (
      <div className="flex-1 overflow-y-auto p-4 space-y-4">
        {messages.map(message => (
          <MessageItem key={message.id.value} message={message} />
        ))}
        <div ref={scrollRef} />
      </div>
    );
  };
  ```

- [ ] Implement `Sidebar`
  ```typescript
  export const Sidebar: React.FC = () => {
    const conversations = useStateSelector(state => state.conversations);
    const activeId = useStateSelector(state => state.activeConversationId);
    const dispatch = useDispatch();
    
    const [_, error, loading] = useEffectHook(
      LoadConversationsUseCase.execute(),
      []
    );
    
    const handleNewConversation = () => {
      const effect = StartConversationUseCase.execute();
      
      Effect.runFork(effect, {
        onSuccess: (threadId) => {
          dispatch({ _tag: "ActiveConversationChanged", id: threadId });
        }
      });
    };
    
    if (loading) return <Loading />;
    if (error) return <ErrorDisplay error={error} />;
    
    return (
      <div className="w-64 bg-gray-900 h-full flex flex-col">
        <button onClick={handleNewConversation} className="m-4 btn-primary">
          New Conversation
        </button>
        <ConversationList
          conversations={conversations}
          activeId={activeId}
          onSelect={(id) => dispatch({ _tag: "ActiveConversationChanged", id })}
        />
      </div>
    );
  };
  ```

**Acceptance Criteria:**
- Components use Effect hooks
- Components are pure and functional
- Proper TypeScript types
- Accessibility features (ARIA labels, keyboard navigation)
- Responsive design

### 6.3 Custom Hooks

**Tasks:**
- [ ] Create `useConversation` hook
  ```typescript
  export const useConversation = (conversationId: string) => {
    const conversation = useStateSelector(state =>
      state.conversations.find(c => c.id.value === conversationId)
    );
    
    const sendMessage = (content: string) => {
      return Effect.runPromise(
        SendMessageUseCase.execute(conversationId, content)
      );
    };
    
    const loadHistory = () => {
      return Effect.runPromise(
        LoadConversationHistoryUseCase.execute(conversationId)
      );
    };
    
    return { conversation, sendMessage, loadHistory };
  };
  ```

- [ ] Create `useStreaming` hook
  ```typescript
  export const useStreaming = (conversationId: string) => {
    const streamingState = useStateSelector(state => state.streamingState);
    const dispatch = useDispatch();
    
    React.useEffect(() => {
      const effect = SubscribeToStreamUseCase.execute(
        conversationId,
        (delta) => {
          dispatch({ _tag: "StreamDeltaReceived", delta: delta.content });
        }
      );
      
      const fiber = Effect.runFork(effect);
      
      return () => {
        Fiber.interrupt(fiber);
      };
    }, [conversationId]);
    
    return streamingState;
  };
  ```

**Acceptance Criteria:**
- Hooks encapsulate business logic
- Hooks handle Effect lifecycle
- Hooks are reusable across components
- Proper cleanup on unmount

---

## Phase 7: Migration Strategy

### 7.1 Feature Parity Checklist

**Core Features:**
- [ ] Start new conversation
- [ ] Send message
- [ ] Receive streaming response
- [ ] Display conversation history
- [ ] Switch between conversations
- [ ] Delete conversation
- [ ] Tag files for context
- [ ] Approve file changes
- [ ] Approve command execution
- [ ] Change agent
- [ ] Change model
- [ ] View token usage
- [ ] View cost
- [ ] Settings panel
- [ ] Server connection management

### 7.2 Communication Protocol Migration

**Current:** JSON-RPC over stdio  
**Target:** JSON-RPC over WebSocket

**Tasks:**
- [ ] Update Rust server to support WebSocket
- [ ] Implement WebSocket adapter in infrastructure layer
- [ ] Add reconnection logic with exponential backoff
- [ ] Add heartbeat/ping-pong for connection health
- [ ] Maintain backward compatibility during transition

### 7.3 Data Migration

**Tasks:**
- [ ] Export existing VSCode workspace state
- [ ] Create migration script for conversations
- [ ] Import into IndexedDB
- [ ] Verify data integrity
- [ ] Create rollback plan

---

## Phase 8: Testing Strategy

### 8.1 Unit Tests with Effect-TS

**Tasks:**
- [ ] Test domain services with Effect.runPromise
  ```typescript
  describe("MessageValidationService", () => {
    it("should validate non-empty messages", async () => {
      const effect = MessageValidationService.validateMessage("Hello");
      const result = await Effect.runPromise(effect);
      expect(result).toBe("Hello");
    });
    
    it("should fail on empty messages", async () => {
      const effect = MessageValidationService.validateMessage("");
      await expect(Effect.runPromise(effect)).rejects.toThrow(ValidationError);
    });
  });
  ```

- [ ] Test use cases with mock layers
  ```typescript
  describe("SendMessageUseCase", () => {
    it("should send message and persist conversation", async () => {
      const mockStorage = Layer.succeed(StorageService, {
        get: () => Effect.succeed(Option.some(mockConversation)),
        set: vi.fn(() => Effect.void),
        remove: () => Effect.void,
        clear: () => Effect.void
      });
      
      const mockRpc = Layer.succeed(JsonRpcService, {
        request: vi.fn(() => Effect.succeed({})),
        notifications: Stream.empty
      });
      
      const effect = SendMessageUseCase.execute("conv-1", "Hello").pipe(
        Effect.provide(Layer.mergeAll(mockStorage, mockRpc))
      );
      
      const result = await Effect.runPromise(effect);
      expect(result.content).toBe("Hello");
    });
  });
  ```

### 8.2 Integration Tests

**Tasks:**
- [ ] Test RPC client with test server
- [ ] Test storage with test database
- [ ] Test state management with test runtime
- [ ] Test end-to-end workflows

### 8.3 Component Tests

**Tasks:**
- [ ] Test components with React Testing Library
- [ ] Test hooks with renderHook
- [ ] Test Effect integration
- [ ] Test accessibility

---

## Phase 9: Performance Optimization

### 9.1 Effect-TS Optimizations

**Tasks:**
- [ ] Use Effect.cached for expensive computations
- [ ] Use Effect.memoize for repeated effects
- [ ] Use Stream.buffer for backpressure
- [ ] Use Fiber pools for concurrent operations
- [ ] Profile Effect execution with Effect.timed

### 9.2 React Optimizations

**Tasks:**
- [ ] Use React.memo for pure components
- [ ] Use React.useMemo for expensive calculations
- [ ] Use React.useCallback for stable functions
- [ ] Implement virtual scrolling for message list
- [ ] Code-split routes with React.lazy

---

## Phase 10: Deployment

### 10.1 Build Configuration

**Tasks:**
- [ ] Configure Vite for production build
- [ ] Optimize bundle size
- [ ] Configure service worker for offline support
- [ ] Set up CDN for static assets
- [ ] Configure environment variables

### 10.2 Release Process

**Tasks:**
- [ ] Create release checklist
- [ ] Set up CI/CD pipeline
- [ ] Create deployment scripts
- [ ] Document release process
- [ ] Plan rollback strategy

---

## Success Metrics

### Technical Metrics
- [ ] Zero runtime errors in production
- [ ] < 3s initial load time
- [ ] < 100ms UI interaction latency
- [ ] 90%+ test coverage
- [ ] Zero memory leaks
- [ ] < 5MB bundle size (gzipped)

### Code Quality Metrics
- [ ] 100% TypeScript strict mode
- [ ] All Effects properly typed
- [ ] All components have proper types
- [ ] Zero ESLint errors
- [ ] Zero accessibility violations

### Feature Metrics
- [ ] 100% feature parity with VSCode extension
- [ ] All existing workflows supported
- [ ] No data loss during migration
- [ ] Backward compatible protocol

---

## Dependencies and Prerequisites

### Required Dependencies
```json
{
  "effect": "^3.9.0",
  "@effect/schema": "^0.75.0",
  "@effect/platform": "^0.68.0",
  "@effect/platform-browser": "^0.48.0",
  "react": "^18.3.0",
  "react-dom": "^18.3.0",
  "react-router-dom": "^6.26.0",
  "tailwindcss": "^3.4.0",
  "@headlessui/react": "^2.1.0",
  "@heroicons/react": "^2.1.0"
}
```

### Development Dependencies
```json
{
  "@types/react": "^18.3.0",
  "@types/react-dom": "^18.3.0",
  "@vitejs/plugin-react": "^4.3.0",
  "vite": "^5.4.0",
  "vitest": "^2.1.0",
  "@testing-library/react": "^16.0.0",
  "@testing-library/user-event": "^14.5.0",
  "typescript": "^5.6.0",
  "eslint": "^9.0.0"
}
```

---

## Risks and Mitigations

### Risk: Effect-TS Learning Curve
**Impact:** High  
**Probability:** High  
**Mitigation:**
- Dedicate time for Effect-TS training
- Start with simple use cases
- Create internal documentation
- Pair programming sessions

### Risk: Performance Issues with Streaming
**Impact:** High  
**Probability:** Medium  
**Mitigation:**
- Profile early and often
- Use Effect.Stream buffering
- Implement backpressure
- Add performance tests

### Risk: State Synchronization Issues
**Impact:** Medium  
**Probability:** Medium  
**Mitigation:**
- Use Effect.Ref for state
- Implement event sourcing pattern
- Add state validation
- Comprehensive integration tests

### Risk: Data Loss During Migration
**Impact:** High  
**Probability:** Low  
**Mitigation:**
- Implement export/import functionality
- Create comprehensive migration tests
- Keep VSCode extension running during transition
- Implement rollback mechanism

---

## Timeline Estimate

| Phase | Duration | Dependencies | Notes |
|-------|----------|--------------|-------|
| Phase 1: Foundation Setup | 1 week | None | |
| Phase 2: Domain Layer | 2 weeks | Phase 1 | |
| Phase 3: Infrastructure Layer | 1 week | Phase 2 | Simplified - only RPC client |
| Phase 4: Application Layer | 2 weeks | Phase 3 | |
| Phase 5: State Management | 2 weeks | Phase 4 | |
| Phase 6: Presentation Layer | 3 weeks | Phase 5 | |
| Phase 7: Migration | 1 week | Phase 6 | |
| Phase 8: Testing | 2 weeks | Phase 6 | |
| Phase 9: Optimization | 1 week | Phase 8 | |
| Phase 10: Deployment | 1 week | Phase 9 | |

**Total Estimated Duration:** 14 weeks (3.5 months)

**Savings:** 4 weeks saved by eliminating unnecessary storage/file infrastructure layers.

---

## Next Steps

1. **Review and approve this plan** with stakeholders
2. **Set up development environment** (Phase 1)
3. **Create project repository** with initial structure
4. **Schedule training sessions** on Effect-TS
5. **Begin Phase 1 implementation**

---

## References

- [Effect-TS Documentation](https://effect.website)
- [Effect-TS Schema](https://effect.website/docs/schema/introduction)
- [Effect-TS Platform](https://effect.website/docs/platform/introduction)
- [Onion Architecture](https://jeffreypalermo.com/2008/07/the-onion-architecture-part-1/)
- [CQRS Pattern](https://martinfowler.com/bliki/CQRS.html)
- [React Best Practices](https://react.dev/learn)

---

**Plan Status:** Ready for Review  
**Next Review Date:** 2025-12-06


---

## Architectural Clarifications (Updated)

### Why No Storage/File Services?

**Discovery:** After reviewing the Rust server code, we found that **all infrastructure already exists server-side**.

#### Server Already Has:
1. **Attachment System** (`crates/forge_domain/src/attachment.rs`)
   - `Attachment::parse_all()` - Parses `@[file/path]` syntax
   - `AttachmentService` trait - Resolves and reads files
   - `ForgeChatRequest` - Implementation that reads file content

2. **Protocol Support** (`crates/forge_app_server/src/protocol/request.rs`)
   ```rust
   TurnStart {
       thread_id: ThreadId,
       turn_id: TurnId,
       message: String,
       files: Option<Vec<String>>,  // ✓ Already supported!
   }
   ```

3. **Thread Persistence**
   - `thread/list` - Lists all conversations
   - `thread/get` - Retrieves conversation history
   - Server is the source of truth

#### What VSCode Extension Has (Unnecessary):
- `FileContextManager` - Client-side file tagging (192 lines)
- Workspace state persistence for tagged files
- File picker UI for selecting files
- **All of this is redundant** - server already handles everything

#### React App Simplification:
```typescript
// Before (over-engineered): FileService layer with Ref, Effect, Layer, etc.
// After (lean): Simple pure function
export const parseFileReferences = (text: string): string[] => {
  const regex = /@\[([^\]]+)\]/g;
  const matches: string[] = [];
  let match;
  while ((match = regex.exec(text)) !== null) {
    matches.push(match[1]);
  }
  return matches;
};

// Use case just extracts paths and sends to server
SendMessageUseCase.execute(threadId, message) // server does the rest
```

### Infrastructure Layer - Final Architecture

**Original Plan (Over-engineered):**
```
infrastructure/
├── rpc/              ❌ Too complex
├── storage/          ❌ Not needed - server has persistence
├── adapters/         ❌ Vague
└── browser/          ❌ Not needed - server reads files
```

**Final (Ultra-Lean):**
```
infrastructure/
└── rpc/              ✓ Only need WebSocket JSON-RPC client
```

That's it. One infrastructure service.

### Key Architectural Principles

1. **Server-First**: Rust backend is the single source of truth
2. **Thin Client**: React app sends commands, receives notifications
3. **No Duplication**: Don't reimplement what server already does
4. **Effect-TS for Side Effects Only**: RPC calls, state management, streams
5. **Pure Functions for Logic**: Parsing, validation, transformations

### Benefits of This Approach

✅ **Simpler codebase** - ~50% less code than original plan  
✅ **No sync issues** - Single source of truth (server)  
✅ **Fewer failure modes** - No client-side persistence bugs  
✅ **Easier testing** - Mock just the RPC layer  
✅ **Server improvements benefit all clients** - Web, CLI, VSCode extension

### Updated Timeline

Original: 18 weeks  
Revised: **14 weeks** (4 weeks saved by removing unnecessary infrastructure)

| Phase | Duration | Change |
|-------|----------|--------|
| Phase 1: Foundation | 1 week | Same |
| Phase 2: Domain Layer | 2 weeks | Same |
| Phase 3: Infrastructure | ~~3 weeks~~ **1 week** | -2 weeks (only RPC client) |
| Phase 4: Application Layer | 2 weeks | Same |
| Phase 5: State Management | 2 weeks | Same |
| Phase 6: Presentation | 3 weeks | Same |
| Phase 7: Migration | 1 week | Same |
| Phase 8: Testing | 2 weeks | Same |
| Phase 9: Optimization | 1 week | Same |
| Phase 10: Deployment | 1 week | Same |

**Total: 14 weeks (3.5 months)**
