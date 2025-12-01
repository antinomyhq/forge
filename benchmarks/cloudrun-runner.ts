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

    this.logger.info(
      { 
        total_tasks: taskUnits.length,
        max_concurrency: this.config.maxConcurrency 
      },
      "Starting parallel task execution on Cloud Run"
    );

    const executeTask = async (taskUnit: DistributedTaskUnit) => {
      try {
        const result = await this.executeTask(taskUnit, taskYml);
        results.push(result);
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
      }
    };

    // Execute tasks with concurrency limit
    for (const taskUnit of taskUnits) {
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
    }

    // Wait for remaining tasks
    await Promise.all(executing);

    this.logger.info(
      { completed: results.length },
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

      // Build environment variables
      const env = this.buildEnvironment(taskUnit);

      // If provider is set, create both ~/forge/.credentials.json and ~/forge/.config.json
      if (env.FORGE_OVERRIDE_PROVIDER) {
        // Find the API key from environment
        let apiKey: string | undefined;
        const providerLower = env.FORGE_OVERRIDE_PROVIDER.toLowerCase();
        
        if (providerLower.includes('openrouter') || providerLower === 'open_router') {
          apiKey = env.OPENROUTER_API_KEY || env.OPEN_ROUTER_API_KEY;
        } else if (providerLower.includes('anthropic')) {
          apiKey = env.ANTHROPIC_API_KEY;
        } else if (providerLower.includes('openai')) {
          apiKey = env.OPENAI_API_KEY;
        }
        
        if (apiKey) {
          // Create .credentials.json
          const credentialsJson = [{
            id: env.FORGE_OVERRIDE_PROVIDER,
            auth_details: {
              ApiKey: apiKey
            }
          }];
          
          const credStr = JSON.stringify(credentialsJson);
          const credBase64 = Buffer.from(credStr).toString('base64');
          
          // Create .config.json
          const configJson: any = {
            keyInfo: null,
            provider: env.FORGE_OVERRIDE_PROVIDER
          };
          
          if (env.FORGE_OVERRIDE_MODEL) {
            configJson.model = {};
            configJson.model[env.FORGE_OVERRIDE_PROVIDER] = env.FORGE_OVERRIDE_MODEL;
          }
          
          const configStr = JSON.stringify(configJson);
          const configBase64 = Buffer.from(configStr).toString('base64');
          
          // Create both files before running the command
          remoteCommand = `mkdir -p ~/forge && echo '${credBase64}' | base64 -d > ~/forge/.credentials.json && echo '${configBase64}' | base64 -d > ~/forge/.config.json && ${remoteCommand}`;
          
          this.logger.info(
            { 
              task_id: taskUnit.id, 
              provider: env.FORGE_OVERRIDE_PROVIDER,
              model: env.FORGE_OVERRIDE_MODEL,
              has_api_key: !!apiKey,
              credentials_json: credentialsJson,
              config_json: configJson
            },
            "Creating forge credentials and config files"
          );
        }
      }

      this.logger.info(
        {
          task_id: taskUnit.id,
          original_command: command,
          remote_command: remoteCommand,
        },
        "Generated remote command for Cloud Run"
      );

      // Create and execute Cloud Run Job (without waiting for completion)
      const timeout = taskYml.run?.timeout || 180;
      const executionName = await this.orchestrator.createJob(
        taskUnit.id,
        remoteCommand,
        env,
        timeout
      );

      // Wait for execution to complete
      await this.orchestrator.waitForExecution(executionName, taskUnit.id, timeout);

      // Get execution result
      const result = await this.orchestrator.getExecutionResult(taskUnit.id);

      const duration = Date.now() - startTime;
      const output = result.stdout + result.stderr;
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
        validations: [] as ValidationResult[],
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

    // Add overrides from config (API keys, provider, model)
    if (this.config.envOverrides) {
      Object.assign(env, this.config.envOverrides);
    }

    // Add task-specific environment variables
    for (const [key, value] of Object.entries(taskUnit.context)) {
      if (typeof value === "string") {
        env[key] = value;
      }
    }

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

      // Process validations if any
      if (validations.length > 0) {
        const { validationResults, status: validationStatus } =
          processValidations(
            result.output || "",
            validations,
            this.logger,
            index + 1,
            result.duration,
            logFile,
            taskUnit.context
          );

        total_validations += validationResults.length;
        passed_validations += validationResults.filter((v: any) => v.passed).length;

        if (validationStatus === TaskStatus.ValidationFailed) {
          validation_failed++;
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
