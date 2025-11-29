import { Effect, Context, Ref, Layer } from 'effect';

export interface ChatState {
  messages: Array<{
    role?: 'user' | 'assistant';
    content?: string;
    timestamp: number;
    type?: 'tool';
    toolName?: string;
    args?: Record<string, any>;
    status?: 'running' | 'completed' | 'failed';
  }>;
  models: Array<{ id: string; name?: string; label?: string; provider?: string; contextWindow?: number }>;
  agentName: string;
  modelName: string;
  tokenCount: string;
  cost: string;
  isLoading: boolean;  // NEW: Shows spinner before stream starts
  isStreaming: boolean;
  streamingContent: string;
  activeToolCalls: Map<string, { 
    id: string; 
    toolName: string; 
    timestamp: number;
    args?: Record<string, any>;
  }>;
  activeToolItemIds: Set<string>;  // NEW: Track item IDs of active tool calls to filter their deltas
  currentThreadId?: string;  // NEW: Track current thread ID
  currentTurnId?: string;    // NEW: Track current turn ID for cancellation
}

export interface ChatStateService {
  readonly getState: Effect.Effect<ChatState>;
  readonly setState: (updater: (state: ChatState) => ChatState) => Effect.Effect<void>;
  readonly addUserMessage: (content: string) => Effect.Effect<void>;
  readonly addAssistantMessage: (content: string) => Effect.Effect<void>;
  readonly updateStreaming: (content: string, isStreaming: boolean) => Effect.Effect<void>;
  readonly updateHeader: (data: any) => Effect.Effect<void>;
  readonly setModels: (models: any[]) => Effect.Effect<void>;
  readonly setMessages: (messages: any[]) => Effect.Effect<void>;
  readonly addToolCall: (id: string, toolName: string, args?: Record<string, any>) => Effect.Effect<void>;
  readonly completeToolCall: (id: string, status: 'completed' | 'failed') => Effect.Effect<void>;
  readonly setLoading: (isLoading: boolean) => Effect.Effect<void>;
  readonly setCurrentTurn: (threadId: string, turnId: string) => Effect.Effect<void>;  // NEW
  readonly clearCurrentTurn: () => Effect.Effect<void>;  // NEW
}

export const ChatStateService = Context.GenericTag<ChatStateService>('ChatStateService');

const initialState: ChatState = {
  messages: [],
  models: [],
  agentName: 'Forge',
  isLoading: false,
  modelName: 'Claude 3.5 Sonnet',
  tokenCount: '0 / 200K tokens',
  cost: '$0.00',
  isStreaming: false,
  streamingContent: '',
  activeToolCalls: new Map(),
  activeToolItemIds: new Set(),
};

export const ChatStateServiceLive = Layer.effect(
  ChatStateService,
  Effect.gen(function* () {
    const stateRef = yield* Ref.make(initialState);
    
    return ChatStateService.of({
      getState: Ref.get(stateRef),
      
      setState: (updater: (state: ChatState) => ChatState) =>
        Ref.update(stateRef, updater),
      
      addUserMessage: (content: string) =>
        Ref.update(stateRef, (state) => ({
          ...state,
          messages: [...state.messages, { role: 'user' as const, content, timestamp: Date.now() }],
        })),
      
      addAssistantMessage: (content: string) =>
        Ref.update(stateRef, (state) => ({
          ...state,
          messages: [...state.messages, { role: 'assistant' as const, content, timestamp: Date.now() }],
          isStreaming: false,
          streamingContent: '',
        })),
      
      updateStreaming: (content: string, isStreaming: boolean) =>
        Ref.update(stateRef, (state) => ({
          ...state,
          isLoading: false,  // Clear loading spinner when stream starts
          isStreaming,
          streamingContent: content,
        })),
      
      updateHeader: (data: any) =>
        Ref.update(stateRef, (state) => ({
          ...state,
          agentName: data?.agent ?? state.agentName,
          modelName: data?.model ?? state.modelName,
          tokenCount: data?.tokens ?? state.tokenCount,
          cost: data?.cost ?? state.cost,
        })),
      
      setModels: (models: any[]) =>
        Ref.update(stateRef, (state) => ({
          ...state,
          models: models || [],
        })),
      
      setMessages: (messages: any[]) =>
        Ref.update(stateRef, (state) => ({
          ...state,
          messages: messages || [],
        })),
      
      addToolCall: (id: string, toolName: string, args?: Record<string, any>) =>
        Ref.update(stateRef, (state) => {
          const newToolCalls = new Map(state.activeToolCalls);
          if (args) {
            newToolCalls.set(id, { id, toolName, timestamp: Date.now(), args });
          } else {
            newToolCalls.set(id, { id, toolName, timestamp: Date.now() });
          }
          
          const newToolItemIds = new Set(state.activeToolItemIds);
          newToolItemIds.add(id);  // Track this item ID to filter its deltas
          
          // Add tool call as a permanent message log
          const toolMessage: any = {
            role: 'assistant' as const,
            content: '',
            timestamp: Date.now(),
            type: 'tool' as const,
            toolName: toolName,
            status: 'running' as const
          };
          
          if (args) {
            toolMessage.args = args;
          }
          
          return {
            ...state,
            messages: [...state.messages, toolMessage],
            activeToolCalls: newToolCalls,
            activeToolItemIds: newToolItemIds,
          };
        }),
      
      completeToolCall: (id: string, status: 'completed' | 'failed') =>
        Ref.update(stateRef, (state) => {
          const toolInfo = state.activeToolCalls.get(id);
          if (!toolInfo) return state;
          
          const newToolCalls = new Map(state.activeToolCalls);
          newToolCalls.delete(id);
          
          const newToolItemIds = new Set(state.activeToolItemIds);
          newToolItemIds.delete(id);  // Remove from tracked item IDs
          
          // Update the tool message status in messages array
          const updatedMessages = state.messages.map((msg) => {
            if (msg.type === 'tool' && msg.toolName === toolInfo.toolName && msg.status === 'running') {
              // Found the running tool message, update its status
              return { ...msg, status };
            }
            return msg;
          });
          
          return {
            ...state,
            messages: updatedMessages,
            activeToolCalls: newToolCalls,
            activeToolItemIds: newToolItemIds,
          };
        }),
      
      setLoading: (isLoading: boolean) =>
        Ref.update(stateRef, (state) => ({
          ...state,
          isLoading,
        })),
      
      setCurrentTurn: (threadId: string, turnId: string) =>
        Ref.update(stateRef, (state) => ({
          ...state,
          currentThreadId: threadId,
          currentTurnId: turnId,
        })),
      
      clearCurrentTurn: () =>
        Ref.update(stateRef, (state) => {
          const { currentThreadId, currentTurnId, ...rest } = state;
          return rest as ChatState;
        }),
    });
  })
);
