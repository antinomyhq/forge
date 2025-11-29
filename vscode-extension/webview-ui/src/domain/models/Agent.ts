import { Schema as S } from "@effect/schema";

export class Agent extends S.Class<Agent>("Agent")({
  id: S.String,
  title: S.optional(S.String),  // Changed from 'name' to 'title' to match domain type
  description: S.optional(S.String),
  provider: S.optional(S.String),
  model: S.optional(S.String),
  capabilities: S.optional(S.Array(S.String)),
}) {}
