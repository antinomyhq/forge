import type { Task } from "./model.js";
import type { Logger } from "pino";
import { DaytonaOrchestrator } from "./daytona-orchestrator.js";
import { RemoteBuilder } from "./remote-builder.js";
import { TaskDistributor } from "./task-distributor.js";
import { RemoteExecutor } from "./remote-executor.js";
import { ResultCollector } from "./result-collector.js";
import { WorkspacePool } from "./workspace-pool.js";
import { getContextsFromSources } from "./command-generator.js";
import * as path from "path";

/**
 * Configuration for distributed execution
 */
export interface DistributedConfig {
  daytonaApiKey: string;
  sourcePath: string;
  debugDir: string;
  maxConcurrency: number;
  apiKey?: string;
  provider?: string;
  model?: string;
}

/**
 * Result from distributed execution
 */
export interface DistributedResult {
  total: number;
  passed: number;
  validationFailed: number;
  timeout: number;
  failed: number;
  totalDuration: number;
  validations: {
    total: number;
    passed: number;
    failed: number;
  };
}

/**
 * Orchestrates distributed execution of evaluation tasks on Daytona
 */
export class DistributedRunner {
  private logger: Logger;

  constructor(logger: Logger) {
    this.logger = logger;
  }

  /**
   * Runs the evaluation in distributed mode
   */
  async run(
    task: Task,
    sourcesData: Record<string, string>[][],
    config: DistributedConfig
  ): Promise<DistributedResult> {
    this.logger.info("Starting distributed evaluation");

    // Initialize components
    const orchestrator = new DaytonaOrchestrator(
      { apiKey: config.daytonaApiKey },
      this.logger,
      config.sourcePath
    );

    // Build custom Docker image with forge binary (one-time setup)
    this.logger.info("Building custom Docker image with forge binary...");
    await orchestrator.buildCustomImage();
    this.logger.info("Custom image ready");

    const distributor = new TaskDistributor(this.logger);

    // Prepare environment overrides from config
    const envOverrides: Record<string, string> = {};
    if (config.apiKey) {
      // Try to infer which API key to set based on provider
      const provider = config.provider?.toLowerCase();
      if (provider?.includes("anthropic")) {
        envOverrides["ANTHROPIC_API_KEY"] = config.apiKey;
      } else if (provider?.includes("openai")) {
        envOverrides["OPENAI_API_KEY"] = config.apiKey;
      } else if (provider?.includes("openrouter")) {
        envOverrides["OPENROUTER_API_KEY"] = config.apiKey;
      } else if (provider?.includes("gemini")) {
        envOverrides["GEMINI_API_KEY"] = config.apiKey;
      } else if (provider?.includes("groq")) {
        envOverrides["GROQ_API_KEY"] = config.apiKey;
      } else {
        // Default to ANTHROPIC_API_KEY if no provider specified
        envOverrides["ANTHROPIC_API_KEY"] = config.apiKey;
      }
    }
    if (config.provider) {
      envOverrides["FORGE_OVERRIDE_PROVIDER"] = config.provider;
    }
    if (config.model) {
      envOverrides["FORGE_OVERRIDE_MODEL"] = config.model;
    }

    const executor = new RemoteExecutor(
      orchestrator,
      "/usr/local/bin/forge", // Binary is in the custom image
      this.logger,
      envOverrides
    );

    const collector = new ResultCollector(
      orchestrator,
      config.debugDir,
      this.logger
    );

    const pool = new WorkspacePool(
      orchestrator,
      null, // No builder needed - binary is in the image
      {
        maxConcurrency: config.maxConcurrency,
        maxRetries: 0, // No retries for testing
        retryDelayMs: 2000,
        enableGracefulShutdown: true,
      },
      this.logger
    );

    try {
      // Get contexts from sources
      const contexts = getContextsFromSources(sourcesData);

      // Create task units
      const taskUnits = distributor.createTaskUnits(
        task,
        contexts,
        config.debugDir
      );

      // Estimate resources
      const estimate = distributor.estimateResources(taskUnits);
      this.logger.info(
        {
          total_tasks: estimate.totalTasks,
          estimated_workspaces: estimate.estimatedWorkspaces,
          estimated_duration_seconds: estimate.estimatedDuration,
        },
        "Resource estimation"
      );

      // Create workspace pool
      const workspaceCount = Math.min(
        taskUnits.length,
        config.maxConcurrency
      );

      const workspaceIds = await pool.createWorkspaces(workspaceCount);

      // Check if we have enough workspaces
      if (workspaceIds.length === 0) {
        throw new Error("Failed to create any workspaces");
      }

      this.logger.info(
        {
          workspaces_created: workspaceIds.length,
          tasks_to_execute: taskUnits.length,
        },
        "Workspace pool ready"
      );

      // Execute tasks (executor will reuse workspaces if needed)
      this.logger.info("Executing tasks on remote workspaces");

      const executionResults = await executor.executeTasks(
        taskUnits,
        workspaceIds,
        config.maxConcurrency
      );

      // Collect results
      const aggregatedResults = await collector.collectResults(
        executionResults,
        taskUnits
      );

      // Generate summary
      const summary = collector.generateSummary(aggregatedResults);
      collector.saveSummary(summary);

      // Log summary
      this.logger.info(
        {
          total: summary.total,
          passed: summary.passed,
          validation_failed: summary.validationFailed,
          timeout: summary.timeout,
          failed: summary.failed,
          total_duration_ms: summary.totalDuration,
          validations: summary.validations,
        },
        "Distributed evaluation completed"
      );

      // Cleanup
      await pool.destroyAll();

      return summary;
    } catch (error) {
      this.logger.error(
        {
          error: error instanceof Error ? error.message : String(error),
        },
        "Distributed evaluation failed"
      );

      // Ensure cleanup on error
      try {
        await pool.destroyAll();
      } catch (cleanupError) {
        this.logger.error(
          {
            error:
              cleanupError instanceof Error
                ? cleanupError.message
                : String(cleanupError),
          },
          "Cleanup failed"
        );
      }

      throw error;
    }
  }
}
