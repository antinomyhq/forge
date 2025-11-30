import { useCallback } from 'react';
import { Effect } from 'effect';
import { VscodeRpcService } from '@/infrastructure/vscode/VscodeRpcService';
import { ChatStateService } from '@/application/state/ChatStateService';
import { useEffectCallback } from './useEffectBridge';

/**
 * Hook for chat actions using Effect-TS with proper cancellation
 */
export function useChatActions() {
  const sendMessage = useEffectCallback((text: string) =>
    Effect.gen(function* () {
      const vscode = yield* VscodeRpcService;
      const chatState = yield* ChatStateService;
      
      console.log('[useChatActions] Sending message, setting loading=true');
      
      // Add user message to state
      yield* chatState.addUserMessage(text);
      
      // Show loading spinner
      yield* chatState.setLoading(true);
      
      console.log('[useChatActions] Loading state set, sending to extension');
      
      // Send to extension
      yield* vscode.sendMessage(text);
      
      console.log('[useChatActions] Message sent to extension');
    })
  );
  
  const changeModel = useEffectCallback((modelId: string) =>
    Effect.gen(function* () {
      const vscode = yield* VscodeRpcService;
      yield* vscode.changeModel(modelId);
    })
  );
  
  const changeAgent = useEffectCallback((agentId: string) =>
    Effect.gen(function* () {
      const vscode = yield* VscodeRpcService;
      yield* vscode.changeAgent(agentId);
    })
  );
  
  const initialize = useEffectCallback(() =>
    Effect.gen(function* () {
      const vscode = yield* VscodeRpcService;
      yield* vscode.sendReady;
      yield* vscode.requestModels;
      yield* vscode.requestAgents;
    })
  );
  
  const cancelMessage = useEffectCallback(() =>
    Effect.gen(function* () {
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
    })
  );
  
  const [sendMessageFn] = sendMessage;
  const [changeModelFn] = changeModel;
  const [changeAgentFn] = changeAgent;
  const [initializeFn] = initialize;
  const [cancelMessageFn] = cancelMessage;
  
  return {
    sendMessage: useCallback((text: string) => sendMessageFn(text), [sendMessageFn]),
    changeModel: useCallback((modelId: string) => changeModelFn(modelId), [changeModelFn]),
    changeAgent: useCallback((agentId: string) => changeAgentFn(agentId), [changeAgentFn]),
    initialize: useCallback(() => initializeFn(), [initializeFn]),
    cancelMessage: useCallback(() => cancelMessageFn(), [cancelMessageFn]),
  };
}
