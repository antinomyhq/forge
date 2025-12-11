import * as fs from "fs";
import * as path from "path";
import { spawn } from "child_process";
import stripAnsi from "strip-ansi";
import type { Validation, Task } from "./model.js";
import { runValidations, allValidationsPassed } from "./verification.js";
import { formatTimestamp } from "./utils.js";

export type TaskExecutionResult = {
  index: number;
  command: string;
  duration: number;
  output?: string;
  error?: string;
  isTimeout: boolean;
  earlyExit?: boolean;
};

/**
 * Executes a single task command and returns the result
 */
export async function executeTask(
  command: string,
  index: number,
  logFile: string,
  cwd: string,
  task: Task,
  context?: Record<string, string>,
  append: boolean = false,
): Promise<TaskExecutionResult> {
  const startTime = Date.now();

  // Create log stream for this task (append if this is not the first command)
  const logStream = fs.createWriteStream(logFile, {
    flags: append ? "a" : "w",
  });

  // Write command at the top of the log file
  logStream.write(`Command: ${command}\n`);
  logStream.write(`Started: ${formatTimestamp(new Date())}\n`);
  logStream.write(`${"=".repeat(80)}\n\n`);

  try {
    // Track timeout state outside the promise
    let timedOut = false;
    let exitedEarly = false;

    // Execute command and stream output to log file
    const output = await new Promise<string>((resolve, reject) => {
      const child = spawn(command, {
        shell: true,
        cwd: cwd,
        stdio: ["ignore", "pipe", "pipe"],
      });

      let stdout = "";
      let stderr = "";
      let timeoutId: NodeJS.Timeout | null = null;

      // Helper function to check validations after each write
      const checkValidations = async () => {
        if (exitedEarly || timedOut) return;

        if (
          task.early_exit &&
          task.validations &&
          task.validations.length > 0
        ) {
          const currentOutput = stdout + stderr;
          if (currentOutput) {
            const results = await runValidations(
              currentOutput,
              task.validations,
              context,
            );
            if (allValidationsPassed(results)) {
              exitedEarly = true;
              if (timeoutId) clearTimeout(timeoutId);
              if (logStream.writable) {
                logStream.write(
                  `\n${"=".repeat(80)}\nEarly exit: All validations passed\nKilling process...\n`,
                  () => {
                    logStream.end();
                  },
                );
              }
              child.kill("SIGTERM");
              resolve(currentOutput);
            }
          }
        }
      };

      // Set up timeout if configured
      if (task.timeout) {
        timeoutId = setTimeout(() => {
          timedOut = true;
          if (logStream.writable) {
            logStream.write(`\n${"=".repeat(80)}\n`);
            logStream.write(`Timeout: ${task.timeout}s exceeded\n`);
            logStream.write(`Killing process...\n`);
            logStream.end();
          }
          child.kill("SIGKILL");
          // Resolve with captured output so far
          resolve(stdout + stderr);
        }, task.timeout * 1000);
      }

      // Stream stdout to both log file and capture for validation
      child.stdout?.on("data", (data) => {
        const text = data.toString();
        stdout += text;
        if (logStream.writable) {
          logStream.write(stripAnsi(text));
        }
        checkValidations();
      });

      // Stream stderr to both log file and capture for validation
      child.stderr?.on("data", (data) => {
        const text = data.toString();
        stderr += text;
        if (logStream.writable) {
          logStream.write(stripAnsi(text));
        }
        checkValidations();
      });

      child.on("close", (code) => {
        if (timeoutId) clearTimeout(timeoutId);

        // Don't log or resolve if already timed out or exited early
        if (timedOut || exitedEarly) return;

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

        // Don't log if already timed out or exited early
        if (timedOut || exitedEarly) return;

        logStream.write(`\nError: ${err.message}\n`);
        logStream.end();
        reject(err);
      });
    });

    const duration = Date.now() - startTime;

    return {
      index,
      command,
      duration,
      output,
      isTimeout: timedOut,
      earlyExit: exitedEarly,
    };
  } catch (error) {
    const duration = Date.now() - startTime;
    const errorMessage =
      error instanceof Error ? error.message : "Command failed";

    return {
      index,
      command,
      duration,
      error: errorMessage,
      isTimeout: false,
      earlyExit: false,
    };
  }
}
