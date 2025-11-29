import { createContext, useContext, ReactNode, useEffect, useState } from 'react';
import { Effect, Layer, Runtime } from 'effect';
import { VscodeRpcService, VscodeRpcServiceLive } from '@/infrastructure/vscode/VscodeRpcService';
import { ChatStateService, ChatStateServiceLive } from '@/application/state/ChatStateService';
import { handleIncomingMessages, initializeChat } from '@/application/services/MessageHandlerService';

/**
 * Runtime context for Effect-TS
 */
const RuntimeContext = createContext<Runtime.Runtime<VscodeRpcService | ChatStateService> | null>(null);

/**
 * Provider component that creates and provides the Effect runtime
 */
export function EffectRuntimeProvider({ children }: { children: ReactNode }) {
  const [runtime, setRuntime] = useState<Runtime.Runtime<VscodeRpcService | ChatStateService> | null>(null);
  const [error, setError] = useState<Error | null>(null);
  
  useEffect(() => {
    console.log('[EffectRuntimeProvider] Starting initialization...');
    
    // Create the application layer with both services
    const AppLayer = Layer.mergeAll(
      VscodeRpcServiceLive,
      ChatStateServiceLive
    );
    
    // Create runtime from layer
    const program = Layer.toRuntime(AppLayer);
    
    // Run the program to get the runtime
    Effect.runPromise(Effect.scoped(program)).then((rt) => {
      console.log('[EffectRuntimeProvider] Runtime created successfully');
      setRuntime(rt as Runtime.Runtime<VscodeRpcService | ChatStateService>);
      
      // Initialize chat and start message handler
      const initProgram = Effect.gen(function* () {
        console.log('[EffectRuntimeProvider] Initializing chat...');
        yield* initializeChat;
        console.log('[EffectRuntimeProvider] Starting message handler...');
        yield* Effect.fork(handleIncomingMessages);
        console.log('[EffectRuntimeProvider] All initialization complete');
      });
      
      Runtime.runPromise(rt)(initProgram).catch((err) => {
        console.error('[EffectRuntimeProvider] Init error:', err);
        setError(err);
      });
    }).catch((err) => {
      console.error('[EffectRuntimeProvider] Runtime creation error:', err);
      setError(err);
    });
  }, []);
  
  if (error) {
    return <div className="error">Failed to initialize: {error.message}</div>;
  }
  
  if (!runtime) {
    return <div className="loading">Loading...</div>;
  }
  
  return <RuntimeContext.Provider value={runtime}>{children}</RuntimeContext.Provider>;
}

/**
 * Hook to access the Effect runtime
 */
export function useRuntime() {
  const runtime = useContext(RuntimeContext);
  if (!runtime) {
    throw new Error('useRuntime must be used within EffectRuntimeProvider');
  }
  return runtime;
}
