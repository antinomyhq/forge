import { Effect, Context, Ref, Layer } from 'effect';

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

export interface ChatStateService {
  readonly getState: Effect.Effect<ChatState>;
  readonly setState: (updater: (state: ChatState) => ChatState) => Effect.Effect<void>;
  readonly addUserMessage: (content: string) => Effect.Effect<void>;
  readonly addAssistantMessage: (content: string) => Effect.Effect<void>;
  readonly updateStreaming: (content: string, isStreaming: boolean) => Effect.Effect<void>;
  readonly updateHeader: (data: any) => Effect.Effect<void>;
  readonly setModels: (models: any[]) => Effect.Effect<void>;
  readonly setMessages: (messages: any[]) => Effect.Effect<void>;
}

export const ChatStateService = Context.GenericTag<ChatStateService>('ChatStateService');

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
    });
  })
);
