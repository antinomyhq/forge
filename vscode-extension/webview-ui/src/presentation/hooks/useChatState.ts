import { useState, useEffect } from 'react';
import { Effect, Stream, Runtime, Fiber } from 'effect';
import { ChatStateService, ChatState } from '@/application/state/ChatStateService';
import { useRuntime } from './useRuntime';

/**
 * Hook to subscribe to chat state
 */
export function useChatState(): ChatState | null {
  const runtime = useRuntime();
  const [state, setState] = useState<ChatState | null>(null);
  
  useEffect(() => {
    const program = Effect.gen(function* () {
      const chatState = yield* ChatStateService;
      
      // Get initial state
      const initial = yield* chatState.getState();
      setState(initial);
      
      // Subscribe to changes
      yield* Stream.runForEach(chatState.state$, (newState) =>
        Effect.sync(() => setState(newState))
      );
    });
    
    const fiber = Runtime.runFork(runtime)(program);
    
    return () => {
      Runtime.runSync(runtime)(Fiber.interrupt(fiber));
    };
  }, [runtime]);
  
  return state;
}

/**
 * Hook to get state selector
 */
export function useChatStateSelector<T>(selector: (state: ChatState) => T): T | null {
  const state = useChatState();
  return state ? selector(state) : null;
}
