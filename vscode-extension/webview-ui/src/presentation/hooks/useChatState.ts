import { useState, useEffect } from 'react';
import { Effect, Runtime, Stream, Fiber } from 'effect';
import { ChatStateService, type ChatState } from '@/application/state/ChatStateService';
import { useRuntime } from './useRuntime';

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
 * Hook to access chat state with stream-based reactivity
 * No more polling - uses Effect streams for reactive updates
 */
export function useChatState() {
  const runtime = useRuntime();
  const [state, setState] = useState<ChatState>(initialState);

  useEffect(() => {
    console.log('[useChatState] Setting up stream subscription...');
    
    const program = Effect.gen(function* () {
      const service = yield* ChatStateService;
      
      // Get initial state
      const initial = yield* service.getState;
      setState(initial);
      
      // Subscribe to state changes - no more polling!
      // This will only emit when state actually changes
      yield* Effect.forkDaemon(
        Stream.runForEach(service.state$, (newState) =>
          Effect.sync(() => {
            console.log('[useChatState] State updated via stream');
            setState(newState);
          })
        )
      );
    });

    const runningFiber = Runtime.runFork(runtime)(program);

    return () => {
      console.log('[useChatState] Cleaning up stream subscription...');
      Runtime.runPromise(runtime)(Fiber.interrupt(runningFiber)).catch(console.error);
    };
  }, [runtime]);

  return state;
}

/**
 * Hook to access just the messages (fine-grained subscription)
 * Only re-renders when messages change, not on other state changes
 */
export function useChatMessages() {
  const runtime = useRuntime();
  const [messages, setMessages] = useState<ChatState['messages']>([]);

  useEffect(() => {
    const program = Effect.gen(function* () {
      const service = yield* ChatStateService;
      const state = yield* service.getState;
      setMessages(state.messages);

      yield* Effect.forkDaemon(
        Stream.runForEach(service.messages$, (newMessages) =>
          Effect.sync(() => setMessages(newMessages))
        )
      );
    });

    const runningFiber = Runtime.runFork(runtime)(program);
    return () => {
      Runtime.runPromise(runtime)(Fiber.interrupt(runningFiber)).catch(console.error);
    };
  }, [runtime]);

  return messages;
}

/**
 * Hook to access streaming status (fine-grained subscription)
 * Only re-renders when streaming status changes
 */
export function useIsStreaming() {
  const runtime = useRuntime();
  const [isStreaming, setIsStreaming] = useState(false);

  useEffect(() => {
    const program = Effect.gen(function* () {
      const service = yield* ChatStateService;
      const state = yield* service.getState;
      setIsStreaming(state.isStreaming);

      yield* Effect.forkDaemon(
        Stream.runForEach(service.isStreaming$, (status) =>
          Effect.sync(() => setIsStreaming(status))
        )
      );
    });

    const runningFiber = Runtime.runFork(runtime)(program);
    return () => {
      Runtime.runPromise(runtime)(Fiber.interrupt(runningFiber)).catch(console.error);
    };
  }, [runtime]);

  return isStreaming;
}

/**
 * Hook to access loading status (fine-grained subscription)
 * Only re-renders when loading status changes
 */
export function useIsLoading() {
  const runtime = useRuntime();
  const [isLoading, setIsLoading] = useState(false);

  useEffect(() => {
    const program = Effect.gen(function* () {
      const service = yield* ChatStateService;
      const state = yield* service.getState;
      setIsLoading(state.isLoading);

      yield* Effect.forkDaemon(
        Stream.runForEach(service.isLoading$, (status) =>
          Effect.sync(() => setIsLoading(status))
        )
      );
    });

    const runningFiber = Runtime.runFork(runtime)(program);
    return () => {
      Runtime.runPromise(runtime)(Fiber.interrupt(runningFiber)).catch(console.error);
    };
  }, [runtime]);

  return isLoading;
}
