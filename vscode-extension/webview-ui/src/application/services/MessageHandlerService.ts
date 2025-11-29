import { Effect, Stream } from 'effect';
import { VscodeRpcService } from '@/infrastructure/vscode/VscodeRpcService';
import { ChatStateService } from '../state/ChatStateService';

/**
 * Message Handler Service
 * Processes incoming messages from VSCode extension and updates state
 */
export const handleIncomingMessages = Effect.gen(function* () {
  console.log('[MessageHandler] Starting...');
  const vscode = yield* VscodeRpcService;
  const chatState = yield* ChatStateService;
  
  console.log('[MessageHandler] Services acquired, subscribing to messages...');
  
  // Process incoming messages
  yield* Stream.runForEach(vscode.messages, (message: any) =>
    Effect.gen(function* () {
      console.log('[MessageHandler] Processing message:', message);
      
      switch (message.type) {
        case 'state':
          if (message.messages) {
            yield* chatState.updateState((state) => ({
              ...state,
              messages: message.messages,
            }));
          }
          if (message.agent || message.model || message.tokens || message.cost) {
            yield* chatState.updateHeader({
              agent: message.agent,
              model: message.model,
              tokens: message.tokens,
              cost: message.cost,
            });
          }
          break;
        
        case 'streamStart':
          yield* chatState.updateStreaming('', true);
          break;
        
        case 'streamDelta':
          const currentState = yield* chatState.getState();
          yield* chatState.updateStreaming(
            currentState.streamingContent + message.delta,
            true
          );
          break;
        
        case 'streamEnd':
          const state = yield* chatState.getState();
          if (state.streamingContent) {
            yield* chatState.addAssistantMessage(state.streamingContent);
          } else if (message.content) {
            yield* chatState.addAssistantMessage(message.content);
          }
          break;
        
        case 'updateHeader':
          yield* chatState.updateHeader(message.data);
          break;
        
        case 'modelsList':
          yield* chatState.setModels(message.models || []);
          break;
      }
    })
  );
});

/**
 * Initialize chat - sends ready message and requests models
 */
export const initializeChat = Effect.gen(function* () {
  console.log('[InitializeChat] Starting...');
  const vscode = yield* VscodeRpcService;
  console.log('[InitializeChat] Sending ready message...');
  yield* vscode.sendReady();
  console.log('[InitializeChat] Requesting models...');
  yield* vscode.requestModels();
  console.log('[InitializeChat] Complete');
});
