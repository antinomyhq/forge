import { Effect, Context, Layer } from 'effect';

// VSCode API singleton
let vscodeApi: any = null;
function getVscodeApi() {
  if (!vscodeApi) {
    // @ts-ignore
    vscodeApi = acquireVsCodeApi();
  }
  return vscodeApi;
}

/**
 * Simple VSCode RPC Service
 */
export interface VscodeRpcService {
  readonly sendReady: Effect.Effect<void>;
  readonly requestModels: Effect.Effect<void>;
  readonly requestAgents: Effect.Effect<void>;
  readonly sendMessage: (text: string) => Effect.Effect<void>;
  readonly changeModel: (modelId: string) => Effect.Effect<void>;
  readonly changeAgent: (agentId: string) => Effect.Effect<void>;
  readonly cancelTurn: (threadId: string, turnId: string) => Effect.Effect<void>;
}

export const VscodeRpcService = Context.GenericTag<VscodeRpcService>('VscodeRpcService');

export const VscodeRpcServiceLive = Layer.succeed(
  VscodeRpcService,
  VscodeRpcService.of({
    sendReady: Effect.sync(() => {
      console.log('[VscodeRpc] Sending ready');
      getVscodeApi().postMessage({ type: 'ready' });
    }),
    
    requestModels: Effect.sync(() => {
      console.log('[VscodeRpc] Requesting models');
      getVscodeApi().postMessage({ type: 'requestModels' });
    }),
    
    requestAgents: Effect.sync(() => {
      console.log('[VscodeRpc] Requesting agents');
      getVscodeApi().postMessage({ type: 'requestAgents' });
    }),
    
    sendMessage: (text: string) =>
      Effect.sync(() => {
        console.log('[VscodeRpc] Sending message:', text);
        getVscodeApi().postMessage({ type: 'sendMessage', text });
      }),
    
    changeModel: (modelId: string) =>
      Effect.sync(() => {
        console.log('[VscodeRpc] Changing model:', modelId);
        getVscodeApi().postMessage({ type: 'modelChange', modelId });
      }),
    
    changeAgent: (agentId: string) =>
      Effect.sync(() => {
        console.log('[VscodeRpc] Changing agent:', agentId);
        getVscodeApi().postMessage({ type: 'agentChange', agentId });
      }),
    
    cancelTurn: (threadId: string, turnId: string) =>
      Effect.sync(() => {
        console.log('[VscodeRpc] Cancelling turn:', threadId, turnId);
        getVscodeApi().postMessage({ type: 'cancel', threadId, turnId });
      }),
  })
);
