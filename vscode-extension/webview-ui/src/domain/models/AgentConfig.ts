import { Schema as S } from "effect";

/// AgentConfig represents the configuration for an AI agent
export class AgentConfig extends S.Class<AgentConfig>("AgentConfig")({
  name: S.String,
  model: S.String,
  provider: S.String,
  maxTokens: S.Number,
  temperature: S.Number.pipe(S.between(0, 2)),
}) {}
