import { useState, useEffect, useCallback } from 'react';
import { Effect, Runtime } from 'effect';
import { ChatStateService, ChatState } from '@/application/state/ChatStateServiceSimple';
import { useRuntime } from './useRuntimeSimple';

/**
 * Hook to get and subscribe to chat state
 * Uses polling to update React state from Effect Ref
 */
export function useChatState(): ChatState {
  const runtime = useRuntime();
  const [state, setState] = useState<ChatState>({
    messages: [],
    models: [],
    agentName: 'Forge',
    modelName: 'Claude 3.5 Sonnet',
    tokenCount: '0 / 200K tokens',
    cost: '$0.00',
    isStreaming: false,
    streamingContent: '',
  });
  
  useEffect(() => {
    // Poll state every 100ms for updates
    const interval = setInterval(() => {
      const program = Effect.gen(function* () {
        const chatState = yield* ChatStateService;
        const currentState = yield* chatState.getState;
        return currentState;
      });
      
      Runtime.runPromise(runtime)(program)
        .then(setState)
        .catch(console.error);
    }, 100);
    
    return () => clearInterval(interval);
  }, [runtime]);
  
  return state;
}

/**
 * Hook to update chat state
 */
export function useChatStateUpdater() {
  const runtime = useRuntime();
  
  const updateFromMessage = useCallback((message: any) => {
    const program = Effect.gen(function* () {
      const chatState = yield* ChatStateService;
      
      switch (message.type) {
        case 'state':
          if (message.messages) yield* chatState.setMessages(message.messages);
          if (message.agent || message.model || message.tokens || message.cost) {
            yield* chatState.updateHeader({
              agent: message.agent,
              model: message.model,
              tokens: message.tokens,
              cost: message.cost,
            });
          }
          break;
        
        case 'streamStart':
          yield* chatState.updateStreaming('', true);
          break;
        
        case 'streamDelta':
          const state = yield* chatState.getState;
          yield* chatState.updateStreaming(state.streamingContent + message.delta, true);
          break;
        
        case 'streamEnd':
          const endState = yield* chatState.getState;
          if (endState.streamingContent) {
            yield* chatState.addAssistantMessage(endState.streamingContent);
          } else if (message.content) {
            yield* chatState.addAssistantMessage(message.content);
          }
          break;
        
        case 'updateHeader':
          yield* chatState.updateHeader(message.data);
          break;
        
        case 'modelsList':
          yield* chatState.setModels(message.models || []);
          break;
      }
    });
    
    Runtime.runPromise(runtime)(program).catch(console.error);
  }, [runtime]);
  
  return {
    updateFromMessage,
  };
}
