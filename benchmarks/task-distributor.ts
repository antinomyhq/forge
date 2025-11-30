import type { Task, Validation } from "./model.js";
import type { Logger } from "pino";

/**
 * Represents a single unit of work to be executed on a Daytona workspace
 */
export interface DistributedTaskUnit {
  id: string;
  index: number;
  command: string;
  context: Record<string, string>;
  validations: Validation[];
  timeout: number | undefined;
  cwd: string | undefined;
  debugDir: string;
}

/**
 * Configuration for task distribution
 */
export interface DistributionConfig {
  maxConcurrency: number;
  debugDir: string;
}

/**
 * Distributes evaluation tasks into individual units for parallel execution
 */
export class TaskDistributor {
  private logger: Logger;

  constructor(logger: Logger) {
    this.logger = logger;
  }

  /**
   * Splits a task and its data sources into individual executable units
   */
  createTaskUnits(
    task: Task,
    contexts: Record<string, string>[],
    debugDir: string
  ): DistributedTaskUnit[] {
    this.logger.info(
      { count: contexts.length },
      "Creating distributed task units"
    );

    const units: DistributedTaskUnit[] = contexts.map((context, index) => {
      const id = `task-${index + 1}`;
      // Use /tmp for remote path since it's accessible on the workspace
      const debugRequestFile = `/tmp/debug/request_${index + 1}.json`;

      return {
        id,
        index: index + 1,
        command: task.run.command,
        context: {
          ...context,
          context_input: debugRequestFile, // Remote path for FORGE_DEBUG_REQUESTS
        },
        validations: task.validations || [],
        timeout: task.run.timeout,
        cwd: task.run.cwd,
        debugDir, // Keep local debugDir for result collection
      };
    });

    this.logger.info(
      { total_units: units.length },
      "Task units created successfully"
    );

    return units;
  }

  /**
   * Groups task units into batches for parallel execution
   */
  createBatches(
    units: DistributedTaskUnit[],
    batchSize: number
  ): DistributedTaskUnit[][] {
    const batches: DistributedTaskUnit[][] = [];

    for (let i = 0; i < units.length; i += batchSize) {
      batches.push(units.slice(i, i + batchSize));
    }

    this.logger.info(
      {
        total_units: units.length,
        batch_size: batchSize,
        total_batches: batches.length,
      },
      "Created task batches"
    );

    return batches;
  }

  /**
   * Validates task distribution configuration
   */
  validateConfig(config: DistributionConfig): void {
    if (config.maxConcurrency <= 0) {
      throw new Error(
        `Invalid maxConcurrency: ${config.maxConcurrency}. Must be > 0`
      );
    }

    if (!config.debugDir) {
      throw new Error("debugDir is required");
    }
  }

  /**
   * Calculates estimated resource requirements
   */
  estimateResources(
    units: DistributedTaskUnit[],
    avgExecutionTimeSeconds: number = 60
  ): {
    totalTasks: number;
    estimatedDuration: number;
    estimatedWorkspaces: number;
  } {
    const totalTasks = units.length;
    const estimatedWorkspaces = Math.min(
      totalTasks,
      10 // Default max concurrent workspaces
    );
    const estimatedDuration = Math.ceil(
      (totalTasks / estimatedWorkspaces) * avgExecutionTimeSeconds
    );

    return {
      totalTasks,
      estimatedDuration,
      estimatedWorkspaces,
    };
  }
}
