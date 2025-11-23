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
import { spawn, execSync } from "child_process";
import pLimit from "p-limit";
import pino from "pino";
import type { Task } from "./model.js";
import {
  getContextsFromSources,
  generateCommand,
} from "./command-generator.js";
import {
  runValidations,
  allValidationsPassed,
  countPassed,
  type ValidationResult,
} from "./validator.js";
import { parseCliArgs } from "./parse.js";

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

/**
 * Formats a date with local timezone information
 */
function formatTimestamp(date: Date): string {
  const offset = -date.getTimezoneOffset();
  const sign = offset >= 0 ? "+" : "-";
  const hours = Math.floor(Math.abs(offset) / 60)
    .toString()
    .padStart(2, "0");
  const minutes = (Math.abs(offset) % 60).toString().padStart(2, "0");
  const timezone = `${sign}${hours}:${minutes}`;

  return `${date.toISOString().replace("Z", "")}${timezone}`;
}

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

  const { evalName, dryRun, evalDir, taskFile } = args;

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

  // If dry-run mode, validate YAML and exit
  if (dryRun) {
    // Validate that sources exist
    for (const source of task.sources) {
      if ("csv" in source) {
        const csvPath = path.join(evalDir, source.csv);
        if (!fs.existsSync(csvPath)) {
          logger.error({ csvPath }, "CSV file not found");
          process.exit(1);
        }
      }
    }

    process.exit(0);
  }

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
    }
  }

  // Create cross product of all sources
  if (sourcesData.length === 0) {
    logger.error("No sources configured");
    process.exit(1);
  }

  // Get contexts from sources using pure function
  const data = getContextsFromSources(sourcesData);

  const results: {
    index: number;
    status: string;
    command: string;
    duration: number;
    validationResults?: ValidationResult[];
  }[] = [];

  // Get parallelism setting (default to 1 for sequential execution)
  const parallelism = task.run.parallelism ?? 1;
  const limit = pLimit(parallelism);
  const timeout = task.run.timeout;

  // Execute run command for each data row
  // Create promises for all tasks
  const taskPromises = data.map((row, i) => {
    return limit(async () => {
      // Generate command using pure function
      const command = generateCommand(task.run.command, row);

      logger.info(
        {
          command,
        },
        "Executing task"
      );

      const startTime = Date.now();

      // Create log file for this task
      const logFile = path.join(debugDir, `task_run_${i + 1}.log`);
      const logStream = fs.createWriteStream(logFile);

      // Write command at the top of the log file
      logStream.write(`Command: ${command}\n`);
      logStream.write(`Started: ${formatTimestamp(new Date())}\n`);
      logStream.write(`${"=".repeat(80)}\n\n`);

      try {
        // Execute command and stream output to log file
        const output = await new Promise<string>((resolve, reject) => {
          const child = spawn(command, {
            shell: true,
            cwd: path.dirname(evalDir),
            stdio: ["ignore", "pipe", "pipe"],
          });

          let stdout = "";
          let stderr = "";
          let timeoutId: NodeJS.Timeout | null = null;
          let timedOut = false;

          // Set up timeout if configured
          if (task.run.timeout) {
            timeoutId = setTimeout(() => {
              timedOut = true;
              logStream.write(`\n${"=".repeat(80)}\n`);
              logStream.write(`Timeout: ${task.run.timeout}s exceeded\n`);
              logStream.write(`Killing process...\n`);
              logStream.end();
              child.kill("SIGKILL");
              reject(new Error(`Task timed out after ${task.run.timeout}s`));
            }, task.run.timeout * 1000);
          }

          // Stream stdout to both log file and capture for validation
          child.stdout?.on("data", (data) => {
            const text = data.toString();
            stdout += text;
            logStream.write(text);
          });

          // Stream stderr to both log file and capture for validation
          child.stderr?.on("data", (data) => {
            const text = data.toString();
            stderr += text;
            logStream.write(text);
          });

          child.on("close", (code) => {
            if (timeoutId) clearTimeout(timeoutId);

            // Don't log if already timed out
            if (timedOut) return;

            logStream.write(`\n${"=".repeat(80)}\n`);
            logStream.write(`Finished: ${formatTimestamp(new Date())}\n`);
            logStream.write(`Exit Code: ${code}\n`);
            logStream.end();

            if (code === 0) {
              resolve(stdout + stderr);
            } else {
              reject(new Error(`Command failed with exit code ${code}`));
            }
          });

          child.on("error", (err) => {
            if (timeoutId) clearTimeout(timeoutId);

            // Don't log if already timed out
            if (timedOut) return;

            logStream.write(`\nError: ${err.message}\n`);
            logStream.end();
            reject(err);
          });
        });

        const duration = Date.now() - startTime;

        // Perform all validations if configured
        const validationResults =
          task.validations && task.validations.length > 0
            ? runValidations(output, task.validations)
            : [];

        const allPassed = allValidationsPassed(validationResults);

        // Determine overall status
        const status = allPassed ? "passed" : "validation_failed";

        return {
          index: i + 1,
          status,
          command,
          duration,
          validationResults,
        };
      } catch (error) {
        const duration = Date.now() - startTime;
        const errorMessage =
          error instanceof Error ? error.message : "Command failed";
        const isTimeout = errorMessage.includes("timed out");

        logger.error(
          {
            taskIndex: i + 1,
            totalTasks: data.length,
            command,
            duration,
            error: errorMessage,
            isTimeout,
          },
          "Task failed"
        );

        return {
          index: i + 1,
          status: "failed",
          command,
          duration,
          validationResults: [],
        };
      }
    });
  });

  // Wait for all tasks to complete
  const taskResults = await Promise.all(taskPromises);
  results.push(...taskResults);

  // Calculate summary statistics
  const successCount = results.filter((r) => r.status === "passed").length;
  const warningCount = results.filter(
    (r) => r.status === "validation_failed"
  ).length;
  const failCount = results.filter((r) => r.status === "failed").length;
  const totalDuration = results.reduce((sum, r) => sum + r.duration, 0);

  // Exit with error code if any task failed
  if (failCount > 0) {
    process.exit(1);
  }
}

main();
