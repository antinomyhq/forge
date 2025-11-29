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
      
      console.log('[useChatActions] Sending message, setting loading=true');
      
      // Add user message to state
      yield* chatState.addUserMessage(text);
      
      // Show loading spinner (like Rust CLI spinner.start())
      yield* chatState.setLoading(true);
      
      console.log('[useChatActions] Loading state set, sending to extension');
      
      // Send to extension
      yield* vscode.sendMessage(text);
      
      console.log('[useChatActions] Message sent to extension');
    });
    
    Runtime.runPromise(runtime)(program).catch((error) => {
      console.error('[useChatActions] Error sending message:', error);
    });
  }, [runtime]);
  
  const changeModel = useCallback((modelId: string) => {
    const program = Effect.gen(function* () {
      const vscode = yield* VscodeRpcService;
      yield* vscode.changeModel(modelId);
    });
    
    Runtime.runPromise(runtime)(program).catch(console.error);
  }, [runtime]);
  
  const changeAgent = useCallback((agentId: string) => {
    const program = Effect.gen(function* () {
      const vscode = yield* VscodeRpcService;
      yield* vscode.changeAgent(agentId);
    });
    
    Runtime.runPromise(runtime)(program).catch(console.error);
  }, [runtime]);
  
  const initialize = useCallback(() => {
    const program = Effect.gen(function* () {
      const vscode = yield* VscodeRpcService;
      yield* vscode.sendReady;
      yield* vscode.requestModels;
      yield* vscode.requestAgents;
    });
    
    Runtime.runPromise(runtime)(program).catch(console.error);
  }, [runtime]);
  
  const cancelMessage = useCallback(() => {
    const program = Effect.gen(function* () {
      const chatState = yield* ChatStateService;
      const state = yield* chatState.getState;
      
      // Only cancel if we have an active turn
      if (state.currentThreadId && state.currentTurnId) {
        const vscode = yield* VscodeRpcService;
        yield* vscode.cancelTurn(state.currentThreadId, state.currentTurnId);
        
        // Clear the current turn
        yield* chatState.clearCurrentTurn();
        
        // Stop streaming state
        yield* chatState.updateStreaming('', false);
        yield* chatState.setLoading(false);
      }
    });
    
    Runtime.runPromise(runtime)(program).catch(console.error);
  }, [runtime]);
  
  return {
    sendMessage,
    changeModel,
    changeAgent,
    initialize,
    cancelMessage,
  };
}
