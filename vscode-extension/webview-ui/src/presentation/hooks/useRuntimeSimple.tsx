import { createContext, useContext, ReactNode, useEffect, useState } from 'react';
import { Layer, Runtime, Effect } from 'effect';
import { VscodeRpcService, VscodeRpcServiceLive } from '@/infrastructure/vscode/VscodeRpcServiceSimple';
import { ChatStateService, ChatStateServiceLive } from '@/application/state/ChatStateServiceSimple';

type AppRuntime = Runtime.Runtime<VscodeRpcService | ChatStateService>;

const RuntimeContext = createContext<AppRuntime | null>(null);

export function EffectRuntimeProvider({ children }: { children: ReactNode }) {
  const [runtime, setRuntime] = useState<AppRuntime | null>(null);
  
  useEffect(() => {
    console.log('[Runtime] Creating...');
    
    const AppLayer = Layer.mergeAll(
      VscodeRpcServiceLive,
      ChatStateServiceLive
    );
    
    const program = Layer.toRuntime(AppLayer);
    
    Effect.runPromise(Effect.scoped(program)).then((rt) => {
      console.log('[Runtime] Created successfully');
      setRuntime(rt as AppRuntime);
    }).catch((error) => {
      console.error('[Runtime] Failed to create:', error);
    });
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
