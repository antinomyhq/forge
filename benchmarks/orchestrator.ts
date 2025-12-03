/**
 * Orchestrator interface for executing benchmark tasks on different infrastructure platforms.
 * 
 * This interface provides both high-level and low-level methods:
 * - High-level: runTasks() - Simple API for CLI usage
 * - Low-level: createExecution(), waitForExecution(), etc. - Fine-grained control
 */

import type { DistributedTaskUnit } from './task-distributor.js';
import type { ValidationResult, Validation } from './model.js';

/**
 * Parameters for creating a single execution
 */
export interface CreateExecutionParams {
  taskId: string;
  command: string;
  timeout: number;
  environment: Record<string, string>;
  taskConfig: {
    validations: Validation[];
    context: Record<string, string>;
    earlyExit: boolean;
    cwd?: string;
  };
}

/**
 * Result of a single task execution
 */
export interface TaskResult {
  taskId: string;
  exitCode: number;
  output: string;
  logs: string;
  validations: ValidationResult[];
  duration: number;
  isTimeout: boolean;
  error?: string;
}

/**
 * Parameters for running multiple tasks
 */
export interface RunTasksParams {
  tasks: DistributedTaskUnit[];
  maxConcurrency: number;
  debugDir: string;
  envOverrides?: Record<string, string>;
}

/**
 * Aggregated result of running multiple tasks
 */
export interface RunnerResult {
  passed: number;
  failed: number;
  validation_failed: number;
  timeout: number;
  total: number;
  validationsPassed: number;
  validationsFailed: number;
  validationsTotal: number;
  tasks: Array<{
    id: string;
    status: 'passed' | 'failed' | 'validation_failed' | 'timeout';
    exitCode: number;
    duration: number;
    validations: ValidationResult[];
    error?: string;
  }>;
}

/**
 * Execution orchestrator interface
 * 
 * Implementations handle task execution on specific infrastructure:
 * - CloudRunOrchestrator: Google Cloud Run
 * - LocalOrchestrator: Local machine execution
 * - Future: AWSLambdaOrchestrator, KubernetesOrchestrator, etc.
 */
export interface ExecutionOrchestrator {
  /**
   * High-level method: Runs multiple tasks with parallelism control
   * 
   * This is the primary method called by the CLI. It handles:
   * - Creating executions for all tasks
   * - Managing concurrency (max N tasks at once)
   * - Waiting for all tasks to complete
   * - Aggregating results
   * - Cleanup
   * 
   * @param params Task execution parameters
   * @returns Aggregated results from all tasks
   */
  runTasks(params: RunTasksParams): Promise<RunnerResult>;

  /**
   * Low-level: Creates a single task execution
   * 
   * Starts a task execution on the target infrastructure and returns
   * an execution ID that can be used to track/cancel the execution.
   * 
   * @param params Execution parameters
   * @returns Execution ID (platform-specific identifier)
   */
  createExecution(params: CreateExecutionParams): Promise<string>;

  /**
   * Low-level: Waits for an execution to complete
   * 
   * Polls the execution status and returns when complete or timed out.
   * Retrieves logs and parses results.
   * 
   * @param executionId Platform-specific execution identifier
   * @param timeout Maximum time to wait in seconds
   * @returns Task execution result
   */
  waitForExecution(executionId: string, timeout: number): Promise<TaskResult>;

  /**
   * Low-level: Cancels a running execution
   * 
   * Attempts to stop a running task execution. Used for early exit
   * or when shutting down.
   * 
   * @param executionId Platform-specific execution identifier
   */
  cancelExecution(executionId: string): Promise<void>;

  /**
   * Low-level: Cleans up execution resources
   * 
   * Removes any resources created for the execution (jobs, logs, etc.).
   * Should be called after waitForExecution completes.
   * 
   * @param executionId Platform-specific execution identifier
   */
  cleanup(executionId: string): Promise<void>;
}

/**
 * Simple semaphore for controlling concurrency
 */
export class Semaphore {
  private permits: number;
  private queue: Array<() => void> = [];

  constructor(permits: number) {
    this.permits = permits;
  }

  async acquire(): Promise<void> {
    if (this.permits > 0) {
      this.permits--;
      return;
    }

    return new Promise<void>((resolve) => {
      this.queue.push(resolve);
    });
  }

  release(): void {
    this.permits++;
    const resolve = this.queue.shift();
    if (resolve) {
      this.permits--;
      resolve();
    }
  }
}
