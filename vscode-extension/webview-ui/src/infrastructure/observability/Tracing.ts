import { Effect, Metric, MetricBoundaries, MetricLabel } from 'effect';

/**
 * Observability utilities for tracing and metrics
 */

// Define metrics for tracking
export const messageLatency = Metric.histogram(
  'message_latency',
  MetricBoundaries.linear({ start: 10, width: 100, count: 10 })
);

export const rpcCallCount = Metric.counter('rpc_calls_total');

export const rpcErrorCount = Metric.counter('rpc_errors_total');

export const stateUpdateCount = Metric.counter('state_updates_total');

export const streamingDuration = Metric.histogram(
  'streaming_duration',
  MetricBoundaries.linear({ start: 100, width: 1000, count: 10 })
);

/**
 * Wrap an Effect with tracing span
 */
export const withSpan = <A, E, R>(
  name: string,
  effect: Effect.Effect<A, E, R>,
  attributes?: Record<string, string | number | boolean>
): Effect.Effect<A, E, R> => {
  return Effect.withSpan(effect, name, { attributes });
};

/**
 * Track RPC call with metrics and tracing
 */
export const trackRpcCall = <A, E, R>(
  method: string,
  effect: Effect.Effect<A, E, R>
): Effect.Effect<A, E, R> => {
  return Effect.gen(function* () {
    const startTime = Date.now();

    // Increment RPC call counter
    yield* Metric.increment(rpcCallCount);

    // Create span for tracing
    const result = yield* withSpan(`rpc.${method}`, effect, {
      'rpc.method': method,
      'rpc.system': 'vscode',
    }).pipe(
      Effect.tapError(() =>
        Effect.gen(function* () {
          // Increment error counter on failure
          yield* Metric.increment(rpcErrorCount);

          // Add error span event
          yield* Effect.logError(`RPC call failed: ${method}`);
        })
      )
    );

    // Record latency
    const duration = Date.now() - startTime;
    yield* Metric.update(messageLatency, duration);

    yield* Effect.log(`RPC call completed: ${method} (${duration}ms)`);

    return result;
  });
};

/**
 * Track state update with metrics
 */
export const trackStateUpdate = <A, E, R>(
  updateType: string,
  effect: Effect.Effect<A, E, R>
): Effect.Effect<A, E, R> => {
  return Effect.gen(function* () {
    yield* Metric.increment(stateUpdateCount);

    const result = yield* withSpan(`state.${updateType}`, effect, {
      'state.update_type': updateType,
    });

    yield* Effect.logDebug(`State updated: ${updateType}`);

    return result;
  });
};

/**
 * Track streaming session
 */
export const trackStreaming = <A, E, R>(
  effect: Effect.Effect<A, E, R>
): Effect.Effect<A, E, R> => {
  return Effect.gen(function* () {
    const startTime = Date.now();

    const result = yield* withSpan('streaming.session', effect, {
      'streaming.type': 'llm_response',
    });

    const duration = Date.now() - startTime;
    yield* Metric.update(streamingDuration, duration);

    yield* Effect.log(`Streaming completed (${duration}ms)`);

    return result;
  });
};

/**
 * Create a tagged metric for specific operations
 */
export const withMetricLabels = (
  labels: Record<string, string>
): MetricLabel.MetricLabel[] => {
  return Object.entries(labels).map(([key, value]) => MetricLabel.make(key, value));
};

/**
 * Log performance metrics to console
 */
export const logMetrics = (): Effect.Effect<void> =>
  Effect.gen(function* () {
    // Get current metric values
    const metrics = {
      rpcCalls: yield* Metric.value(rpcCallCount),
      rpcErrors: yield* Metric.value(rpcErrorCount),
      stateUpdates: yield* Metric.value(stateUpdateCount),
    };

    yield* Effect.log('Performance Metrics:', JSON.stringify(metrics, null, 2));
  });

/**
 * Enable development mode tracing with detailed logs
 */
export const enableDevTracing = (): Effect.Effect<void> =>
  Effect.gen(function* () {
    yield* Effect.log('[Observability] Development tracing enabled');
    yield* Effect.log('[Observability] Metrics: RPC calls, errors, state updates, latency');
    yield* Effect.log('[Observability] Tracing: All operations instrumented with spans');
  });

/**
 * Create a tracer span with automatic timing
 */
export const tracedOperation = <A, E, R>(
  operationName: string,
  operation: Effect.Effect<A, E, R>,
  metadata?: Record<string, unknown>
): Effect.Effect<A, E, R> => {
  return Effect.gen(function* () {
    const startTime = Date.now();

    yield* Effect.logInfo(`[Trace] Starting: ${operationName}`);

    const result = yield* withSpan(operationName, operation, {
      ...(metadata as Record<string, string | number | boolean>),
      timestamp: startTime,
    }).pipe(
      Effect.tap(() =>
        Effect.gen(function* () {
          const duration = Date.now() - startTime;
          yield* Effect.logInfo(
            `[Trace] Completed: ${operationName} (${duration}ms)`
          );
        })
      ),
      Effect.tapError((error) =>
        Effect.gen(function* () {
          const duration = Date.now() - startTime;
          yield* Effect.logError(
            `[Trace] Failed: ${operationName} (${duration}ms) - ${error}`
          );
        })
      )
    );

    return result;
  });
};
