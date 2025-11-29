import { Effect, Context, Stream, Layer } from 'effect';
import { getVscodeApi } from './VscodeApi';

/**
 * VSCode RPC Service Tag
 * Provides Effect-based JSON-RPC communication with VSCode extension host
 */
export interface VscodeRpcService {
  /**
   * Send a message to the extension host
   */
  readonly postMessage: (message: any) => Effect.Effect<void>;
  
  /**
   * Subscribe to incoming messages from extension host
   */
  readonly messages: Stream.Stream<any>;
  
  /**
   * Send ready message to extension host
   */
  readonly sendReady: () => Effect.Effect<void>;
  
  /**
   * Request models list from extension host
   */
  readonly requestModels: () => Effect.Effect<void>;
  
  /**
   * Send message to chat
   */
  readonly sendChatMessage: (text: string) => Effect.Effect<void>;
  
  /**
   * Change model
   */
  readonly changeModel: (modelId: string) => Effect.Effect<void>;
}

export const VscodeRpcService = Context.GenericTag<VscodeRpcService>('VscodeRpcService');

// Store message handlers globally
const messageHandlers: Array<(message: any) => void> = [];

/**
 * Live implementation of VscodeRpcService
 */
export const VscodeRpcServiceLive = Layer.succeed(
  VscodeRpcService,
  VscodeRpcService.of({
    postMessage: (message: any) =>
      Effect.sync(() => {
        console.log('[VscodeRpcService] Sending message:', message);
        getVscodeApi().postMessage(message);
      }),
    
    messages: Stream.async<any>((emit) => {
      console.log('[VscodeRpcService] Stream subscriber attached');
      
      const handler = (message: any) => {
        console.log('[VscodeRpcService] Emitting to stream:', message);
        emit.single(message);
      };
      
      messageHandlers.push(handler);
      
      return Effect.sync(() => {
        console.log('[VscodeRpcService] Stream subscriber detached');
        const index = messageHandlers.indexOf(handler);
        if (index > -1) {
          messageHandlers.splice(index, 1);
        }
      });
    }),
    
    sendReady: () =>
      Effect.sync(() => {
        console.log('[VscodeRpcService] Sending ready');
        getVscodeApi().postMessage({ type: 'ready' });
      }),
    
    requestModels: () =>
      Effect.sync(() => {
        console.log('[VscodeRpcService] Requesting models');
        getVscodeApi().postMessage({ type: 'requestModels' });
      }),
    
    sendChatMessage: (text: string) =>
      Effect.sync(() => {
        console.log('[VscodeRpcService] Sending chat message:', text);
        getVscodeApi().postMessage({ type: 'sendMessage', text });
      }),
    
    changeModel: (modelId: string) =>
      Effect.sync(() => {
        console.log('[VscodeRpcService] Changing model:', modelId);
        getVscodeApi().postMessage({ type: 'modelChange', modelId });
      }),
  })
);

// Set up global message listener once
if (typeof window !== 'undefined') {
  window.addEventListener('message', (event: MessageEvent) => {
    console.log('[VscodeRpcService] Window received message:', event.data);
    // Notify all handlers
    messageHandlers.forEach(handler => {
      try {
        handler(event.data);
      } catch (error) {
        console.error('[VscodeRpcService] Handler error:', error);
      }
    });
  });
  console.log('[VscodeRpcService] Global message listener attached');
}
