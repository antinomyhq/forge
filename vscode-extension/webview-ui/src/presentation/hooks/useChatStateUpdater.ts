import { useCallback } from 'react';
import { Effect, Runtime } from 'effect';
import { ChatStateService } from '@/application/state/ChatStateService';
import { useRuntime } from './useRuntime';

/**
 * Hook for updating chat state from VSCode messages
 * Replaces the 280-line switch statement with Effect-based message handling
 */
export function useChatStateUpdater() {
  const runtime = useRuntime();
  
  const updateFromMessage = useCallback(
    (message: any) => {
      const program = Effect.gen(function* () {
        const chatState = yield* ChatStateService;
        
        // Only handle JSON-RPC format messages
        const method = message.method;
        const params = message.params || {};
        
        console.log('[useChatStateUpdater] Processing JSON-RPC method:', method, params);
        
        switch (method) {
          case 'state/update':
            console.log('[useChatStateUpdater] Updating state:', params);
            yield* chatState.updateHeader(params);
            break;
          
          case 'header/update':
            console.log('[useChatStateUpdater] Updating header:', params);
            yield* chatState.updateHeader(params);
            break;
          
          case 'models/list':
            console.log('[useChatStateUpdater] Received models:', params.models);
            console.log('[useChatStateUpdater] Models count:', params.models?.length ?? 0);
            yield* chatState.setModels(params.models);
            break;
          
          case 'agents/list':
            console.log('[useChatStateUpdater] Received agents:', params.agents);
            console.log('[useChatStateUpdater] Agents count:', params.agents?.length ?? 0);
            yield* chatState.setAgents(params.agents);
            break;
          
          case 'messages/set':
            yield* chatState.setMessages(params.messages);
            yield* chatState.setLoading(false);
            break;
          
          case 'stream/start':
            console.log('[useChatStateUpdater] Stream started');
            yield* chatState.updateStreaming('', true);
            yield* chatState.setLoading(false);
            if (params.threadId && params.turnId) {
              yield* chatState.setCurrentTurn(params.threadId, params.turnId);
            }
            break;
          
          case 'stream/delta':
            yield* chatState.updateStreaming(params.delta || '', true);
            break;
          
          case 'stream/end':
            console.log('[useChatStateUpdater] Stream ended');
            const state = yield* chatState.getState;
            if (state.streamingContent) {
              yield* chatState.addAssistantMessage(state.streamingContent);
            }
            yield* chatState.updateStreaming('', false);
            yield* chatState.clearCurrentTurn();
            break;
          
          case 'tool/callStart':
            console.log('[useChatStateUpdater] Tool call started:', params.tool);
            yield* chatState.addToolCall(
              params.callId || `tool-${Date.now()}`,
              params.tool,
              params.arguments
            );
            break;
          
          case 'tool/callEnd':
            console.log('[useChatStateUpdater] Tool call ended:', params.callId);
            yield* chatState.completeToolCall(
              params.callId,
              params.isError ? 'failed' : 'completed'
            );
            break;
          
          case 'tool/show':
            console.log('[useChatStateUpdater] Tool show:', params.tool);
            // Handle tool display
            break;
          
          case 'reasoning/show':
            console.log('[useChatStateUpdater] Reasoning:', params.text);
            if (params.text) {
              yield* chatState.addReasoning(params.text);
            }
            break;
          
          case 'approval/request':
            console.log('[useChatStateUpdater] Approval request:', params.approval);
            // Handle approval request
            break;
          
          case 'turn/started':
            console.log('[useChatStateUpdater] Turn started:', params.turnId);
            if (params.threadId && params.turnId) {
              yield* chatState.setCurrentTurn(params.threadId, params.turnId);
            }
            break;
          
          case 'turn/completed':
            console.log('[useChatStateUpdater] Turn completed:', params.turnId);
            yield* chatState.clearCurrentTurn();
            break;
          
          case 'chat/event':
            // Handle JSON-RPC chat/event notifications from LSP
            console.log('[useChatStateUpdater] Chat event:', params.event);
            break;
          
          case 'error':
            console.error('[useChatStateUpdater] Error from extension:', params.error);
            yield* chatState.setLoading(false);
            yield* chatState.updateStreaming('', false);
            break;
          
          default:
            console.log('[useChatStateUpdater] Unknown method:', method);
        }
      });
      
      Runtime.runPromise(runtime)(program).catch((error) => {
        console.error('[useChatStateUpdater] Error processing message:', error);
      });
    },
    [runtime]
  );
  
  return {
    updateFromMessage,
  };
}
