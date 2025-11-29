import { Effect } from "effect";
import { JsonRpcService } from "@infrastructure/rpc";
import { ApplicationError } from "@shared/types/errors";
import { startThread } from "@infrastructure/rpc/RpcMethods";

/// StartConversationUseCase handles starting a new conversation
export class StartConversationUseCase {
  /// Executes the use case to start a new conversation
  ///
  /// # Errors
  /// Returns ApplicationError if starting the thread fails
  static execute(): Effect.Effect<string, ApplicationError, JsonRpcService> {
    return Effect.gen(function* () {
      // Start thread on server
      const { threadId } = yield* startThread().pipe(
        Effect.mapError((e) => new ApplicationError({ message: e.message, cause: e }))
      );

      // Return threadId - server handles persistence
      return threadId;
    });
  }
}
