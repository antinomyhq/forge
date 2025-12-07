#!/usr/bin/env node

// Handle EPIPE errors gracefully (e.g., when piping to `head` or `jq` that closes early)
process.stdout.on("error", (error: NodeJS.ErrnoException) => {
  if (error.code === "EPIPE") {
    process.exit(0);
  }
  throw error;
});

import * as fs from "fs/promises";
import * as path from "path";
import { fileURLToPath } from "url";
import { parse as parseYaml } from "yaml";
import { exec } from "child_process";
import { promisify } from "util";
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
import { createTempDir, parseCsvAsync } from "./utils.js";

const execAsync = promisify(exec);

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
  try {
    await fs.access(evalDir);
  } catch {
    logger.error({ evalDir }, "Eval directory not found");
    process.exit(1);
  }

  try {
    await fs.access(taskFile);
  } catch {
    logger.error({ evalDir }, "task.yml not found");
    process.exit(1);
  }

  // Read and parse task.yml
  const taskContent = await fs.readFile(taskFile, "utf-8");
  const task: Task = parseYaml(taskContent);

  // Display header
  const displayName = path.relative(__dirname, evalDir) || evalName;

  // Create debug directory with timestamp
  const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
  const debugDir = path.join(evalDir, "debug", timestamp);
  await fs.mkdir(debugDir, { recursive: true });

  // Create a temp directory for setup commands
  const setupTmpDir = await createTempDir("forge-setup-");

  // Execute before_run commands
  if (task.before_run && task.before_run.length > 0) {
    for (const cmd of task.before_run) {
      try {
        logger.info(
          { dir: setupTmpDir.name, command: cmd },
          "Running setup command",
        );
        // Small delay to allow logger to flush before command output
        await new Promise((resolve) => setTimeout(resolve, 0));
        await execAsync(cmd, {
          cwd: setupTmpDir.name,
        });
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
      try {
        await fs.access(csvPath);
      } catch {
        logger.error({ csvPath }, "CSV file not found");
        process.exit(1);
      }

      const csvContent = await fs.readFile(csvPath, "utf-8");
      const csvData = await parseCsvAsync(csvContent, {
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
  const parallelism = task.parallelism ?? 1;
  const limit = pLimit(parallelism);

  // Execute run command for each data row
  // Create promises for all tasks
  const taskPromises = data.map((row, i) => {
    return limit(async () => {
      // Create a unique temp directory for this task
      const taskTmpDir = await createTempDir(`forge-task-${i + 1}-`);

      // Create a 'task' subdirectory for running commands
      const taskWorkDir = path.join(taskTmpDir.name, 'task');
      await fs.mkdir(taskWorkDir, { recursive: true });

      const logFile = path.join(taskTmpDir.name, `task.log`);

      // Context for command interpolation and validations
      const context = { ...row, dir: taskTmpDir.name };

      // Support both single command and multiple commands
      const commands = Array.isArray(task.run) ? task.run : [task.run];

      // Filter out empty or non-string commands
      const validCommands = commands.filter(cmd => typeof cmd === 'string' && cmd.trim().length > 0);

      // If no valid commands, skip this task
      if (validCommands.length === 0) {
        logger.warn({ task_id: i + 1 }, "No valid commands found, skipping task");
        return {
          index: i + 1,
          status: TaskStatus.Failed,
          command: "No valid commands",
          duration: 0,
          validationResults: [],
        };
      }

      let combinedOutput = "";
      let totalDuration = 0;
      let lastError: string | undefined;
      let hasTimeout = false;
      let hasEarlyExit = false;

      // Log task launch once before executing commands
      logger.info(
        {
          task_id: i + 1,
          total_commands: validCommands.length,
          log: logFile,
          dir: taskTmpDir.name,
          work_dir: taskWorkDir,
          parameters: context,
        },
        "Launching task",
      );

      // Execute commands sequentially
      for (let cmdIdx = 0; cmdIdx < validCommands.length; cmdIdx++) {
        const commandTemplate = validCommands[cmdIdx]!; // Non-null assertion safe after filter

        const command = generateCommand(commandTemplate, context);

        logger.info(
          {
            command,
            task_id: i + 1,
            command_id: cmdIdx + 1,
            total_commands: validCommands.length,
          },
          "Executing command",
        );

        const executionResult = await executeTask(
          command,
          i + 1,
          logFile,
          taskWorkDir, // Use taskWorkDir instead of taskTmpDir.name
          task,
          context,
          cmdIdx > 0, // append if this is not the first command
        );

        totalDuration += executionResult.duration;

        if (executionResult.output) {
          combinedOutput += executionResult.output;
        }

        if (executionResult.earlyExit) {
          hasEarlyExit = true;
        }

        // If execution failed or timed out, stop executing remaining commands
        if (executionResult.error) {
          lastError = executionResult.error;
          hasTimeout = executionResult.isTimeout;

          logger.warn(
            {
              task_id: executionResult.index,
              command: executionResult.command,
              command_id: cmdIdx + 1,
              duration: executionResult.duration,
              error: executionResult.error,
              is_timeout: executionResult.isTimeout,
            },
            executionResult.isTimeout ? "Task timed out" : "Task failed",
          );
          break;
        }
      }

      // If any command failed, return failure result
      if (lastError) {
        const { validationResults } = await processValidations(
          combinedOutput,
          task,
          logger,
          i + 1,
          totalDuration,
          logFile,
          context,
        );

        return {
          index: i + 1,
          status: hasTimeout ? TaskStatus.Timeout : TaskStatus.Failed,
          command: validCommands.length === 1 ? validCommands[0]! : `${validCommands.length} commands`,
          duration: totalDuration,
          validationResults,
        };
      }

      // Run validations on the combined output
      const { validationResults, status: validationStatus } =
        await processValidations(
          combinedOutput,
          task,
          logger,
          i + 1,
          totalDuration,
          logFile,
          context,
        );

      return {
        index: i + 1,
        status:
          validationStatus === "passed"
            ? TaskStatus.Passed
            : TaskStatus.ValidationFailed,
        command: validCommands.length === 1 ? validCommands[0]! : `${validCommands.length} commands`,
        duration: totalDuration,
        validationResults,
      };
    });
  });

  // Wait for all tasks to complete
  const taskResults = await Promise.all(taskPromises);
  results.push(...taskResults);

  // Calculate summary statistics
  const successCount = results.filter(
    (r) => r.status === TaskStatus.Passed,
  ).length;
  const warningCount = results.filter(
    (r) => r.status === TaskStatus.ValidationFailed,
  ).length;
  const timeoutCount = results.filter(
    (r) => r.status === TaskStatus.Timeout,
  ).length;
  const failCount = results.filter(
    (r) => r.status === TaskStatus.Failed,
  ).length;
  const totalDuration = results.reduce((sum, r) => sum + r.duration, 0);

  // Calculate validation statistics
  const totalValidations = results.reduce(
    (sum, r) => sum + r.validationResults.length,
    0,
  );
  const passedValidations = results.reduce(
    (sum, r) => sum + r.validationResults.filter((v) => v.passed).length,
    0,
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
      dir: setupTmpDir.name,
    },
    "Evaluation completed",
  );

  // Exit with error code if any task failed (excluding timeouts and validation failures)
  if (failCount > 0) {
    process.exit(1);
  }
  
  // Exit successfully - ensures process terminates even with open handles
  process.exit(0);
}

main().catch((error) => {
  logger.error({ error: error.message }, "Fatal error");
  process.exit(1);
});
