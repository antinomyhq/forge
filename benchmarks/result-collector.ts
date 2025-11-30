import type { DaytonaOrchestrator } from "./daytona-orchestrator.js";
import type { RemoteExecutionResult } from "./remote-executor.js";
import type { DistributedTaskUnit } from "./task-distributor.js";
import type { Logger } from "pino";
import type { ValidationResult } from "./verification.js";
import { processValidations } from "./verification.js";
import * as fs from "fs";
import * as path from "path";
import { TaskStatus } from "./model.js";

/**
 * Aggregated result for a task including validation outcomes
 */
export interface AggregatedTaskResult {
  taskId: string;
  workspaceId: string;
  status: TaskStatus;
  command: string;
  duration: number;
  validationResults: ValidationResult[];
  logFile: string;
  debugFile: string | undefined;
}

/**
 * Summary statistics for all tasks
 */
export interface ResultSummary {
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
 * Collects and aggregates results from remote task executions
 */
export class ResultCollector {
  private orchestrator: DaytonaOrchestrator;
  private logger: Logger;
  private debugDir: string;

  constructor(
    orchestrator: DaytonaOrchestrator,
    debugDir: string,
    logger: Logger
  ) {
    this.orchestrator = orchestrator;
    this.debugDir = debugDir;
    this.logger = logger;

    // Ensure debug directory exists
    fs.mkdirSync(debugDir, { recursive: true });
  }

  /**
   * Collects and processes results from a single task execution
   */
  async collectTaskResult(
    executionResult: RemoteExecutionResult,
    taskUnit: DistributedTaskUnit
  ): Promise<AggregatedTaskResult> {
    this.logger.debug(
      {
        task_id: executionResult.taskId,
        workspace_id: executionResult.workspaceId,
      },
      "Collecting task result"
    );

    // Save execution output to log file
    const logFile = path.join(
      this.debugDir,
      `task_run_${taskUnit.index}.log`
    );
    fs.writeFileSync(logFile, executionResult.output, "utf-8");

    // Try to download debug request file if it exists
    const debugRequestFile = path.join(
      this.debugDir,
      `request_${taskUnit.index}.json`
    );

    const remoteDebugPath = taskUnit.context.context_input;
    if (remoteDebugPath) {
      try {
        await this.orchestrator.downloadFile(
          executionResult.workspaceId,
          remoteDebugPath,
          debugRequestFile
        );
      } catch (error) {
        this.logger.debug(
          {
            task_id: executionResult.taskId,
            remote_path: remoteDebugPath,
          },
          "Debug file not available"
        );
      }
    }

    // Process validations
    const { validationResults, status: validationStatus } =
      processValidations(
        executionResult.output,
        taskUnit.validations,
        this.logger,
        taskUnit.index,
        executionResult.duration,
        logFile,
        taskUnit.context
      );

    // Determine final status
    let finalStatus = executionResult.status;
    if (executionResult.isTimeout) {
      finalStatus = TaskStatus.Timeout;
    } else if (
      executionResult.status === TaskStatus.Passed &&
      validationStatus === "validation_failed"
    ) {
      finalStatus = TaskStatus.ValidationFailed;
    }

    const result: AggregatedTaskResult = {
      taskId: executionResult.taskId,
      workspaceId: executionResult.workspaceId,
      status: finalStatus,
      command: taskUnit.command,
      duration: executionResult.duration,
      validationResults,
      logFile,
      debugFile: fs.existsSync(debugRequestFile)
        ? debugRequestFile
        : undefined,
    };

    this.logger.info(
      {
        task_id: result.taskId,
        status: result.status,
        validation_count: validationResults.length,
        passed_validations: validationResults.filter((v) => v.passed)
          .length,
      },
      "Task result collected"
    );

    return result;
  }

  /**
   * Collects results from multiple task executions
   */
  async collectResults(
    executionResults: RemoteExecutionResult[],
    taskUnits: DistributedTaskUnit[]
  ): Promise<AggregatedTaskResult[]> {
    this.logger.info(
      { count: executionResults.length },
      "Collecting all task results"
    );

    const results: AggregatedTaskResult[] = [];

    for (let i = 0; i < executionResults.length; i++) {
      const execResult = executionResults[i];
      const taskUnit = taskUnits[i];
      if (execResult && taskUnit) {
        const result = await this.collectTaskResult(execResult, taskUnit);
        results.push(result);
      }
    }

    this.logger.info(
      { total: results.length },
      "All results collected successfully"
    );

    return results;
  }

  /**
   * Generates summary statistics from aggregated results
   */
  generateSummary(results: AggregatedTaskResult[]): ResultSummary {
    const passedCount = results.filter(
      (r) => r.status === TaskStatus.Passed
    ).length;
    const validationFailedCount = results.filter(
      (r) => r.status === TaskStatus.ValidationFailed
    ).length;
    const timeoutCount = results.filter(
      (r) => r.status === TaskStatus.Timeout
    ).length;
    const failedCount = results.filter(
      (r) => r.status === TaskStatus.Failed
    ).length;

    const totalValidations = results.reduce(
      (sum, r) => sum + r.validationResults.length,
      0
    );
    const passedValidations = results.reduce(
      (sum, r) =>
        sum + r.validationResults.filter((v) => v.passed).length,
      0
    );

    const totalDuration = results.reduce(
      (sum, r) => sum + r.duration,
      0
    );

    return {
      total: results.length,
      passed: passedCount,
      validationFailed: validationFailedCount,
      timeout: timeoutCount,
      failed: failedCount,
      totalDuration,
      validations: {
        total: totalValidations,
        passed: passedValidations,
        failed: totalValidations - passedValidations,
      },
    };
  }

  /**
   * Saves summary to file
   */
  saveSummary(summary: ResultSummary): void {
    const summaryPath = path.join(this.debugDir, "summary.json");
    fs.writeFileSync(summaryPath, JSON.stringify(summary, null, 2));

    this.logger.info(
      { path: summaryPath },
      "Summary saved to file"
    );
  }

  /**
   * Downloads all artifacts from a workspace
   */
  async downloadArtifacts(
    workspaceId: string,
    taskId: string
  ): Promise<void> {
    const artifactPaths = [
      "/tmp/code-forge/target/debug/forge.log",
      "/workspace/forge_debug.json",
      "/tmp/forge_output.log",
    ];

    for (const remotePath of artifactPaths) {
      const filename = path.basename(remotePath);
      const localPath = path.join(
        this.debugDir,
        `${taskId}_${filename}`
      );

      await this.orchestrator.downloadFile(
        workspaceId,
        remotePath,
        localPath
      );
    }
  }
}
