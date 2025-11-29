import { useCallback } from 'react';
import { Effect, Runtime } from 'effect';
import { VscodeRpcService } from '@/infrastructure/vscode/VscodeRpcService';
import { ChatStateService } from '@/application/state/ChatStateService';
import { useRuntime } from './useRuntime';

/**
 * Hook to get chat actions
 */
export function useChatActions() {
  const runtime = useRuntime();
  
  const sendMessage = useCallback(
    (text: string) => {
      const program = Effect.gen(function* () {
        const vscode = yield* VscodeRpcService;
        const chatState = yield* ChatStateService;
        
        // Add user message to state
        yield* chatState.addUserMessage(text);
        
        // Send to extension
        yield* vscode.sendChatMessage(text);
      });
      
      Runtime.runPromise(runtime)(program).catch(console.error);
    },
    [runtime]
  );
  
  const changeModel = useCallback(
    (modelId: string) => {
      const program = Effect.gen(function* () {
        const vscode = yield* VscodeRpcService;
        yield* vscode.changeModel(modelId);
      });
      
      Runtime.runPromise(runtime)(program).catch(console.error);
    },
    [runtime]
  );
  
  return {
    sendMessage,
    changeModel,
  };
}
