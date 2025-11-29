import { useState, useEffect, useCallback } from 'react';
import { Effect, Runtime } from 'effect';
import { ChatStateService, ChatState } from '@/application/state/ChatStateServiceSimple';
import { useRuntime } from './useRuntimeSimple';

/**
 * Hook to get and subscribe to chat state
 * Uses polling to update React state from Effect Ref
 */
export function useChatState(): ChatState {
  const runtime = useRuntime();
  const [state, setState] = useState<ChatState>({
    messages: [],
    models: [],
    agentName: 'Forge',
    modelName: 'Claude 3.5 Sonnet',
    tokenCount: '0 / 200K tokens',
    cost: '$0.00',
    isLoading: false,
    isStreaming: false,
    streamingContent: '',
    activeToolCalls: new Map(),
    activeToolItemIds: new Set(),
  });
  
  useEffect(() => {
    // Poll state every 100ms for updates
    const interval = setInterval(() => {
      const program = Effect.gen(function* () {
        const chatState = yield* ChatStateService;
        const currentState = yield* chatState.getState;
        return currentState;
      });
      
      Runtime.runPromise(runtime)(program)
        .then((newState) => {
          // Log state changes for debugging
          if (newState.isLoading !== state.isLoading || newState.isStreaming !== state.isStreaming) {
            console.log('[useChatState] State changed:', {
              isLoading: newState.isLoading,
              isStreaming: newState.isStreaming,
              currentTurnId: newState.currentTurnId,
            });
          }
          setState(newState);
        })
        .catch(console.error);
    }, 100);
    
    return () => clearInterval(interval);
  }, [runtime, state.isLoading, state.isStreaming]);
  
  return state;
}

/**
 * Hook to update chat state
 */
export function useChatStateUpdater() {
  const runtime = useRuntime();
  
  const updateFromMessage = useCallback((message: any) => {
    console.log('[useChatStateUpdater] ====== MESSAGE RECEIVED ======');
    console.log('[useChatStateUpdater] Type:', message.type);
    console.log('[useChatStateUpdater] Full message:', JSON.stringify(message, null, 2));
    
    const program = Effect.gen(function* () {
      const chatState = yield* ChatStateService;
      const stateBefore = yield* chatState.getState;
      console.log('[useChatStateUpdater] State before:', {
        messagesCount: stateBefore.messages.length,
        isStreaming: stateBefore.isStreaming,
        streamingContent: stateBefore.streamingContent?.substring(0, 50) + '...'
      });
      
      switch (message.type) {
        case 'state':
          if (message.messages) yield* chatState.setMessages(message.messages);
          if (message.agent || message.model || message.tokens || message.cost) {
            yield* chatState.updateHeader({
              agent: message.agent,
              model: message.model,
              tokens: message.tokens,
              cost: message.cost,
            });
          }
          break;
        
        case 'turn/started':
          // Store the turn ID when a turn starts
          yield* chatState.setCurrentTurn(message.threadId, message.turnId);
          break;
        
        case 'streamStart':
          yield* chatState.updateStreaming('', true);
          break;
        
        case 'streamDelta':
          const state = yield* chatState.getState;
          const delta = message.delta || '';
          const itemId = message.itemId;
          
          console.log('[useChatStateUpdater] streamDelta received:', {
            delta: delta.substring(0, 50),
            itemId,
            activeToolItemIds: Array.from(state.activeToolItemIds),
            willFilter: itemId && state.activeToolItemIds.has(itemId)
          });
          
          // Filter out deltas from tool call items (like "Execute [/bin/zsh]")
          if (itemId && state.activeToolItemIds.has(itemId)) {
            console.log('[useChatStateUpdater] ✓ Filtering delta from tool item:', itemId);
            break;  // Skip this delta - it's tool commentary
          }
          
          console.log('[useChatStateUpdater] ✓ Appending delta to streaming content');
          if (delta) {
            yield* chatState.updateStreaming(state.streamingContent + delta, true);
          }
          break;
        
        case 'streamEnd':
          console.log('[useChatStateUpdater] streamEnd received');
          const endState = yield* chatState.getState;
          console.log('[useChatStateUpdater] streamEnd - streamingContent:', endState.streamingContent?.substring(0, 100));
          
          if (endState.streamingContent) {
            console.log('[useChatStateUpdater] Adding assistant message from streaming content');
            yield* chatState.addAssistantMessage(endState.streamingContent);
            yield* chatState.updateStreaming('', false);  // Clear streaming state
          } else if (message.content) {
            console.log('[useChatStateUpdater] Adding assistant message from message.content');
            yield* chatState.addAssistantMessage(message.content);
          }
          // Reset loading state when stream ends
          yield* chatState.setLoading(false);
          
          const stateAfter = yield* chatState.getState;
          console.log('[useChatStateUpdater] streamEnd complete - messages count:', stateAfter.messages.length);
          break;
        
        case 'turn/completed':
          // Turn completed - reset loading/streaming state
          yield* chatState.updateStreaming('', false);
          yield* chatState.setLoading(false);
          yield* chatState.clearCurrentTurn();
          break;
        
        case 'updateHeader':
          yield* chatState.updateHeader(message.data);
          break;
        
        case 'modelsList':
          yield* chatState.setModels(message.models || []);
          break;
        
        case 'ItemStarted':
          // Track the current item type and ID
          console.log('[useChatStateUpdater] ItemStarted received:', {
            itemId: message.itemId,
            itemType: message.itemType
          });
          
          if (message.itemType?.type === 'toolCall') {
            console.log('[useChatStateUpdater] ✓ Tool started:', {
              itemId: message.itemId,
              toolName: message.itemType.tool_name,
              args: message.itemType.arguments
            });
            
            // Commit current streaming message before tool call
            const toolState = yield* chatState.getState;
            if (toolState.streamingContent) {
              console.log('[useChatStateUpdater] Committing streaming message before tool');
              yield* chatState.addAssistantMessage(toolState.streamingContent);
            }
            yield* chatState.updateStreaming('', false); // Clear streaming state
            
            // Add tool call to active list with arguments
            yield* chatState.addToolCall(
              message.itemId,
              message.itemType.tool_name,
              message.itemType.arguments
            );
            
            const updatedState = yield* chatState.getState;
            console.log('[useChatStateUpdater] ✓ Tool added to activeToolItemIds:', {
              itemId: message.itemId,
              activeToolItemIds: Array.from(updatedState.activeToolItemIds)
            });
          } else if (message.itemType?.type === 'agentMessage') {
            console.log('[useChatStateUpdater] Agent message item started:', message.itemId);
            // New agent message item - ensure streaming is ready for new content
            const state = yield* chatState.getState;
            if (state.streamingContent) {
              // Commit any existing content first
              yield* chatState.addAssistantMessage(state.streamingContent);
            }
            yield* chatState.updateStreaming('', true); // Start fresh streaming
          }
          break;
        
        case 'ItemCompleted':
          console.log('[useChatStateUpdater] Item completed:', message.itemId);
          
          // Check if this is a tool item
          const completedState = yield* chatState.getState;
          if (completedState.activeToolItemIds.has(message.itemId)) {
            // Tool call completed
            yield* chatState.completeToolCall(message.itemId, 'completed');
          } else {
            // Agent message completed - commit streaming content
            console.log('[useChatStateUpdater] Agent message completed, committing streaming content');
            if (completedState.streamingContent) {
              yield* chatState.addAssistantMessage(completedState.streamingContent);
              yield* chatState.updateStreaming('', false);
              console.log('[useChatStateUpdater] ✓ Message committed and streaming cleared');
            }
          }
          break;
        
        case 'ItemFailed':
          console.log('[useChatStateUpdater] Item failed:', message.itemId);
          yield* chatState.completeToolCall(message.itemId, 'failed');
          break;
      }
    });
    
    Runtime.runPromise(runtime)(program).catch(console.error);
  }, [runtime]);
  
  return {
    updateFromMessage,
  };
}
