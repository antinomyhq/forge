/**
 * Base orchestrator providing default runTasks() implementation
 * 
 * Platforms can extend this class and only implement the low-level methods:
 * - createExecution()
 * - waitForExecution()
 * - cancelExecution()
 * - cleanup()
 * 
 * The runTasks() method is provided with generic parallelism logic.
 * Platforms can override it for platform-specific optimizations.
 */

import type {
  ExecutionOrchestrator,
  RunTasksParams,
  RunnerResult,
  CreateExecutionParams,
  TaskResult,
} from './orchestrator.js';
import { Semaphore } from './orchestrator.js';
import type { ValidationResult } from './model.js';

export abstract class BaseOrchestrator implements ExecutionOrchestrator {
  // Subclasses must provide a logger
  protected abstract logger: { error: (data: unknown, message: string) => void };

  /**
   * Default implementation of runTasks using low-level methods
   * 
   * This provides generic parallelism control and result aggregation.
   * Platforms can override for optimization if needed.
   */
  async runTasks(params: RunTasksParams): Promise<RunnerResult> {
    const { tasks, maxConcurrency, debugDir, envOverrides = {} } = params;
    const semaphore = new Semaphore(maxConcurrency);
    const results: Array<TaskResult & { taskId: string }> = [];

    // Execute tasks with parallelism control
    await Promise.all(
      tasks.map(async (task, index) => {
        await semaphore.acquire();
        let executionId: string | undefined;
        try {
          executionId = await this.createExecution({
            taskId: task.id,
            command: task.command,
            timeout: task.timeout || 300,
            environment: { ...envOverrides },
            taskConfig: {
              validations: task.validations || [],
              context: task.context || {},
              earlyExit: task.earlyExit || false,
              ...(task.cwd && { cwd: task.cwd }),
            },
          });

          const result = await this.waitForExecution(
            executionId,
            task.timeout || 300
          );

          results.push({ ...result, taskId: task.id });
        } catch (error) {
          // Task failed to execute
          results.push({
            taskId: task.id,
            exitCode: -1,
            output: '',
            logs: '',
            validations: [],
            duration: 0,
            isTimeout: false,
            error: error instanceof Error ? error.message : String(error),
          });
        } finally {
          // Always cleanup, even if execution failed
          if (executionId) {
            try {
              await this.cleanup(executionId);
            } catch (cleanupError) {
              this.logger.error(
                {
                  task_id: task.id,
                  execution_id: executionId,
                  error: cleanupError instanceof Error ? cleanupError.message : String(cleanupError),
                },
                'Failed to cleanup execution'
              );
            }
          }
          semaphore.release();
        }
      })
    );

    // Aggregate results
    return this.aggregateResults(results);
  }

  /**
   * Aggregates individual task results into summary
   */
  protected aggregateResults(results: TaskResult[]): RunnerResult {
    let passed = 0;
    let failed = 0;
    let validation_failed = 0;
    let timeout = 0;
    let validationsPassed = 0;
    let validationsFailed = 0;

    const tasks = results.map((result) => {
      const taskValidationsPassed = result.validations.filter(
        (v) => v.passed
      ).length;
      const taskValidationsFailed = result.validations.filter(
        (v) => !v.passed
      ).length;

      validationsPassed += taskValidationsPassed;
      validationsFailed += taskValidationsFailed;

      let status: 'passed' | 'failed' | 'validation_failed' | 'timeout';

      if (result.isTimeout) {
        timeout++;
        status = 'timeout';
      } else if (result.exitCode !== 0) {
        failed++;
        status = 'failed';
      } else if (taskValidationsFailed > 0) {
        validation_failed++;
        status = 'validation_failed';
      } else {
        passed++;
        status = 'passed';
      }

      return {
        id: result.taskId,
        status,
        exitCode: result.exitCode,
        duration: result.duration,
        validations: result.validations,
        ...(result.error ? { error: result.error } : {}),
      };
    });

    return {
      passed,
      failed,
      validation_failed,
      timeout,
      total: results.length,
      validationsPassed,
      validationsFailed,
      validationsTotal: validationsPassed + validationsFailed,
      tasks,
    };
  }

  // Abstract methods that platforms must implement
  abstract createExecution(params: CreateExecutionParams): Promise<string>;
  abstract waitForExecution(
    executionId: string,
    timeout: number
  ): Promise<TaskResult>;
  abstract cancelExecution(executionId: string): Promise<void>;
  abstract cleanup(executionId: string): Promise<void>;
}
