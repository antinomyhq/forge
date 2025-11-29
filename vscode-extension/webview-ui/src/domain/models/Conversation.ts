import { Schema as S } from "effect";
import { Message } from "./Message";

/// ConversationId represents a unique identifier for a conversation
export class ConversationId extends S.Class<ConversationId>("ConversationId")({
  value: S.UUID,
}) {}

/// Conversation represents a thread of messages with an AI agent
export class Conversation extends S.Class<Conversation>("Conversation")({
  id: ConversationId,
  threadId: S.String,
  messages: S.Array(Message),
  agent: S.String,
  model: S.String,
  createdAt: S.Date,
  updatedAt: S.Date,
}) {}
