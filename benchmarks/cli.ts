#!/usr/bin/env node

// Handle EPIPE errors gracefully (e.g., when piping to `head` or `jq` that closes early)
process.stdout.on("error", (error: NodeJS.ErrnoException) => {
  if (error.code === "EPIPE") {
    process.exit(0);
  }
  throw error;
});

import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";
import { parse as parseYaml } from "yaml";
import { parse as parseCsv } from "csv-parse/sync";
import { execSync } from "child_process";
import pLimit from "p-limit";
import pino from "pino";
import { TaskStatus, type Task } from "./model.js";
import {
  getContextsFromSources,
  generateCommand,
} from "./command-generator.js";
import { parseCliArgs } from "./parse.js";
import { executeTask, type TaskExecutionResult } from "./task-executor.js";
import { processValidations, type ValidationResult } from "./verification.js";

export type TaskResult = {
  index: number;
  status: TaskStatus;
  command: string;
  duration: number;
  validationResults: ValidationResult[];
};

// ESM compatibility for __dirname
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

/**
 * Create logger instance
 * - Human-readable CLI output by default
 * - Set LOG_JSON=1 for machine-readable JSON output (for piping to jq, log aggregators, etc.)
 */
const logger =
  process.env.LOG_JSON === "1"
    ? pino({
        level: process.env.LOG_LEVEL || "info",
        formatters: {
          level: (label) => ({ level: label }),
        },
        timestamp: pino.stdTimeFunctions.isoTime,
      })
    : pino({
        level: process.env.LOG_LEVEL || "info",
        transport: {
          target: "pino-pretty",
          options: {
            colorize: true,
            translateTime: "HH:MM:ss",
            ignore: "pid,hostname",
            messageFormat: "{msg}",
          },
        },
        formatters: {
          level: (label) => ({ level: label }),
        },
        timestamp: pino.stdTimeFunctions.isoTime,
      });

async function main() {
  // Parse command line arguments
  let args;
  try {
    args = await parseCliArgs(__dirname);
  } catch (error) {
    const message = error instanceof Error ? error.message : "Unknown error";
    logger.error({ error: message }, "Failed to parse CLI arguments");
    process.exit(1);
  }

  const { evalName, evalDir, taskFile } = args;

  // Check if eval directory and task file exist
  if (!fs.existsSync(evalDir)) {
    logger.error({ evalDir }, "Eval directory not found");
    process.exit(1);
  }

  if (!fs.existsSync(taskFile)) {
    logger.error({ evalDir }, "task.yml not found");
    process.exit(1);
  }

  // Read and parse task.yml
  const taskContent = fs.readFileSync(taskFile, "utf-8");
  const task: Task = parseYaml(taskContent);

  // Display header
  const displayName = path.relative(__dirname, evalDir) || evalName;

  // Create debug directory with timestamp
  const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
  const debugDir = path.join(evalDir, "debug", timestamp);
  fs.mkdirSync(debugDir, { recursive: true });

  // Execute before_run commands
  if (task.before_run && task.before_run.length > 0) {
    for (const cmd of task.before_run) {
      try {
        execSync(cmd, { stdio: "pipe", cwd: path.dirname(evalDir) });
      } catch (error) {
        logger.error({ command: cmd }, "Setup command failed");
        process.exit(1);
      }
    }
  }

  // Load data from sources and create cross product
  const sourcesData: Record<string, string>[][] = [];

  for (const source of task.sources) {
    if ("csv" in source) {
      const csvPath = path.join(evalDir, source.csv);
      if (!fs.existsSync(csvPath)) {
        logger.error({ csvPath }, "CSV file not found");
        process.exit(1);
      }

      const csvContent = fs.readFileSync(csvPath, "utf-8");
      const csvData: Record<string, string>[] = parseCsv(csvContent, {
        columns: true,
        skip_empty_lines: true,
      });
      sourcesData.push(csvData);
    } else if ("cmd" in source) {
      logger.error("cmd source type not yet implemented");
      process.exit(1);
    } else if ("value" in source) {
      sourcesData.push(source.value);
    }
  }

  // Create cross product of all sources
  if (sourcesData.length === 0) {
    logger.error("No sources configured");
    process.exit(1);
  }

  // Get contexts from sources using pure function
  const data = getContextsFromSources(sourcesData);

  const results: TaskResult[] = [];

  // Get parallelism setting (default to 1 for sequential execution)
  const parallelism = task.run.parallelism ?? 1;
  const limit = pLimit(parallelism);

  // Execute run command for each data row
  // Create promises for all tasks
  const taskPromises = data.map((row, i) => {
    return limit(async () => {
      // Generate command using pure function
      const command = generateCommand(task.run.command, row);

      logger.info({ command, task_id: i + 1 }, "Executing task");

      // Execute the task
      const executionResult = await executeTask(
        command,
        i + 1,
        debugDir,
        evalDir,
        task.run.timeout,
        task.run.early_exit,
        task.validations,
        row
      );

      // If execution failed or timed out, still run validations if output is available
      if (executionResult.error) {
        logger.warn(
          {
            task_id: executionResult.index,
            command: executionResult.command,
            duration: executionResult.duration,
            error: executionResult.error,
            is_timeout: executionResult.isTimeout,
          },
          executionResult.isTimeout ? "Task timed out" : "Task failed"
        );

        // Run validations on available output even if task failed/timed out
        const { validationResults } = processValidations(
          executionResult.output,
          task.validations,
          logger,
          executionResult.index,
          executionResult.duration,
          row
        );

        return {
          index: executionResult.index,
          status: executionResult.isTimeout
            ? TaskStatus.Timeout
            : TaskStatus.Failed,
          command: executionResult.command,
          duration: executionResult.duration,
          validationResults,
        };
      }

      // Task completed successfully, log execution result
      const logMessage = executionResult.earlyExit
        ? "Task completed (early exit)"
        : "Task completed successfully";
      
      logger.info(
        {
          task_id: executionResult.index,
          duration: executionResult.duration,
          early_exit: executionResult.earlyExit || undefined,
        },
        logMessage
      );

      // Run validations on the output
      const { validationResults, status: validationStatus } =
        processValidations(
          executionResult.output,
          task.validations,
          logger,
          executionResult.index,
          executionResult.duration,
          row
        );

      return {
        index: executionResult.index,
        status:
          validationStatus === "passed"
            ? TaskStatus.Passed
            : TaskStatus.ValidationFailed,
        command: executionResult.command,
        duration: executionResult.duration,
        validationResults,
      };
    });
  });

  // Wait for all tasks to complete
  const taskResults = await Promise.all(taskPromises);
  results.push(...taskResults);

  // Calculate summary statistics
  const successCount = results.filter(
    (r) => r.status === TaskStatus.Passed
  ).length;
  const warningCount = results.filter(
    (r) => r.status === TaskStatus.ValidationFailed
  ).length;
  const timeoutCount = results.filter(
    (r) => r.status === TaskStatus.Timeout
  ).length;
  const failCount = results.filter(
    (r) => r.status === TaskStatus.Failed
  ).length;
  const totalDuration = results.reduce((sum, r) => sum + r.duration, 0);

  // Calculate validation statistics
  const totalValidations = results.reduce(
    (sum, r) => sum + r.validationResults.length,
    0
  );
  const passedValidations = results.reduce(
    (sum, r) => sum + r.validationResults.filter((v) => v.passed).length,
    0
  );

  // Print summary
  logger.info(
    {
      total: results.length,
      passed: successCount,
      validation_failed: warningCount,
      timeout: timeoutCount,
      failed: failCount,
      total_duration: totalDuration,
      validations: {
        total: totalValidations,
        passed: passedValidations,
        failed: totalValidations - passedValidations,
      },
    },
    "Evaluation completed"
  );

  // Exit with error code if any task failed (excluding timeouts and validation failures)
  if (failCount > 0) {
    process.exit(1);
  }
}

main();
