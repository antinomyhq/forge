import * as fs from "fs";
import * as path from "path";
import { spawn } from "child_process";
import type { Validation } from "./model.js";
import { runValidations, allValidationsPassed } from "./verification.js";

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
  earlyExitOnValidation: boolean | undefined,
  validations: Array<Validation> | undefined,
  context?: Record<string, string>
): Promise<TaskExecutionResult> {
  const startTime = Date.now();

  // Create log file for this task
  const logFile = path.join(debugDir, `task_run_${index}.log`);
  const logStream = fs.createWriteStream(logFile);

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
        cwd: path.dirname(evalDir),
        stdio: ["ignore", "pipe", "pipe"],
      });

      let stdout = "";
      let stderr = "";
      let timeoutId: NodeJS.Timeout | null = null;

      // Helper function to check validations after each write
      const checkValidations = () => {
        if (exitedEarly || timedOut) return;
        
        if (earlyExitOnValidation && validations && validations.length > 0) {
          const currentOutput = stdout + stderr;
          if (currentOutput) {
            const results = runValidations(currentOutput, validations, context);
            if (allValidationsPassed(results)) {
              exitedEarly = true;
              if (timeoutId) clearTimeout(timeoutId);
              logStream.write(`\n${"=".repeat(80)}\n`);
              logStream.write(`Early exit: All validations passed\n`);
              logStream.write(`Killing process...\n`);
              logStream.end();
              child.kill("SIGTERM");
              resolve(currentOutput);
            }
          }
        }
      };

      // Set up timeout if configured
      if (timeout) {
        timeoutId = setTimeout(() => {
          timedOut = true;
          logStream.write(`\n${"=".repeat(80)}\n`);
          logStream.write(`Timeout: ${timeout}s exceeded\n`);
          logStream.write(`Killing process...\n`);
          logStream.end();
          child.kill("SIGKILL");
          // Resolve with captured output so far
          resolve(stdout + stderr);
        }, timeout * 1000);
      }

      // Stream stdout to both log file and capture for validation
      child.stdout?.on("data", (data) => {
        const text = data.toString();
        stdout += text;
        logStream.write(text);
        checkValidations();
      });

      // Stream stderr to both log file and capture for validation
      child.stderr?.on("data", (data) => {
        const text = data.toString();
        stderr += text;
        logStream.write(text);
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
    const errorMessage = error instanceof Error ? error.message : "Command failed";

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
