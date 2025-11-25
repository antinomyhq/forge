import Handlebars from "handlebars";
import type { Validation } from "./model.js";

/**
 * Escapes special regex characters in a string
 */
function escapeRegex(str: string): string {
  return str.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

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
function validateRegex(output: string, regex: string, name: string): ValidationResult {
  const pattern = new RegExp(regex);
  const passed = pattern.test(output);

  return {
    name,
    passed,
    message: passed ? `Matched: ${regex}` : `Did not match: ${regex}`,
  };
}

/**
 * Runs all validations on output and returns results
 */
export function runValidations(
  output: string,
  validations: Array<Validation>,
  context?: Record<string, string>
): ValidationResult[] {
  const results: ValidationResult[] = [];

  for (const validation of validations) {
    if (validation.type === "matches_regex") {
      // Interpolate regex with context if provided
      let regex = validation.regex;
      if (context) {
        const template = Handlebars.compile(regex, { strict: true });
        regex = template(context);
      }
      results.push(validateRegex(output, regex, validation.name));
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
export function processValidations(
  output: string | undefined,
  validations: Array<Validation> | undefined,
  logger: {
    info: (data: any, message: string) => void;
    warn: (data: any, message: string) => void;
  },
  task_id: number,
  duration: number,
  context?: Record<string, string>
): ProcessValidationsResult {
  // Run validations if configured and output is available
  const validationResults =
    validations && validations.length > 0 && output
      ? runValidations(output, validations, context)
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
          passed: validationResults.map((r) => r.name),
        },
        "Validation Passed"
      );
    } else {
      logger.warn(
        {
          task_id,
          duration,
          failed: validationResults.filter((r) => !r.passed).map((r) => ({
            name: r.name,
            message: r.message,
          })),
          summary: `${passedCount}/${totalCount} passed`,
        },
        "Validation failed"
      );
    }
  }

  return { validationResults, status };
}
