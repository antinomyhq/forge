import { Data } from "effect";
import { StreamDelta } from "../models/StreamDelta";

/// MessageSentEvent is emitted when a message is sent
export class MessageSentEvent extends Data.TaggedClass("MessageSentEvent")<{
  readonly messageId: string;
  readonly conversationId: string;
  readonly occurredAt: Date;
}> {}

/// ConversationStartedEvent is emitted when a conversation starts
export class ConversationStartedEvent extends Data.TaggedClass("ConversationStartedEvent")<{
  readonly conversationId: string;
  readonly agent: string;
  readonly occurredAt: Date;
}> {}

/// StreamDeltaReceivedEvent is emitted when a stream delta is received
export class StreamDeltaReceivedEvent extends Data.TaggedClass("StreamDeltaReceivedEvent")<{
  readonly delta: StreamDelta;
  readonly conversationId: string;
  readonly occurredAt: Date;
}> {}
