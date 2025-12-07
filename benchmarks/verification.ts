import { exec } from "child_process";
import { promisify } from "util";
import Handlebars from "handlebars";
import type { Task, Validation } from "./model.js";
import { escapeRegex } from "./utils.js";

const execAsync = promisify(exec);

// Register Handlebars helper for escaping regex
Handlebars.registerHelper("escapeRegex", escapeRegex);

export type ValidationResult = {
  name: string;
  passed: boolean;
  message: string;
};

/**
 * Validates output against a regex pattern
 */
function validateRegex(
  output: string,
  regex: string,
  name: string,
): ValidationResult {
  const pattern = new RegExp(regex);
  const passed = pattern.test(output);

  return {
    name,
    passed,
    message: passed ? `Matched: ${regex}` : `Did not match: ${regex}`,
  };
}

/**
 * Validates output using a shell command
 */
async function validateShellCommand(
  output: string,
  command: string,
  expectedExitCode: number,
  name: string,
): Promise<ValidationResult> {
  try {
    const { spawn } = await import("child_process");

    // Use spawn to pipe stdin properly
    const result = await new Promise<{ code: number }>((resolve, reject) => {
      const child = spawn(command, {
        shell: true,
        stdio: ["pipe", "pipe", "pipe"],
      });

      let processExited = false;

      // Handle stdin errors (EPIPE when process exits early)
      child.stdin.on("error", (err: NodeJS.ErrnoException) => {
        // Ignore EPIPE errors - they happen when the child process
        // exits before we finish writing, which is expected behavior
        if (err.code !== "EPIPE") {
          reject(err);
        }
      });

      child.on("close", (code) => {
        processExited = true;
        resolve({ code: code ?? 0 });
      });

      child.on("error", (err) => {
        reject(err);
      });

      // Write output to stdin
      // Use setImmediate to ensure event handlers are attached first
      setImmediate(() => {
        if (!processExited && child.stdin.writable) {
          child.stdin.write(output, (err?: Error | null) => {
            if (err && (err as NodeJS.ErrnoException).code !== "EPIPE") {
              // Only reject on non-EPIPE errors
              reject(err);
            } else {
              child.stdin.end();
            }
          });
        } else {
          // Process already exited or stdin not writable
          child.stdin.end();
        }
      });
    });

    // Command succeeded (exit code 0)
    const passed = result.code === expectedExitCode;
    return {
      name,
      passed,
      message: passed
        ? `Command succeeded with exit code ${result.code}`
        : `Expected exit code ${expectedExitCode}, got ${result.code}`,
    };
  } catch (error: any) {
    // Command failed with error
    return {
      name,
      passed: false,
      message: `Command failed: ${error.message}`,
    };
  }
}

/**
 * Runs all validations on output and returns results
 */
export async function runValidations(
  output: string,
  validations: Array<Validation>,
  context?: Record<string, string>,
): Promise<ValidationResult[]> {
  const results: ValidationResult[] = [];

  for (const validation of validations) {
    if (validation.type === "regex") {
      // Interpolate regex with context if provided
      let regex = validation.regex;
      if (context) {
        const template = Handlebars.compile(regex, { strict: true });
        regex = template(context);
      }
      results.push(validateRegex(output, regex, validation.name));
    } else if (validation.type === "shell") {
      // Interpolate command with context if provided
      let command = validation.command;
      if (context) {
        const template = Handlebars.compile(command, { strict: true });
        command = template(context);
      }
      const expectedExitCode = validation.exit_code ?? 0;
      results.push(
        await validateShellCommand(
          output,
          command,
          expectedExitCode,
          validation.name,
        ),
      );
    }
  }

  return results;
}

/**
 * Checks if all validation results passed
 */
export function allValidationsPassed(results: ValidationResult[]): boolean {
  return results.every((result) => result.passed);
}

/**
 * Counts how many validations passed
 */
export function countPassed(results: ValidationResult[]): number {
  return results.filter((result) => result.passed).length;
}

export type ProcessValidationsResult = {
  validationResults: ValidationResult[];
  status: "passed" | "validation_failed";
};

/**
 * Processes validations and returns results with status
 */
export async function processValidations(
  output: string | undefined,
  task: Task,
  logger: {
    info: (data: any, message: string) => void;
    warn: (data: any, message: string) => void;
    error: (data: any, message: string) => void;
  },
  task_id: number,
  duration: number,
  logFile: string,
  context?: Record<string, string>,
): Promise<ProcessValidationsResult> {
  // Run validations if configured and output is available
  const validationResults =
    task.validations && task.validations.length > 0 && output
      ? await runValidations(output, task.validations, context)
      : [];

  const allPassed = allValidationsPassed(validationResults);
  const status = allPassed ? "passed" : "validation_failed";

  // Log all validation results
  if (validationResults.length > 0) {
    const passedCount = countPassed(validationResults);
    const totalCount = validationResults.length;

    if (allPassed) {
      logger.info(
        {
          task_id,
          duration,
          log: logFile,
          parameters: context,
          passed: validationResults.map((r) => r.name),
        },
        "Validation passed",
      );
    } else {
      logger.error(
        {
          task_id,
          duration,
          log_file: logFile,
          parameters: context,
          failed: validationResults
            .filter((r) => !r.passed)
            .map((r) => ({
              name: r.name,
              message: r.message,
            })),
          summary: `${passedCount}/${totalCount} passed`,
        },
        "Validation Failed",
      );
    }
  }

  return { validationResults, status };
}
