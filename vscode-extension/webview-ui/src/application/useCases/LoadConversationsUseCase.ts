import { Effect } from "effect";
import { JsonRpcService } from "@infrastructure/rpc";
import { ApplicationError } from "@shared/types/errors";
import { listThreads } from "@infrastructure/rpc/RpcMethods";

/// LoadConversationsUseCase handles loading the list of conversations
export class LoadConversationsUseCase {
  /// Executes the use case to load all conversations
  ///
  /// # Errors
  /// Returns ApplicationError if loading fails
  static execute(): Effect.Effect<
    ReadonlyArray<{ threadId: string; title?: string; updatedAt?: Date }>,
    ApplicationError,
    JsonRpcService
  > {
    return Effect.gen(function* () {
      // Fetch thread list from server (server has all data)
      const threads = yield* listThreads().pipe(
        Effect.mapError((e) => new ApplicationError({ message: e.message, cause: e }))
      );

      // Parse dates and return
      return threads.map((t) => ({
        threadId: t.threadId,
        ...(t.title !== undefined ? { title: t.title } : {}),
        ...(t.updatedAt ? { updatedAt: new Date(t.updatedAt) } : {}),
      }));
    });
  }
}
