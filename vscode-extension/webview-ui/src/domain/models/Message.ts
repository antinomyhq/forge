import { Schema as S } from "effect";

/// MessageId represents a unique identifier for a message
export class MessageId extends S.Class<MessageId>("MessageId")({
  value: S.UUID,
}) {}

/// Message represents a chat message in a conversation
export class Message extends S.Class<Message>("Message")({
  id: MessageId,
  content: S.String,
  role: S.Literal("user", "assistant", "system"),
  timestamp: S.Date,
  status: S.Literal("pending", "completed", "failed"),
  metadata: S.optional(S.Record({ key: S.String, value: S.Unknown })),
}) {}
