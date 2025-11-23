import type { Validation } from "./model.js";

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
  validations: Array<Validation>
): ValidationResult[] {
  const results: ValidationResult[] = [];

  for (const validation of validations) {
    if (validation.type === "matches_regex") {
      results.push(validateRegex(output, validation.regex, validation.name));
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
  },
  taskIndex: number,
  duration: number
): ProcessValidationsResult {
  // Run validations if configured and output is available
  const validationResults =
    validations && validations.length > 0 && output
      ? runValidations(output, validations)
      : [];

  const allPassed = allValidationsPassed(validationResults);
  const status = allPassed ? "passed" : "validation_failed";

  // Log validation results if any were run
  if (validationResults.length > 0) {
    const passedCount = countPassed(validationResults);
    const totalCount = validationResults.length;

    logger.info(
      {
        taskIndex,
        status,
        duration,
        validations: validationResults.map((r) => ({
          name: r.name,
          passed: r.passed,
          message: r.message,
        })),
        summary: `${passedCount}/${totalCount} validations passed`,
      },
      allPassed
        ? "Validations completed successfully"
        : "Validations completed with failures"
    );
  }

  return { validationResults, status };
}
