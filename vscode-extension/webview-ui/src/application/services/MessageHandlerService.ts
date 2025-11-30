import { Effect, Context, Layer } from 'effect';
import { ChatStateService } from '@/application/state/ChatStateService';
import { 
  decodeExtensionMessageEffect, 
  isMessageType 
} from '@/domain/schemas/MessageSchemas';
import { ValidationError } from '@/shared/types/errors';

/**
 * Service that handles messages from the extension host
 * with runtime validation using Effect Schema
 */
export interface MessageHandlerService {
  readonly handleMessage: (data: unknown) => Effect.Effect<void, ValidationError, ChatStateService>;
}

export const MessageHandlerService = Context.GenericTag<MessageHandlerService>('MessageHandlerService');

/**
 * Live implementation with schema validation
 */
export const MessageHandlerServiceLive = Layer.effect(
  MessageHandlerService,
  Effect.gen(function* () {
    console.log('[MessageHandlerService] Initializing with schema validation...');

    const handleMessage = (data: unknown) =>
      Effect.gen(function* () {
        // Decode and validate message using Effect Schema
        const message = yield* decodeExtensionMessageEffect(data).pipe(
          Effect.mapError((parseError) => 
            new ValidationError({ 
              reason: `Invalid message format: ${parseError.message}` 
            })
          )
        );

        console.log('[MessageHandler] Validated message:', message.type);

        // Get chat state service
        const chatState = yield* ChatStateService;

        // Handle different message types with type safety
        if (isMessageType(message, 'state')) {
          if (message.messages) {
            yield* chatState.setMessages([...message.messages]);
          }
          if (message.agent || message.model || message.tokens || message.cost) {
            yield* chatState.updateHeader({
              agent: message.agent,
              agentId: message.agentId,
              model: message.model,
              modelId: message.modelId,
              tokens: message.tokens,
              cost: message.cost,
            });
          }
        } else if (isMessageType(message, 'turn/started')) {
          yield* chatState.setCurrentTurn(message.threadId, message.turnId);
        } else if (isMessageType(message, 'streamStart')) {
          yield* chatState.updateStreaming('', true);
        } else if (isMessageType(message, 'streamDelta')) {
          const state = yield* chatState.getState;
          const delta = message.delta || '';
          const itemId = message.itemId;

          // Filter out deltas from tool call items
          if (itemId && state.activeToolItemIds.has(itemId)) {
            console.log('[MessageHandler] Filtering delta from tool item:', itemId);
          } else if (delta) {
            yield* chatState.updateStreaming(state.streamingContent + delta, true);
          }
        } else if (isMessageType(message, 'streamEnd')) {
          const state = yield* chatState.getState;

          if (state.streamingContent) {
            yield* chatState.addAssistantMessage(state.streamingContent);
            yield* chatState.updateStreaming('', false);
          } else if (message.content) {
            yield* chatState.addAssistantMessage(message.content);
          }
          yield* chatState.setLoading(false);
        } else if (isMessageType(message, 'turn/completed')) {
          yield* chatState.updateStreaming('', false);
          yield* chatState.setLoading(false);
          yield* chatState.clearCurrentTurn();
        } else if (isMessageType(message, 'ItemStarted')) {
          if (message.toolName) {
            yield* chatState.addToolCall(message.itemId, message.toolName, message.args);
          }
        } else if (isMessageType(message, 'ItemCompleted')) {
          yield* chatState.completeToolCall(
            message.itemId,
            message.status || 'completed'
          );
        } else if (isMessageType(message, 'modelsList')) {
          yield* chatState.setModels([...message.models]);
        } else if (isMessageType(message, 'agentsList')) {
          yield* chatState.setAgents([...message.agents]);
        }
      });

    return MessageHandlerService.of({
      handleMessage,
    });
  })
);
