import { Effect, Context, Layer } from 'effect';
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

/**
 * Live implementation of VscodeRpcService  
 * Note: Message listening is handled by App.tsx useEffect with window.addEventListener
 * We don't set up a global listener here to avoid duplicate message processing
 */
export const VscodeRpcServiceLive = Layer.succeed(
  VscodeRpcService,
  VscodeRpcService.of({
    postMessage: (message: any) =>
      Effect.sync(() => {
        console.log('[VscodeRpcService] Sending message:', message);
        getVscodeApi().postMessage(message);
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
