import { Effect } from "effect";
import { Conversation } from "../models/Conversation";
import { Message } from "../models/Message";
import { MessageValidationService } from "./MessageValidationService";
import { DomainError } from "@shared/types/errors";

/// ConversationService provides business logic for conversation operations
export class ConversationService {
  /// Adds a message to a conversation
  ///
  /// # Errors
  /// Returns DomainError if validation fails
  static addMessage(
    conversation: Conversation,
    message: Message
  ): Effect.Effect<Conversation, DomainError> {
    return Effect.gen(function* () {
      // Validate message
      yield* MessageValidationService.validateMessage(message.content).pipe(
        Effect.mapError((e) => new DomainError({ message: e.reason }))
      );

      // Create updated conversation
      return new Conversation({
        ...conversation,
        messages: [...conversation.messages, message],
        updatedAt: new Date(),
      });
    });
  }

  /// Calculates approximate token usage for a conversation
  static calculateTokenUsage(conversation: Conversation): Effect.Effect<number> {
    return Effect.succeed(
      conversation.messages.reduce((sum, msg) => sum + msg.content.length, 0)
    );
  }
}
