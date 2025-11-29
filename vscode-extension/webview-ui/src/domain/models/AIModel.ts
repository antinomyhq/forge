import { Schema as S } from "effect";

/// AIModel represents an available AI model with its configuration
/// Matches the fields from generated Model type
export class AIModel extends S.Class<AIModel>("AIModel")({
  id: S.String,
  name: S.optional(S.String),
  description: S.optional(S.String),
  context_length: S.optional(S.Number),
  tools_supported: S.optional(S.Boolean),
  supports_parallel_tool_calls: S.optional(S.Boolean),
  supports_reasoning: S.optional(S.Boolean),
}) {}
