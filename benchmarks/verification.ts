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
