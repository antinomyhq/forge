import { createContext, useContext, ReactNode, useEffect, useState } from 'react';
import { Layer, Runtime, Effect } from 'effect';
import { VscodeRpcService, VscodeRpcServiceLive } from '@/infrastructure/vscode/VscodeRpcService';
import { JsonRpcService, JsonRpcServiceLive } from '@/infrastructure/rpc/JsonRpcService';
import { ChatStateService, ChatStateServiceLive } from '@/application/state/ChatStateService';
import { getVscodeApi } from '@/infrastructure/vscode/VscodeApi';

type AppRuntime = Runtime.Runtime<VscodeRpcService | JsonRpcService | ChatStateService>;

const RuntimeContext = createContext<AppRuntime | null>(null);

// Acquire VSCode API once at module level to prevent StrictMode issues
// This ensures the API is acquired before any React component mounts
getVscodeApi();

export function EffectRuntimeProvider({ children }: { children: ReactNode }) {
  const [runtime, setRuntime] = useState<AppRuntime | null>(null);
  
  useEffect(() => {
    console.log('[Runtime] Creating with Stream-based services...');
    
    const AppLayer = Layer.mergeAll(
      JsonRpcServiceLive,
      VscodeRpcServiceLive,
      ChatStateServiceLive
    );
    
    // Build runtime and keep scope alive using runFork with scoped
    const fiber = Effect.runFork(
      Effect.scoped(
        Effect.gen(function* () {
          const runtime = yield* Layer.toRuntime(AppLayer);
          console.log('[Runtime] Created successfully with reactive streams');
          setRuntime(runtime as AppRuntime);
          // Yield forever to keep the scope alive
          yield* Effect.never;
        })
      )
    );
    
    // Cleanup function to dispose runtime when component unmounts
    return () => {
      console.log('[Runtime] Disposing runtime...');
      fiber.unsafePoll();
      setRuntime(null);
      console.log('[Runtime] Runtime disposed');
    };
  }, []);
  
  if (!runtime) {
    return <div className="loading">Initializing Effect runtime...</div>;
  }
  
  return <RuntimeContext.Provider value={runtime}>{children}</RuntimeContext.Provider>;
}

export function useRuntime(): AppRuntime {
  const runtime = useContext(RuntimeContext);
  if (!runtime) {
    throw new Error('useRuntime must be used within EffectRuntimeProvider');
  }
  return runtime;
}
