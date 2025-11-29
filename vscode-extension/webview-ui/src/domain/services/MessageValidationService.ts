import { Effect } from "effect";
import { ValidationError } from "@shared/types/errors";

/// MessageValidationService provides validation logic for messages
export class MessageValidationService {
  /// Validates a message ensuring it meets business rules
  ///
  /// # Errors
  /// Returns ValidationError if message is empty or exceeds maximum length
  static validateMessage(message: string): Effect.Effect<string, ValidationError> {
    return Effect.gen(function* () {
      if (message.trim().length === 0) {
        yield* Effect.fail(new ValidationError({ reason: "Message cannot be empty" }));
      }
      if (message.length > 10000) {
        yield* Effect.fail(new ValidationError({ reason: "Message too long (max 10000 characters)" }));
      }
      return message.trim();
    });
  }
}
