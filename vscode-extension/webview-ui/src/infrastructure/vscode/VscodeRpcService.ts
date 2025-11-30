import { Effect, Context, Layer } from 'effect';
import { getVscodeApi } from './VscodeApi';

/**
 * VSCode RPC Service Tag
 * Provides Effect-based commands for sending messages to VSCode extension host
 * 
 * Note: Incoming messages are handled by JsonRpcService which provides
 * a reactive notification stream consumed by App.tsx
 */
export interface VscodeRpcService {
  /**
   * Send a message to the extension host
   */
  readonly postMessage: (message: any) => Effect.Effect<void>;
  
  /**
   * Send ready message to extension host
   */
  readonly sendReady: Effect.Effect<void>;
  
  /**
   * Request models list from extension host
   */
  readonly requestModels: Effect.Effect<void>;
  
  /**
   * Request agents list from extension host
   */
  readonly requestAgents: Effect.Effect<void>;
  
  /**
   * Send message to chat
   */
  readonly sendMessage: (text: string) => Effect.Effect<void>;
  
  /**
   * Change model
   */
  readonly changeModel: (modelId: string) => Effect.Effect<void>;
  
  /**
   * Change agent
   */
  readonly changeAgent: (agentId: string) => Effect.Effect<void>;
  
  /**
   * Cancel turn
   */
  readonly cancelTurn: (threadId: string, turnId: string) => Effect.Effect<void>;
}

export const VscodeRpcService = Context.GenericTag<VscodeRpcService>('VscodeRpcService');

/**
 * Live implementation of VscodeRpcService for sending messages to extension host
 * 
 * Note: Incoming message handling is done by JsonRpcService which creates a
 * reactive Stream of notifications that App.tsx consumes
 */
export const VscodeRpcServiceLive = Layer.succeed(
  VscodeRpcService,
  VscodeRpcService.of({
    postMessage: (message: any) =>
      Effect.sync(() => {
        console.log('[VscodeRpcService] Sending message:', message);
        getVscodeApi().postMessage(message);
      }),
    
    sendReady: Effect.sync(() => {
      console.log('[VscodeRpcService] Sending ready (JSON-RPC)');
      getVscodeApi().postMessage({ 
        jsonrpc: '2.0',
        method: 'webview/ready',
        params: {}
      });
    }),
    
    requestModels: Effect.sync(() => {
      console.log('[VscodeRpcService] Requesting models (JSON-RPC)');
      getVscodeApi().postMessage({ 
        jsonrpc: '2.0',
        method: 'models/request',
        params: {}
      });
    }),
    
    requestAgents: Effect.sync(() => {
      console.log('[VscodeRpcService] Requesting agents (JSON-RPC)');
      getVscodeApi().postMessage({ 
        jsonrpc: '2.0',
        method: 'agents/request',
        params: {}
      });
    }),
    
    sendMessage: (text: string) =>
      Effect.sync(() => {
        console.log('[VscodeRpcService] Sending message:', text);
        getVscodeApi().postMessage({ 
          jsonrpc: '2.0',
          method: 'chat/sendMessage',
          params: { text }
        });
      }),
    
    changeModel: (modelId: string) =>
      Effect.sync(() => {
        console.log('[VscodeRpcService] Changing model:', modelId);
        getVscodeApi().postMessage({ 
          jsonrpc: '2.0',
          method: 'model/change',
          params: { modelId }
        });
      }),
    
    changeAgent: (agentId: string) =>
      Effect.sync(() => {
        console.log('[VscodeRpcService] Changing agent:', agentId);
        getVscodeApi().postMessage({ 
          jsonrpc: '2.0',
          method: 'agent/change',
          params: { agentId }
        });
      }),
    
    cancelTurn: (threadId: string, turnId: string) =>
      Effect.sync(() => {
        console.log('[VscodeRpcService] Cancelling turn:', threadId, turnId);
        getVscodeApi().postMessage({ 
          jsonrpc: '2.0',
          method: 'turn/cancel',
          params: { threadId, turnId }
        });
      }),
  })
);
