import { Effect } from "effect";
import { JsonRpcService } from "./JsonRpcService";
import { RpcError } from "@shared/types/errors";

/// Starts a new thread and returns the thread ID
export const startThread = (): Effect.Effect<
  { threadId: string },
  RpcError,
  JsonRpcService
> =>
  Effect.gen(function* () {
    const rpc = yield* JsonRpcService;
    return yield* rpc.request<{ threadId: string }>("thread/start", {});
  });

/// Sends a message to start a new turn in a conversation
///
/// # Arguments
/// - threadId: The ID of the thread
/// - message: The message content
/// - files: Optional array of file paths to attach
export const sendMessage = (
  threadId: string,
  message: string,
  files?: string[]
): Effect.Effect<void, RpcError, JsonRpcService> =>
  Effect.gen(function* () {
    const rpc = yield* JsonRpcService;
    yield* rpc.request("turn/start", {
      threadId,
      turnId: crypto.randomUUID(),
      message,
      files: files && files.length > 0 ? files : undefined,
    });
  });

/// Lists all threads
export const listThreads = (): Effect.Effect<
  Array<{ threadId: string; title?: string; updatedAt?: string }>,
  RpcError,
  JsonRpcService
> =>
  Effect.gen(function* () {
    const rpc = yield* JsonRpcService;
    return yield* rpc.request<Array<{ threadId: string; title?: string; updatedAt?: string }>>(
      "thread/list",
      {}
    );
  });

/// Gets a thread by ID
export const getThread = (threadId: string): Effect.Effect<
  { threadId: string; messages: Array<unknown> },
  RpcError,
  JsonRpcService
> =>
  Effect.gen(function* () {
    const rpc = yield* JsonRpcService;
    return yield* rpc.request<{ threadId: string; messages: Array<unknown> }>("thread/get", { threadId });
  });
