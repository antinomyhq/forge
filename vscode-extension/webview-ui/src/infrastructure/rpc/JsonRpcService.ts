import { Context, Effect, Layer, Stream, Ref, Deferred, Duration, Queue } from "effect";
import { RpcError, RpcTimeoutError } from "@shared/types/errors";
import { JsonRpcNotification } from "./JsonRpcTypes";
import { getVscodeApi } from "@/infrastructure/vscode/VscodeApi";

/// JsonRpcService provides JSON-RPC 2.0 communication via VSCode postMessage
/// with reactive notification streaming
export interface JsonRpcService {
  /// Send a request to the extension host and await response
  readonly request: <A, E = never>(
    method: string,
    params: unknown
  ) => Effect.Effect<A, RpcError | E>;

  /// Stream of all notifications received from the extension host
  /// Consumed by App.tsx to update application state reactively
  readonly notifications: Stream.Stream<JsonRpcNotification>;
}

export const JsonRpcService = Context.GenericTag<JsonRpcService>("JsonRpcService");

/// JsonRpcServiceLive implements the JSON-RPC service using VSCode postMessage API
export const JsonRpcServiceLive = Layer.scoped(
  JsonRpcService,
  Effect.gen(function* () {
    const queue = yield* Queue.unbounded<JsonRpcNotification>();
    const requests = yield* Ref.make(new Map<string, Deferred.Deferred<unknown, RpcError>>());

    // Get VSCode API (singleton)
    const vscode = getVscodeApi();

    // Set up message handler using window.addEventListener
    // Register cleanup with finalizer so it stays alive throughout scope
    console.log('[JsonRpcService] Setting up message listener...');
    
    const handler = (event: MessageEvent) => {
      const message = event.data;

      console.log('[JsonRpcService] Received message:', message);

      if ("id" in message) {
        // Response - run the effect to update deferred
        Effect.runPromise(
          Ref.get(requests).pipe(
            Effect.flatMap((map) => {
              const deferred = map.get(message.id);
              if (deferred) {
                if (message.error) {
                  return Deferred.fail(
                    deferred,
                    new RpcError({
                      method: "unknown",
                      message: message.error.message,
                      cause: message.error,
                    })
                  );
                } else {
                  return Deferred.succeed(deferred, message.result);
                }
              }
              return Effect.void;
            })
          )
        ).catch(console.error);
      } else if ("method" in message) {
        // JSON-RPC Notification - offer to queue
        console.log('[JsonRpcService] Queueing notification:', message.method);
        Effect.runPromise(Queue.offer(queue, message as JsonRpcNotification)).catch(console.error);
      } else {
        console.warn('[JsonRpcService] Ignoring non-JSON-RPC message:', message);
      }
    };

    window.addEventListener('message', handler);
    console.log('[JsonRpcService] Message listener ACTIVE - ready to receive messages');
    
    // Register cleanup to be called when scope closes
    yield* Effect.addFinalizer(() => Effect.sync(() => {
      console.log('[JsonRpcService] Removing message listener');
      window.removeEventListener('message', handler);
    }));

    return JsonRpcService.of({
      request: <A, E = never>(method: string, params: unknown) =>
        Effect.gen(function* () {
          const id = crypto.randomUUID();
          const deferred = yield* Deferred.make<A, RpcError>();

          yield* Ref.update(requests, (map) => new Map(map).set(id, deferred as Deferred.Deferred<unknown, RpcError>));

          // Send message to extension host
          vscode.postMessage({ jsonrpc: "2.0", id, method, params });

          return yield* Deferred.await(deferred).pipe(
            Effect.timeout(Duration.seconds(30)),
            Effect.catchTag("TimeoutException", () =>
              Effect.fail(new RpcTimeoutError({ method }))
            )
          ) as Effect.Effect<A, RpcError | E>;
        }),

      notifications: Stream.fromQueue(queue),
    });
  })
);
