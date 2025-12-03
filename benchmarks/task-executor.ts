import * as fs from "fs";
import * as path from "path";
import { spawn } from "child_process";
import type { Validation, Task } from "./model.js";
import { runValidations, allValidationsPassed } from "./verification.js";

export type TaskExecutionResult = {
  index: number;
  command: string;
  duration: number;
  output?: string;
  error?: string;
  isTimeout: boolean;
  earlyExit?: boolean;
  validations?: Array<{
    name: string;
    passed: boolean;
    error?: string;
  }>;
  exitCode?: number;
  logs?: string;
  stderr?: string;
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
  logFile: string,
  evalDir: string,
  task: Task,
  context?: Record<string, string>,
  append: boolean = false,
  jsonOutput: boolean = false
): Promise<TaskExecutionResult> {
  const startTime = Date.now();

  // Create log stream for this task (append if this is not the first command)
  const logStream = fs.createWriteStream(logFile, { flags: append ? 'a' : 'w' });

  // Get the directory of the log file for validations to use as working directory
  const logDir = path.dirname(logFile);

  // Track timeout and early exit state outside try block
  let timedOut = false;
  let exitedEarly = false;
  
  // Write command at the top of the log file
  logStream.write(`Command: ${command}\n`);
  logStream.write(`Started: ${formatTimestamp(new Date())}\n`);
  logStream.write(`${"=".repeat(80)}\n\n`);

  try {
    
    // Execute command and stream output to log file
    const output = await new Promise<string>((resolve, reject) => {
      const child = spawn(command, {
        shell: true,
        cwd: task.cwd ?? path.dirname(evalDir),
        stdio: ["ignore", "pipe", "pipe"],
      });

      let stdout = "";
      let stderr = "";
      let timeoutId: NodeJS.Timeout | null = null;

      // Helper function to check validations after each write
      const checkValidations = () => {
        if (exitedEarly || timedOut) return;
        
        if (task.early_exit && task.validations && task.validations.length > 0) {
          const currentOutput = stdout + stderr;
          if (currentOutput) {
            // Pass working directory so shell validations can access files
            const results = runValidations(currentOutput, task.validations, context, logDir);
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
      if (task.timeout) {
        timeoutId = setTimeout(() => {
          timedOut = true;
          logStream.write(`\n${"=".repeat(80)}\n`);
          logStream.write(`Timeout: ${task.timeout}s exceeded\n`);
          logStream.write(`Killing process...\n`);
          logStream.end();
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
          logStream.write(text);
        }
        checkValidations();
      });

      // Stream stderr to both log file and capture for validation
      child.stderr?.on("data", (data) => {
        const text = data.toString();
        stderr += text;
        if (logStream.writable) {
          logStream.write(text);
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
    
    // Extract actual command exit code from the log file
    const logContents = fs.existsSync(logFile) ? fs.readFileSync(logFile, 'utf-8') : '';
    const exitCodeMatch = logContents.match(/Exit Code: (\d+)/);
    const commandExitCode = exitCodeMatch ? parseInt(exitCodeMatch[1], 10) : 0;
    
    // Run validations if configured
    let validationResults: Array<{ name: string; passed: boolean; error?: string }> = [];
    
    if (task.validations && task.validations.length > 0) {
      const results = runValidations(output, task.validations, context, logDir);
      validationResults = results.map(v => ({
        name: v.name,
        passed: v.passed,
        message: v.message
      }));
    }
    
    // Read log file for full logs
    const logs = fs.existsSync(logFile) ? fs.readFileSync(logFile, 'utf-8') : '';
    
    const result: TaskExecutionResult = {
      index,
      command,
      duration,
      output,
      isTimeout: timedOut,
      earlyExit: exitedEarly,
      validations: validationResults,
      exitCode: commandExitCode,
      logs,
    };
    
    // Output JSON if requested
    if (jsonOutput) {
      console.log('\n<<<TASK_RESULT_JSON>>>');
      console.log(JSON.stringify(result, null, 2));
      console.log('<<<TASK_RESULT_JSON_END>>>');
    }

    return result;
  } catch (error) {
    const duration = Date.now() - startTime;
    const errorMessage = error instanceof Error ? error.message : "Command failed";
    
    const logs = fs.existsSync(logFile) ? fs.readFileSync(logFile, 'utf-8') : '';
    
    // Extract actual command exit code from the log file
    const exitCodeMatch = logs.match(/Exit Code: (\d+)/);
    const commandExitCode = exitCodeMatch ? parseInt(exitCodeMatch[1], 10) : 1;

    const result: TaskExecutionResult = {
      index,
      command,
      duration,
      error: errorMessage,
      isTimeout: timedOut,
      earlyExit: exitedEarly,
      exitCode: commandExitCode,
      logs,
    };
    
    // Output JSON if requested
    if (jsonOutput) {
      console.log('\n<<<TASK_RESULT_JSON>>>');
      console.log(JSON.stringify(result, null, 2));
      console.log('<<<TASK_RESULT_JSON_END>>>');
    }
    
    return result;
  }
}
