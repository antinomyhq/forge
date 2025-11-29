import { Effect, Context, Ref, Stream, Queue, Layer } from 'effect';

/**
 * Chat State - simplified for VSCode webview
 */
export interface ChatState {
  messages: Array<{ role: 'user' | 'assistant'; content: string; timestamp: number }>;
  models: Array<{ id: string; name?: string; label?: string; provider?: string; contextWindow?: number }>;
  agentName: string;
  modelName: string;
  tokenCount: string;
  cost: string;
  isStreaming: boolean;
  streamingContent: string;
}

/**
 * Chat State Service interface
 */
export interface ChatStateService {
  /**
   * Get current state
   */
  readonly getState: () => Effect.Effect<ChatState>;
  
  /**
   * Subscribe to state changes
   */
  readonly state$: Stream.Stream<ChatState>;
  
  /**
   * Update state
   */
  readonly updateState: (updater: (state: ChatState) => ChatState) => Effect.Effect<void>;
  
  /**
   * Add user message
   */
  readonly addUserMessage: (content: string) => Effect.Effect<void>;
  
  /**
   * Add assistant message
   */
  readonly addAssistantMessage: (content: string) => Effect.Effect<void>;
  
  /**
   * Update streaming content
   */
  readonly updateStreaming: (content: string, isStreaming: boolean) => Effect.Effect<void>;
  
  /**
   * Update header info
   */
  readonly updateHeader: (data?: any) => Effect.Effect<void>;
  
  /**
   * Set models list
   */
  readonly setModels: (models?: any) => Effect.Effect<void>;
}

export const ChatStateService = Context.GenericTag<ChatStateService>('ChatStateService');

/**
 * Initial state
 */
const initialState: ChatState = {
  messages: [],
  models: [],
  agentName: 'Forge',
  modelName: 'Claude 3.5 Sonnet',
  tokenCount: '0 / 200K tokens',
  cost: '$0.00',
  isStreaming: false,
  streamingContent: '',
};

/**
 * Live implementation of ChatStateService
 */
export const ChatStateServiceLive = Layer.effect(
  ChatStateService,
  Effect.gen(function* () {
    console.log('[ChatStateService] Initializing...');
    
    const stateRef = yield* Ref.make(initialState);
    const queue = yield* Queue.unbounded<ChatState>();
    
    const getState = () => Ref.get(stateRef);
    
    const updateState = (updater: (state: ChatState) => ChatState) =>
      Effect.gen(function* () {
        yield* Ref.update(stateRef, updater);
        const newState = yield* Ref.get(stateRef);
        yield* Queue.offer(queue, newState);
      });
    
    console.log('[ChatStateService] Initialized successfully');
    
    return ChatStateService.of({
      getState,
      state$: Stream.fromQueue(queue),
      updateState,
      
      addUserMessage: (content: string) =>
        updateState((state) => ({
          ...state,
          messages: [
            ...state.messages,
            { role: 'user' as const, content, timestamp: Date.now() },
          ],
        })),
      
      addAssistantMessage: (content: string) =>
        updateState((state) => ({
          ...state,
          messages: [
            ...state.messages,
            { role: 'assistant' as const, content, timestamp: Date.now() },
          ],
          isStreaming: false,
          streamingContent: '',
        })),
      
      updateStreaming: (content: string, isStreaming: boolean) =>
        updateState((state) => ({
          ...state,
          isStreaming,
          streamingContent: content,
        })),
      
      updateHeader: (data?: any) =>
        updateState((state) => ({
          ...state,
          agentName: data?.agent ?? state.agentName,
          modelName: data?.model ?? state.modelName,
          tokenCount: data?.tokens ?? state.tokenCount,
          cost: data?.cost ?? state.cost,
        })),
      
      setModels: (models?: any) =>
        updateState((state) => ({
          ...state,
          models: models ?? state.models,
        })),
    });
  })
);
