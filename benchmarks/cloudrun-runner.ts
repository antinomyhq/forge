import type { Logger } from "pino";
import { CloudRunOrchestrator } from "./cloudrun-orchestrator.js";
import type { CloudRunConfig } from "./cloudrun-orchestrator.js";
import { TaskDistributor } from "./task-distributor.js";
import type { DistributedTaskUnit } from "./task-distributor.js";
import { TaskStatus } from "./model.js";
import type { ValidationResult } from "./model.js";
import { generateCommand } from "./command-generator.js";
import { processValidations } from "./verification.js";
import * as fs from "fs";
import * as path from "path";
import * as os from "os";
import { execSync } from "child_process";

/**
 * Configuration for Cloud Run distributed execution
 */
export interface CloudRunRunnerConfig {
  cloudRun: CloudRunConfig;
  maxConcurrency: number;
  debugDir: string;
  envOverrides?: Record<string, string>;
}

/**
 * Result from distributed execution on Cloud Run
 */
export interface CloudRunExecutionSummary {
  total: number;
  passed: number;
  failed: number;
  timeout: number;
  validation_failed: number;
  total_duration_ms: number;
  validations: {
    total: number;
    passed: number;
    failed: number;
  };
}

/**
 * Executes evaluation tasks in parallel on Google Cloud Run
 * 
 * Each task runs as a separate Cloud Run Job execution with proper isolation.
 * Results are collected from Cloud Logging and validated.
 */
export class CloudRunRunner {
  private logger: Logger;
  private config: CloudRunRunnerConfig;
  private orchestrator: CloudRunOrchestrator;
  private distributor: TaskDistributor;

  constructor(config: CloudRunRunnerConfig, logger: Logger) {
    this.config = config;
    this.logger = logger;
    this.orchestrator = new CloudRunOrchestrator(config.cloudRun, logger);
    this.distributor = new TaskDistributor(logger);
    
    // Ensure debug directory exists
    fs.mkdirSync(config.debugDir, { recursive: true });
  }

  /**
   * Runs the evaluation on Cloud Run
   */
  async run(
    taskYml: any,
    sources: any[]
  ): Promise<CloudRunExecutionSummary> {
    const startTime = Date.now();

    this.logger.info({ sources_count: sources.length }, "Starting Cloud Run evaluation");

    try {
      // Create task units
      this.logger.info("Creating distributed task units for Cloud Run");
      // Flatten sources - each source can have multiple values
      const contexts: Record<string, string>[] = [];
      for (const source of sources) {
        if (Array.isArray(source)) {
          contexts.push(...source);
        } else {
          contexts.push(source);
        }
      }
      
      const debugDir = path.join(
        taskYml.eval_dir,
        "debug",
        new Date().toISOString().replace(/[:.]/g, "-")
      );
      fs.mkdirSync(debugDir, { recursive: true });
      
      const taskUnits = this.distributor.createTaskUnits(taskYml, contexts, debugDir);
      this.logger.info(
        { count: taskUnits.length },
        "Task units created successfully"
      );

      // Execute tasks in parallel (with concurrency limit)
      const results = await this.executeTasksInParallel(taskUnits, taskYml);

      // Collect and validate results inline
      const summary = this.collectAndValidateResults(
        results,
        taskUnits,
        taskYml.validations || []
      );

      const duration = Date.now() - startTime;

      this.logger.info(
        {
          total: summary.total,
          passed: summary.passed,
          validation_failed: summary.validation_failed,
          timeout: summary.timeout,
          failed: summary.failed,
          total_duration_ms: duration,
          validations: summary.validations,
        },
        "Cloud Run evaluation completed"
      );

      return {
        ...summary,
        total_duration_ms: duration,
      };
    } finally {
      // Cleanup all jobs
      await this.orchestrator.cleanup();
    }
  }

  /**
   * Executes tasks in parallel with concurrency control
   */
  private async executeTasksInParallel(
    taskUnits: DistributedTaskUnit[],
    taskYml: any
  ): Promise<any[]> {
    const results: any[] = [];
    const executing: Promise<void>[] = [];
    let index = 0;
    let shouldStopLaunching = false;

    this.logger.info(
      { 
        total_tasks: taskUnits.length,
        max_concurrency: this.config.maxConcurrency,
        early_exit: taskYml.early_exit || false
      },
      "Starting parallel task execution on Cloud Run"
    );

    const executeTask = async (taskUnit: DistributedTaskUnit) => {
      try {
        const result = await this.executeTask(taskUnit, taskYml);
        results.push(result);
        
        // Check if early exit is enabled and this task failed validation
        if (taskYml.early_exit && result.status === TaskStatus.ValidationFailed) {
          this.logger.info(
            { task_id: taskUnit.id },
            "Early exit: Task failed validation, will stop launching new tasks"
          );
          shouldStopLaunching = true;
        }
      } catch (error) {
        this.logger.error(
          {
            task_id: taskUnit.id,
            error: error instanceof Error ? error.message : String(error),
          },
          "Task execution failed"
        );
        results.push({
          taskId: taskUnit.id,
          status: TaskStatus.Failed,
          exitCode: -1,
          output: "",
          stderr: error instanceof Error ? error.message : String(error),
          duration: 0,
          error: error instanceof Error ? error.message : String(error),
        });
        
        // Stop launching new tasks on error if early exit is enabled
        if (taskYml.early_exit) {
          shouldStopLaunching = true;
        }
      }
    };

    // Execute tasks with concurrency limit
    for (const taskUnit of taskUnits) {
      // Stop launching new tasks if early exit triggered
      if (shouldStopLaunching) {
        this.logger.info(
          { remaining_tasks: taskUnits.length - index },
          "Early exit: Skipping remaining tasks"
        );
        break;
      }
      
      // Wait for a slot if we're at max concurrency
      if (executing.length >= this.config.maxConcurrency) {
        await Promise.race(executing);
      }

      const promise = executeTask(taskUnit).then(() => {
        const index = executing.indexOf(promise);
        if (index > -1) {
          executing.splice(index, 1);
        }
      });
      executing.push(promise);
      index++;
    }

    // Wait for remaining tasks
    await Promise.all(executing);

    this.logger.info(
      { completed: results.length, total: taskUnits.length },
      "All tasks completed on Cloud Run"
    );

    return results;
  }

  /**
   * Executes a single task on Cloud Run
   */
  private async executeTask(
    taskUnit: DistributedTaskUnit,
    taskYml: any
  ): Promise<any> {
    const startTime = Date.now();

    this.logger.info({ task_id: taskUnit.id }, "Executing task on Cloud Run");

    try {
      // Generate command
      const command = generateCommand(taskUnit.command, taskUnit.context);
      
      // Replace local forge path with container path
      let remoteCommand = command
        .replace(/\.\.\/\.\.\/target\/(debug|release)\/forge/g, "/usr/local/bin/forge")
        .replace(/-C\s+\.\.\/\.\./g, "-C /tmp/code-forge");

      // Prepend provider/model overrides to the command if they're passed
      if (this.config.envOverrides) {
        const envPrefix = Object.entries(this.config.envOverrides)
          .filter(([key]) => key.startsWith('FORGE_OVERRIDE_'))
          .map(([key, value]) => `${key}=${value}`)
          .join(' ');
        
        if (envPrefix) {
          remoteCommand = `${envPrefix} ${remoteCommand}`;
        }
      }

      // Build environment variables (without FORGE_OVERRIDE_* since they're in the command)
      const env = this.buildEnvironment(taskUnit);

      this.logger.info(
        {
          task_id: taskUnit.id,
          original_command: command,
          remote_command: remoteCommand,
          early_exit: taskYml.early_exit || false,
        },
        "Generated remote command for Cloud Run"
      );

      // Create and execute Cloud Run Job (without waiting for completion)
      const timeout = taskYml.run?.timeout || taskYml.timeout || 180;
      
      // Prepare task config for container
      const taskConfig = {
        early_exit: taskYml.early_exit || false,
        validations: taskYml.validations || [],
        context: taskUnit.context || {},
      };
      
      const executionName = await this.orchestrator.createJob(
        taskUnit.id,
        remoteCommand,
        env,
        timeout,
        taskConfig
      );

      // Wait for execution to complete
      await this.orchestrator.waitForExecution(executionName, taskUnit.id, timeout);

      // Get execution result
      const result = await this.orchestrator.getExecutionResult(taskUnit.id);

      const duration = Date.now() - startTime;
      let output = result.logs || (result.stdout + result.stderr);
      
      // If we have JSON result, use it directly (no need for log extraction)
      if (result.jsonResult) {
        output = result.jsonResult.logs || result.jsonResult.output || output;
      }
      
      const status =
        result.exitCode === 0 ? TaskStatus.Passed : TaskStatus.Failed;

      this.logger.info(
        {
          task_id: taskUnit.id,
          status,
          duration_ms: duration,
          exit_code: result.exitCode,
          output_length: output.length,
          stderr_length: result.stderr.length,
          output_preview: output.slice(0, 200),
        },
        "Task execution completed on Cloud Run"
      );

      return {
        taskId: taskUnit.id,
        status,
        exitCode: result.exitCode,
        output,
        stderr: result.stderr,
        duration,
        validations: result.jsonResult?.validations || [],
        logs: result.logs,
      };
    } catch (error) {
      const duration = Date.now() - startTime;
      const errorMessage =
        error instanceof Error ? error.message : String(error);

      this.logger.error(
        {
          task_id: taskUnit.id,
          error: errorMessage,
          duration_ms: duration,
        },
        "Task execution failed on Cloud Run"
      );

      return {
        taskId: taskUnit.id,
        status: TaskStatus.Failed,
        exitCode: -1,
        output: "",
        stderr: errorMessage,
        duration,
        error: errorMessage,
      };
    }
  }

  /**
   * Builds environment variables for task execution
   */
  private buildEnvironment(taskUnit: DistributedTaskUnit): Record<string, string> {
    const env: Record<string, string> = {};

    // Add all overrides from config (API keys and non-FORGE_OVERRIDE_* vars go to env)
    if (this.config.envOverrides) {
      for (const [key, value] of Object.entries(this.config.envOverrides)) {
        // FORGE_OVERRIDE_* variables are prepended to the command
        // API keys and other env vars are passed as environment variables
        if (!key.startsWith('FORGE_OVERRIDE_')) {
          env[key] = value;
        }
      }
    }

    // Note: taskUnit.context is now passed via TASK_CONTEXT in orchestrator, not as individual env vars

    return env;
  }

  /**
   * Collects and validates results from task executions
   */
  private collectAndValidateResults(
    results: any[],
    taskUnits: DistributedTaskUnit[],
    validations: any[]
  ): any {
    let passed = 0;
    let failed = 0;
    let timeout = 0;
    let validation_failed = 0;
    let total_validations = 0;
    let passed_validations = 0;

    // Save results to files
    results.forEach((result, index) => {
      const taskUnit = taskUnits[index];
      
      // Write log file
      const logFile = path.join(
        this.config.debugDir,
        `task_run_${index + 1}.log`
      );
      fs.writeFileSync(logFile, result.output || result.logs || "", "utf-8");

      // Use validation results from the container (already processed by task-executor)
      if (result.validations && result.validations.length > 0) {
        total_validations += result.validations.length;
        passed_validations += result.validations.filter((v: any) => v.passed).length;
        
        // Check if any validations failed
        const hasFailedValidations = result.validations.some((v: any) => !v.passed);
        if (hasFailedValidations && result.exitCode === 0) {
          // Task succeeded but validations failed
          result.status = TaskStatus.ValidationFailed;
        }
      }

      // Count statuses
      if (result.status === TaskStatus.Passed) {
        passed++;
      } else if (result.status === TaskStatus.Timeout) {
        timeout++;
      } else if (result.status === TaskStatus.ValidationFailed) {
        validation_failed++;
      } else {
        failed++;
      }
    });

    // Write summary
    const summary = {
      total: results.length,
      passed,
      failed,
      timeout,
      validation_failed,
      validations: {
        total: total_validations,
        passed: passed_validations,
        failed: total_validations - passed_validations,
      },
    };

    const summaryFile = path.join(this.config.debugDir, "summary.json");
    fs.writeFileSync(summaryFile, JSON.stringify(summary, null, 2), "utf-8");

    this.logger.info(
      { path: summaryFile },
      "Summary saved to file"
    );

    return summary;
  }
}
