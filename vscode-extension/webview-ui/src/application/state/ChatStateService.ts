import { Effect, Context, SubscriptionRef, Stream, Layer } from 'effect';

export interface ChatState {
  messages: Array<{
    role?: 'user' | 'assistant';
    content?: string;
    timestamp: number;
    type?: 'tool' | 'reasoning';
    toolName?: string;
    args?: Record<string, any>;
    status?: 'running' | 'completed' | 'failed';
    reasoning?: string;
  }>;
  models: Array<{ id: string; name?: string; label?: string; provider?: string; contextWindow?: number }>;
  agents: Array<{ id: string; name?: string; description?: string; provider?: string; model?: string; capabilities?: string[] }>;
  agentName: string;
  agentId: string;
  modelName: string;
  modelId: string;
  tokenCount: string;
  cost: string;
  isLoading: boolean;
  isStreaming: boolean;
  streamingContent: string;
  activeToolCalls: Map<string, { 
    id: string; 
    toolName: string; 
    timestamp: number;
    args?: Record<string, any>;
  }>;
  activeToolItemIds: Set<string>;
  currentThreadId?: string;
  currentTurnId?: string;
}

export interface ChatStateService {
  readonly getState: Effect.Effect<ChatState>;
  readonly state$: Stream.Stream<ChatState>; // Full state stream
  readonly messages$: Stream.Stream<ChatState['messages']>; // Fine-grained: messages only
  readonly isStreaming$: Stream.Stream<boolean>; // Fine-grained: streaming status only
  readonly isLoading$: Stream.Stream<boolean>; // Fine-grained: loading status only
  readonly setState: (updater: (state: ChatState) => ChatState) => Effect.Effect<void>;
  readonly addUserMessage: (content: string) => Effect.Effect<void>;
  readonly addAssistantMessage: (content: string) => Effect.Effect<void>;
  readonly addReasoning: (content: string) => Effect.Effect<void>;
  readonly updateStreaming: (content: string, isStreaming: boolean) => Effect.Effect<void>;
  readonly updateHeader: (data: any) => Effect.Effect<void>;
  readonly setModels: (models: any[]) => Effect.Effect<void>;
  readonly setAgents: (agents: any[]) => Effect.Effect<void>;
  readonly setMessages: (messages: any[]) => Effect.Effect<void>;
  readonly addToolCall: (id: string, toolName: string, args?: Record<string, any>) => Effect.Effect<void>;
  readonly completeToolCall: (id: string, status: 'completed' | 'failed') => Effect.Effect<void>;
  readonly setLoading: (isLoading: boolean) => Effect.Effect<void>;
  readonly setCurrentTurn: (threadId: string, turnId: string) => Effect.Effect<void>;
  readonly clearCurrentTurn: () => Effect.Effect<void>;
}

export const ChatStateService = Context.GenericTag<ChatStateService>('ChatStateService');

const initialState: ChatState = {
  messages: [],
  models: [],
  agents: [],
  agentName: 'Forge',
  agentId: 'forge',
  isLoading: false,
  modelName: '',
  modelId: '',
  tokenCount: '0 / 200K tokens',
  cost: '$0.00',
  isStreaming: false,
  streamingContent: '',
  activeToolCalls: new Map(),
  activeToolItemIds: new Set(),
};

/**
 * Live implementation using SubscriptionRef for fine-grained reactive updates
 * SubscriptionRef automatically notifies subscribers on changes
 */
export const ChatStateServiceLive = Layer.effect(
  ChatStateService,
  Effect.gen(function* () {
    console.log('[ChatStateService] Initializing with SubscriptionRef for fine-grained reactivity...');
    
    // Use SubscriptionRef instead of regular Ref
    const stateRef = yield* SubscriptionRef.make(initialState);
    
    const getState = SubscriptionRef.get(stateRef);
    
    // Helper to update state - SubscriptionRef automatically notifies
    const updateState = (updater: (state: ChatState) => ChatState) =>
      SubscriptionRef.update(stateRef, updater);
    
    // Full state stream
    const state$ = stateRef.changes;
    
    // Fine-grained streams - only emit when specific properties change
    // Using Stream.changes to deduplicate consecutive identical values
    const messages$ = Stream.map(state$, (state: ChatState) => state.messages).pipe(
      Stream.changes
    );
    
    const isStreaming$ = Stream.map(state$, (state: ChatState) => state.isStreaming).pipe(
      Stream.changes
    );
    
    const isLoading$ = Stream.map(state$, (state: ChatState) => state.isLoading).pipe(
      Stream.changes
    );
    
    console.log('[ChatStateService] Initialized with SubscriptionRef - fine-grained reactivity enabled');
    
    return ChatStateService.of({
      getState,
      state$,
      messages$,
      isStreaming$,
      isLoading$,
      
      setState: updateState,
      
      addUserMessage: (content: string) =>
        updateState((state) => ({
          ...state,
          messages: [...state.messages, { role: 'user' as const, content, timestamp: Date.now() }],
        })),
      
      addAssistantMessage: (content: string) =>
        updateState((state) => ({
          ...state,
          messages: [...state.messages, { role: 'assistant' as const, content, timestamp: Date.now() }],
          isStreaming: false,
          streamingContent: '',
        })),
      
      addReasoning: (content: string) =>
        updateState((state) => ({
          ...state,
          messages: [...state.messages, { 
            type: 'reasoning' as const, 
            reasoning: content, 
            timestamp: Date.now() 
          }],
        })),
      
      updateStreaming: (content: string, isStreaming: boolean) =>
        updateState((state) => ({
          ...state,
          isLoading: false,
          isStreaming,
          streamingContent: content,
        })),
      
      updateHeader: (data?: any) =>
        updateState((state) => ({
          ...state,
          agentName: data?.agent ?? state.agentName,
          agentId: data?.agentId ?? data?.agent_id ?? state.agentId,
          modelName: data?.model ?? state.modelName,
          modelId: data?.modelId ?? data?.model_id ?? state.modelId,
          tokenCount: data?.tokens ?? state.tokenCount,
          cost: data?.cost ?? state.cost,
        })),
      
      setModels: (models?: any) =>
        Effect.gen(function* () {
          console.log('[ChatStateService] setModels called with:', models);
          console.log('[ChatStateService] Models is array?', Array.isArray(models));
          console.log('[ChatStateService] Models length:', models?.length ?? 'N/A');
          yield* updateState((state) => ({
            ...state,
            models: models ?? state.models,
          }));
          const newState = yield* getState;
          console.log('[ChatStateService] State after setModels, models count:', newState.models.length);
        }),
      
      setAgents: (agents?: any) =>
        Effect.gen(function* () {
          console.log('[ChatStateService] setAgents called with:', agents);
          console.log('[ChatStateService] Agents is array?', Array.isArray(agents));
          console.log('[ChatStateService] Agents length:', agents?.length ?? 'N/A');
          yield* updateState((state) => ({
            ...state,
            agents: agents ?? state.agents,
          }));
          const newState = yield* getState;
          console.log('[ChatStateService] State after setAgents, agents count:', newState.agents.length);
        }),
      
      setMessages: (messages?: any) =>
        updateState((state) => ({
          ...state,
          messages: messages ?? state.messages,
        })),
      
      addToolCall: (id: string, toolName: string, args?: Record<string, any>) =>
        updateState((state) => {
          const newToolCalls = new Map(state.activeToolCalls);
          newToolCalls.set(id, { id, toolName, timestamp: Date.now(), ...(args && { args }) });
          return {
            ...state,
            activeToolCalls: newToolCalls,
            messages: [
              ...state.messages,
              {
                type: 'tool' as const,
                toolName,
                ...(args && { args }),
                status: 'running' as const,
                timestamp: Date.now(),
              },
            ],
          };
        }),
      
      completeToolCall: (id: string, status: 'completed' | 'failed') =>
        updateState((state) => {
          const newToolCalls = new Map(state.activeToolCalls);
          newToolCalls.delete(id);
          const newToolItemIds = new Set(state.activeToolItemIds);
          newToolItemIds.delete(id);
          
          return {
            ...state,
            activeToolCalls: newToolCalls,
            activeToolItemIds: newToolItemIds,
            messages: state.messages.map((msg) =>
              msg.type === 'tool' && msg.timestamp === state.activeToolCalls.get(id)?.timestamp
                ? { ...msg, status }
                : msg
            ),
          };
        }),
      
      setLoading: (isLoading: boolean) =>
        updateState((state) => ({
          ...state,
          isLoading,
        })),
      
      setCurrentTurn: (threadId: string, turnId: string) =>
        updateState((state) => ({
          ...state,
          currentThreadId: threadId,
          currentTurnId: turnId,
        })),
      
      clearCurrentTurn: () =>
        updateState((state) => {
          const { currentThreadId, currentTurnId, ...rest } = state;
          return rest as ChatState;
        }),
    });
  })
);
