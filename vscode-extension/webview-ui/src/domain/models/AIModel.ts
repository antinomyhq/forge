import { Schema as S } from "effect";

/// AIModel represents an available AI model with its configuration
export class AIModel extends S.Class<AIModel>("AIModel")({
  id: S.String,
  name: S.optional(S.String),
  label: S.optional(S.String),
  provider: S.optional(S.String),
  contextWindow: S.optional(S.Number),
}) {}
