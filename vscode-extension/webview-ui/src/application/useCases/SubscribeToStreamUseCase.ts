import { Effect, Stream } from "effect";
import { JsonRpcService } from "@infrastructure/rpc";
import { ApplicationError } from "@shared/types/errors";
import { StreamDelta } from "@domain/models";

/// SubscribeToStreamUseCase handles subscribing to streaming responses
export class SubscribeToStreamUseCase {
  /// Executes the use case to subscribe to stream deltas
  ///
  /// # Arguments
  /// - conversationId: The ID of the conversation
  /// - onDelta: Callback function called for each delta
  ///
  /// # Errors
  /// Returns ApplicationError if subscription fails
  static execute(
    _conversationId: string,
    onDelta: (delta: StreamDelta) => void
  ): Effect.Effect<void, ApplicationError, JsonRpcService> {
    return Effect.scoped(
      Effect.gen(function* () {
        const rpc = yield* JsonRpcService;

        return yield* Stream.runForEach(
          rpc.notifications.pipe(
            Stream.filter((notif) => notif.method === "agentMessage/delta"),
            Stream.map(
              (notif) =>
                new StreamDelta({
                  type: "content",
                  content: (notif.params as { delta: string }).delta,
                  timestamp: new Date(),
                })
            )
          ),
          (delta) => Effect.sync(() => onDelta(delta))
        ).pipe(Effect.forkScoped, Effect.asVoid);
      })
    );
  }
}
