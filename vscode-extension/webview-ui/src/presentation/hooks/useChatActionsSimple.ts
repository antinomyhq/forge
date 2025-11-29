import { useCallback } from 'react';
import { Effect, Runtime } from 'effect';
import { VscodeRpcService } from '@/infrastructure/vscode/VscodeRpcServiceSimple';
import { ChatStateService } from '@/application/state/ChatStateServiceSimple';
import { useRuntime } from './useRuntimeSimple';

export function useChatActions() {
  const runtime = useRuntime();
  
  const sendMessage = useCallback((text: string) => {
    const program = Effect.gen(function* () {
      const vscode = yield* VscodeRpcService;
      const chatState = yield* ChatStateService;
      
      // Add user message to state
      yield* chatState.addUserMessage(text);
      
      // Show loading spinner (like Rust CLI spinner.start())
      yield* chatState.setLoading(true);
      
      // Send to extension
      yield* vscode.sendMessage(text);
    });
    
    Runtime.runPromise(runtime)(program).catch(console.error);
  }, [runtime]);
  
  const changeModel = useCallback((modelId: string) => {
    const program = Effect.gen(function* () {
      const vscode = yield* VscodeRpcService;
      yield* vscode.changeModel(modelId);
    });
    
    Runtime.runPromise(runtime)(program).catch(console.error);
  }, [runtime]);
  
  const initialize = useCallback(() => {
    const program = Effect.gen(function* () {
      const vscode = yield* VscodeRpcService;
      yield* vscode.sendReady;
      yield* vscode.requestModels;
    });
    
    Runtime.runPromise(runtime)(program).catch(console.error);
  }, [runtime]);
  
  return {
    sendMessage,
    changeModel,
    initialize,
  };
}
