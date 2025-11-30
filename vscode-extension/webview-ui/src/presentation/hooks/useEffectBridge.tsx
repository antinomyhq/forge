import { useState, useEffect, useCallback, useRef } from 'react';
import { Effect, Runtime, Stream, Fiber } from 'effect';
import { useRuntime } from './useRuntime';
import { VscodeRpcService } from '@/infrastructure/vscode/VscodeRpcService';
import { JsonRpcService } from '@/infrastructure/rpc/JsonRpcService';
import { ChatStateService } from '@/application/state/ChatStateService';

type AppServices = VscodeRpcService | JsonRpcService | ChatStateService;

/**
 * Hook to subscribe to an Effect Stream with proper lifecycle management
 * Automatically cancels subscription when component unmounts
 */
export function useEffectSubscription<A, E = never>(
  streamEffect: Effect.Effect<Stream.Stream<A, E, AppServices>, E, AppServices>,
  initialValue: A
): A {
  const runtime = useRuntime();
  const [value, setValue] = useState<A>(initialValue);
  const fiberRef = useRef<Fiber.RuntimeFiber<void, E> | null>(null);

  useEffect(() => {
    console.log('[useEffectSubscription] Starting subscription...');
    
    const program = Effect.gen(function* () {
      const stream = yield* streamEffect;
      yield* Stream.runForEach(stream, (item) => 
        Effect.sync(() => {
          console.log('[useEffectSubscription] Received update');
          setValue(item);
        })
      );
    });

    // Fork the subscription and store the fiber
    const fiber = Runtime.runFork(runtime)(program);
    fiberRef.current = fiber;

    // Cleanup: interrupt the fiber when component unmounts
    return () => {
      console.log('[useEffectSubscription] Cleaning up subscription...');
      if (fiberRef.current) {
        Runtime.runPromise(runtime)(Fiber.interrupt(fiberRef.current)).catch(console.error);
        fiberRef.current = null;
      }
    };
  }, [runtime, streamEffect]);

  return value;
}

/**
 * Hook to execute Effect callbacks with proper cancellation
 * Returns a callback that runs the Effect and a loading state
 */
export function useEffectCallback<A, E = never>(
  effectFn: (...args: any[]) => Effect.Effect<A, E, AppServices>,
  options: {
    onSuccess?: (result: A) => void;
    onError?: (error: E) => void;
  } = {}
): [(...args: any[]) => void, boolean] {
  const runtime = useRuntime();
  const [isLoading, setIsLoading] = useState(false);
  const fiberRef = useRef<Fiber.RuntimeFiber<A, E> | null>(null);

  const callback = useCallback((...args: any[]) => {
    setIsLoading(true);
    
    const effect = effectFn(...args);
    const fiber = Runtime.runFork(runtime)(effect);
    fiberRef.current = fiber;

    Runtime.runPromise(runtime)(Fiber.join(fiber))
      .then((result) => {
        setIsLoading(false);
        options.onSuccess?.(result);
      })
      .catch((error) => {
        setIsLoading(false);
        options.onError?.(error);
        console.error('[useEffectCallback] Error:', error);
      });
  }, [runtime, effectFn, options]);

  // Cleanup: interrupt any in-flight effect when component unmounts
  useEffect(() => {
    return () => {
      if (fiberRef.current) {
        Runtime.runPromise(runtime)(Fiber.interrupt(fiberRef.current)).catch(console.error);
      }
    };
  }, [runtime]);

  return [callback, isLoading];
}

/**
 * Hook to get a single Effect value with proper cancellation
 * Automatically re-fetches when dependencies change
 */
export function useEffectState<A, E = never>(
  effect: Effect.Effect<A, E, AppServices>,
  initialValue: A,
  deps: any[] = []
): [A, boolean, E | null] {
  const runtime = useRuntime();
  const [value, setValue] = useState<A>(initialValue);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<E | null>(null);
  const fiberRef = useRef<Fiber.RuntimeFiber<A, E> | null>(null);

  useEffect(() => {
    setIsLoading(true);
    setError(null);
    
    const fiber = Runtime.runFork(runtime)(effect);
    fiberRef.current = fiber;

    Runtime.runPromise(runtime)(Fiber.join(fiber))
      .then((result) => {
        setValue(result);
        setIsLoading(false);
      })
      .catch((err) => {
        setError(err);
        setIsLoading(false);
        console.error('[useEffectState] Error:', err);
      });

    return () => {
      if (fiberRef.current) {
        Runtime.runPromise(runtime)(Fiber.interrupt(fiberRef.current)).catch(console.error);
      }
    };
  }, [runtime, ...deps]);

  return [value, isLoading, error];
}
