import { Schema as S } from "effect";

/// ToolExecution represents the execution of a tool by the AI agent
export class ToolExecution extends S.Class<ToolExecution>("ToolExecution")({
  id: S.String,
  toolName: S.String,
  status: S.Literal("started", "completed", "failed"),
  startTime: S.Date,
  endTime: S.optional(S.Date),
  result: S.optional(S.Unknown),
}) {}
