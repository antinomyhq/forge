import { Schema as S } from '@effect/schema';

/**
 * Schema for messages received from the VSCode extension host
 * Provides runtime type validation at API boundaries
 */

// Base message schema
export class BaseMessage extends S.Class<BaseMessage>('BaseMessage')({
  type: S.String,
}) {}

// State message schema
export class StateMessage extends S.Class<StateMessage>('StateMessage')({
  type: S.Literal('state'),
  messages: S.optional(S.Array(S.Unknown)),
  agent: S.optional(S.String),
  agentId: S.optional(S.String),
  model: S.optional(S.String),
  modelId: S.optional(S.String),
  tokens: S.optional(S.String),
  cost: S.optional(S.String),
}) {}

// Turn started message schema
export class TurnStartedMessage extends S.Class<TurnStartedMessage>('TurnStartedMessage')({
  type: S.Literal('turn/started'),
  threadId: S.String,
  turnId: S.String,
}) {}

// Stream start message schema
export class StreamStartMessage extends S.Class<StreamStartMessage>('StreamStartMessage')({
  type: S.Literal('streamStart'),
}) {}

// Stream delta message schema
export class StreamDeltaMessage extends S.Class<StreamDeltaMessage>('StreamDeltaMessage')({
  type: S.Literal('streamDelta'),
  delta: S.optional(S.String),
  itemId: S.optional(S.String),
}) {}

// Stream end message schema
export class StreamEndMessage extends S.Class<StreamEndMessage>('StreamEndMessage')({
  type: S.Literal('streamEnd'),
  content: S.optional(S.String),
}) {}

// Turn completed message schema
export class TurnCompletedMessage extends S.Class<TurnCompletedMessage>('TurnCompletedMessage')({
  type: S.Literal('turn/completed'),
}) {}

// Item started message schema (tool execution)
export class ItemStartedMessage extends S.Class<ItemStartedMessage>('ItemStartedMessage')({
  type: S.Literal('ItemStarted'),
  itemId: S.String,
  toolName: S.optional(S.String),
  args: S.optional(S.Record({ key: S.String, value: S.Unknown })),
}) {}

// Item completed message schema
export class ItemCompletedMessage extends S.Class<ItemCompletedMessage>('ItemCompletedMessage')({
  type: S.Literal('ItemCompleted'),
  itemId: S.String,
  status: S.optional(S.Literal('completed', 'failed')),
}) {}

// Models list message schema
export class ModelsListMessage extends S.Class<ModelsListMessage>('ModelsListMessage')({
  type: S.Literal('modelsList'),
  models: S.Array(
    S.Struct({
      id: S.String,
      name: S.optional(S.String),
      label: S.optional(S.String),
      provider: S.optional(S.String),
      contextWindow: S.optional(S.Number),
    })
  ),
}) {}

// Agents list message schema
export class AgentsListMessage extends S.Class<AgentsListMessage>('AgentsListMessage')({
  type: S.Literal('agentsList'),
  agents: S.Array(
    S.Struct({
      id: S.String,
      name: S.optional(S.String),
      description: S.optional(S.String),
      provider: S.optional(S.String),
      model: S.optional(S.String),
      capabilities: S.optional(S.Array(S.String)),
    })
  ),
}) {}

// Union of all possible message types
export const ExtensionMessage = S.Union(
  StateMessage,
  TurnStartedMessage,
  StreamStartMessage,
  StreamDeltaMessage,
  StreamEndMessage,
  TurnCompletedMessage,
  ItemStartedMessage,
  ItemCompletedMessage,
  ModelsListMessage,
  AgentsListMessage
);

export type ExtensionMessage = S.Schema.Type<typeof ExtensionMessage>;

/**
 * Decode and validate a message from the extension host
 * Returns Either with validation error or valid message
 */
export const decodeExtensionMessage = S.decodeUnknownEither(ExtensionMessage);

/**
 * Decode with Effect - fails with ParseError on invalid data
 */
export const decodeExtensionMessageEffect = S.decodeUnknown(ExtensionMessage);

/**
 * Safely decode a message, returning null on error
 */
export function safeDecodeMessage(data: unknown): ExtensionMessage | null {
  const result = decodeExtensionMessage(data);
  if (result._tag === 'Right') {
    return result.right;
  }
  console.error('[Schema] Message validation failed:', result.left);
  return null;
}

/**
 * Type guard to check if message matches a specific type
 */
export function isMessageType<T extends ExtensionMessage['type']>(
  message: ExtensionMessage,
  type: T
): message is Extract<ExtensionMessage, { type: T }> {
  return message.type === type;
}
