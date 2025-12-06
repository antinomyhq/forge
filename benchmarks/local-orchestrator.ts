/**
 * Local orchestrator for running tasks on the local machine
 * 
 * Uses the existing task-executor to run tasks locally with the same
 * validation and early-exit logic as Cloud Run.
 */

import { BaseOrchestrator } from './base-orchestrator.js';
import type {
  CreateExecutionParams,
  TaskResult,
} from './orchestrator.js';
import { executeTask } from './task-executor.js';
import { writeFileSync } from 'fs';
import { join, dirname } from 'node:path';
import type { Task, Validation } from './model.js';

export class LocalOrchestrator extends BaseOrchestrator {
  protected logger = {
    error: (data: unknown, message: string) => console.error(`[LocalOrchestrator] ${message}`, data)
  };
  
  private executions = new Map<string, { logFile: string; promise: Promise<TaskResult> }>();

  async createExecution(params: CreateExecutionParams): Promise<string> {
    const executionId = `local-${params.taskId}-${Date.now()}`;
    const logFile = join(process.cwd(), 'debug', `${params.taskId}.log`);

    // Ensure debug directory exists
    const { mkdirSync } = await import('fs');
    const { dirname } = await import('path');
    mkdirSync(dirname(logFile), { recursive: true });

    // Create task object for executeTask
    const task: Task = {
      run: params.command,
      timeout: params.timeout,
      validations: params.taskConfig.validations,
      early_exit: params.taskConfig.earlyExit,
      before_run: [],
      sources: [],
    };

    // Write context files if needed
    if (params.taskConfig.context.context_input) {
      const contextPath = params.taskConfig.context.context_input as string;
      // Context input file would be written here if needed
    }

    // Start execution (don't await yet)
    const promise = this.executeTaskAsync(
      params.taskId,
      params.command,
      task,
      logFile,
      params.taskConfig.context,
      params.environment,
      params.taskConfig.cwd
    );

    this.executions.set(executionId, { logFile, promise });
    return executionId;
  }

  private async executeTaskAsync(
    taskId: string,
    command: string,
    task: Task,
    logFile: string,
    context: Record<string, string>,
    environment: Record<string, string>,
    cwd?: string
  ): Promise<TaskResult> {

    // Set environment variables
    const originalEnv = { ...process.env };
    Object.assign(process.env, environment);

    try {
      // Use the provided cwd, or task.cwd, or calculate from debug directory
      const evalDir = cwd || task.cwd || dirname(logFile);
      
      const result = await executeTask(
        command,
        1, // index
        logFile,
        evalDir,
        task,
        context,
        false, // append
        false // jsonOutput
      );



      const taskResult: TaskResult = {
        taskId,
        exitCode: result.exitCode ?? -1,
        output: result.output || '',
        logs: result.output || '',
        validations: result.validations || [],
        duration: result.duration,
        isTimeout: result.isTimeout || false,
      };
      
      if (result.error) {
        taskResult.error = result.error;
      }
      
      return taskResult;
    } finally {
      // Restore environment
      process.env = originalEnv;
    }
  }

  async waitForExecution(executionId: string, timeout: number): Promise<TaskResult> {
    const execution = this.executions.get(executionId);
    if (!execution) {
      throw new Error(`Execution not found: ${executionId}`);
    }

    // Wait for the promise with timeout
    const timeoutPromise = new Promise<TaskResult>((_, reject) => {
      setTimeout(() => reject(new Error('Execution timed out')), timeout * 1000);
    });

    try {
      return await Promise.race([execution.promise, timeoutPromise]);
    } catch (error) {
      return {
        taskId: executionId,
        exitCode: -1,
        output: '',
        logs: '',
        validations: [],
        duration: timeout,
        isTimeout: true,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async cancelExecution(executionId: string): Promise<void> {
    // Local execution can't be easily cancelled in Node.js
    // This is a no-op for local orchestrator
    this.executions.delete(executionId);
  }

  async cleanup(executionId: string): Promise<void> {
    // Clean up execution from map
    this.executions.delete(executionId);
  }
}
