import { Context, Effect, Layer, Stream, Ref, Deferred, Duration, Queue } from "effect";
import { RpcError, RpcTimeoutError } from "@shared/types/errors";
import { JsonRpcNotification } from "./JsonRpcTypes";

// VSCode API type (available in webview context)
declare const acquireVsCodeApi: () => {
  postMessage: (message: unknown) => void;
  getState: () => unknown;
  setState: (state: unknown) => void;
};

/// JsonRpcService provides JSON-RPC 2.0 communication via VSCode postMessage
export interface JsonRpcService {
  readonly request: <A, E = never>(
    method: string,
    params: unknown
  ) => Effect.Effect<A, RpcError | E>;

  readonly notifications: Stream.Stream<JsonRpcNotification>;
}

export const JsonRpcService = Context.GenericTag<JsonRpcService>("JsonRpcService");

/// JsonRpcServiceLive implements the JSON-RPC service using VSCode postMessage API
export const JsonRpcServiceLive = Layer.scoped(
  JsonRpcService,
  Effect.gen(function* () {
    const queue = yield* Queue.unbounded<JsonRpcNotification>();
    const requests = yield* Ref.make(new Map<string, Deferred.Deferred<unknown, RpcError>>());

    // Get VSCode API
    const vscode = acquireVsCodeApi();

    // Set up message handler using window.addEventListener
    yield* Effect.async<void>((_resume) => {
      const handler = (event: MessageEvent) => {
        const message = event.data;

        if ("id" in message) {
          // Response
          Effect.runSync(
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
          );
        } else if ("method" in message) {
          // Notification
          Effect.runSync(Queue.offer(queue, message as JsonRpcNotification));
        }
      };

      window.addEventListener('message', handler);

      // Return cleanup effect
      return Effect.sync(() => {
        window.removeEventListener('message', handler);
      });
    }).pipe(Effect.forkScoped);

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
