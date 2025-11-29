import { Schema as S } from "effect";

/// JsonRpcRequest represents a JSON-RPC 2.0 request
export class JsonRpcRequest extends S.Class<JsonRpcRequest>("JsonRpcRequest")({
  jsonrpc: S.Literal("2.0"),
  id: S.String,
  method: S.String,
  params: S.optional(S.Unknown),
}) {}

/// JsonRpcResponse represents a JSON-RPC 2.0 response
export class JsonRpcResponse extends S.Class<JsonRpcResponse>("JsonRpcResponse")({
  jsonrpc: S.Literal("2.0"),
  id: S.String,
  result: S.optional(S.Unknown),
  error: S.optional(
    S.Struct({
      code: S.Number,
      message: S.String,
      data: S.optional(S.Unknown),
    })
  ),
}) {}

/// JsonRpcNotification represents a JSON-RPC 2.0 notification
export class JsonRpcNotification extends S.Class<JsonRpcNotification>("JsonRpcNotification")({
  jsonrpc: S.Literal("2.0"),
  method: S.String,
  params: S.optional(S.Unknown),
}) {}
