import * as fs from "fs";
import * as path from "path";
import { spawn } from "child_process";
import type { Logger } from "pino";
import { TaskStatus, type Task, type Validation } from "./model.js";
import {
  runValidations,
  allValidationsPassed,
  type ValidationResult,
} from "./verification.js";

export type TaskResult = {
  index: number;
  status: TaskStatus;
  command: string;
  duration: number;
  validationResults: ValidationResult[];
};

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

/**
 * Executes a single task command and returns the result
 */
export async function executeTask(
  command: string,
  index: number,
  debugDir: string,
  evalDir: string,
  timeout: number | undefined,
  validations: Array<Validation> | undefined,
  logger: Logger
): Promise<TaskResult> {
  logger.info({ command }, "Executing task");

  const startTime = Date.now();

  // Create log file for this task
  const logFile = path.join(debugDir, `task_run_${index}.log`);
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
      if (timeout) {
        timeoutId = setTimeout(() => {
          timedOut = true;
          logStream.write(`\n${"=".repeat(80)}\n`);
          logStream.write(`Timeout: ${timeout}s exceeded\n`);
          logStream.write(`Killing process...\n`);
          logStream.end();
          child.kill("SIGKILL");
          reject(new Error(`Task timed out after ${timeout}s`));
        }, timeout * 1000);
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
      validations && validations.length > 0
        ? runValidations(output, validations)
        : [];

    const allPassed = allValidationsPassed(validationResults);

    // Determine overall status
    const status = allPassed ? TaskStatus.Passed : TaskStatus.ValidationFailed;

    return {
      index,
      status,
      command,
      duration,
      validationResults,
    };
  } catch (error) {
    const duration = Date.now() - startTime;
    const cause = error instanceof Error ? error.message : "Command failed";
    const isTimeout = cause.includes("timed out");

    logger.warn(
      {
        command,
        duration,
        cause,
        isTimeout,
      },
      isTimeout ? "Task timed out" : "Task failed"
    );

    return {
      index,
      status: isTimeout ? TaskStatus.Timeout : TaskStatus.Failed,
      command,
      duration,
      validationResults: [],
    };
  }
}
