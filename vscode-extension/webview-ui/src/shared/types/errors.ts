import { Data } from "effect";

/// ValidationError represents an error during validation
export class ValidationError extends Data.TaggedError("ValidationError")<{
  readonly reason: string;
}> {}

/// DomainError represents a domain-level error
export class DomainError extends Data.TaggedError("DomainError")<{
  readonly message: string;
  readonly cause?: unknown;
}> {}

/// ApplicationError represents an application-level error
export class ApplicationError extends Data.TaggedError("ApplicationError")<{
  readonly message: string;
  readonly cause?: unknown;
}> {}

/// RpcError represents an error during RPC communication
export class RpcError extends Data.TaggedError("RpcError")<{
  readonly method: string;
  readonly message: string;
  readonly cause?: unknown;
}> {}

/// RpcTimeoutError represents a timeout during RPC communication
export class RpcTimeoutError extends Data.TaggedError("RpcTimeoutError")<{
  readonly method: string;
}> {}

/// ServerError represents an error with the server process
export class ServerError extends Data.TaggedError("ServerError")<{
  readonly message?: string;
  readonly cause?: unknown;
}> {}
