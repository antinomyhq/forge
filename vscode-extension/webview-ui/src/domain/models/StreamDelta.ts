import { Schema as S } from "effect";

/// StreamDelta represents a chunk of streaming response data
export class StreamDelta extends S.Class<StreamDelta>("StreamDelta")({
  type: S.Literal("content", "reasoning", "tool"),
  content: S.String,
  timestamp: S.Date,
}) {}
