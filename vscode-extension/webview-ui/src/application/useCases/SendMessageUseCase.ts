import { Effect } from "effect";
import { MessageValidationService } from "@domain/services";
import { JsonRpcService } from "@infrastructure/rpc";
import { ApplicationError } from "@shared/types/errors";
import { parseFileReferences } from "@shared/utils/fileParser";
import { sendMessage } from "@infrastructure/rpc/RpcMethods";

/// SendMessageUseCase handles sending a message in a conversation
export class SendMessageUseCase {
  /// Executes the use case to send a message
  ///
  /// # Arguments
  /// - threadId: The ID of the thread
  /// - content: The message content
  ///
  /// # Errors
  /// Returns ApplicationError if validation or sending fails
  static execute(
    threadId: string,
    content: string
  ): Effect.Effect<void, ApplicationError, JsonRpcService> {
    return Effect.gen(function* () {
      // Validate message
      const validated = yield* MessageValidationService.validateMessage(content).pipe(
        Effect.mapError((e) => new ApplicationError({ message: e.reason }))
      );

      // Parse file references from message (e.g., @[src/file.ts])
      const files = parseFileReferences(validated);

      // Send to server (server handles file reading and attachment resolution)
      yield* sendMessage(threadId, validated, files).pipe(
        Effect.mapError((e) => new ApplicationError({ message: e.message, cause: e }))
      );

      // Server will send notifications for updates
      // No local persistence needed - server is source of truth
    });
  }
}
