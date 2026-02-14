#!/usr/bin/env tsx

/**
 * Orchestration script to discover and run all benchmark evaluations
 * Exits with non-zero code only if all evals fail or critical errors occur
 */

import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";
import { execSync } from "child_process";

// ESM compatibility
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// ANSI color codes
const colors = {
  red: "\x1b[0;31m",
  green: "\x1b[0;32m",
  yellow: "\x1b[1;33m",
  reset: "\x1b[0m",
};

interface EvalStats {
  total: number;
  successful: number;
  failed: number;
}

/**
 * Find all task.yml files in benchmarks/evals directory
 */
function discoverEvaluations(): string[] {
  const evalsDir = path.join(__dirname, "..", "benchmarks", "evals");
  
  if (!fs.existsSync(evalsDir)) {
    console.error(`${colors.red}Evaluations directory not found: ${evalsDir}${colors.reset}`);
    process.exit(1);
  }

  const taskFiles: string[] = [];
  
  function findTaskFiles(dir: string) {
    const entries = fs.readdirSync(dir, { withFileTypes: true });
    
    for (const entry of entries) {
      const fullPath = path.join(dir, entry.name);
      
      if (entry.isDirectory()) {
        findTaskFiles(fullPath);
      } else if (entry.name === "task.yml") {
        taskFiles.push(fullPath);
      }
    }
  }
  
  findTaskFiles(evalsDir);
  return taskFiles.sort();
}

/**
 * Run a single evaluation
 */
function runEvaluation(taskFile: string, index: number, total: number): boolean {
  const evalName = path.basename(path.dirname(taskFile));
  const projectRoot = path.join(__dirname, "..");
  const benchmarksDir = path.join(projectRoot, "benchmarks");
  
  // Convert to relative path from benchmarks directory  
  const relativeTaskFile = path.relative(benchmarksDir, taskFile);
  
  console.log(`${colors.yellow}[${index}/${total}] Running evaluation: ${evalName}${colors.reset}`);
  console.log(`Task file: ${taskFile}`);
  
  try {
    // Run evaluation from project root, but pass path relative to benchmarks dir
    execSync(`npm run eval "${relativeTaskFile}"`, {
      cwd: projectRoot,
      stdio: "inherit",
    });
    
    console.log(`${colors.green}✓ ${evalName} completed successfully${colors.reset}`);
    return true;
  } catch (error: any) {
    const exitCode = error.status || 1;
    console.log(`${colors.red}✗ ${evalName} failed with exit code ${exitCode}${colors.reset}`);
    return false;
  } finally {
    console.log("");
  }
}

/**
 * Main entry point
 */
function main() {
  console.log("Discovering evaluations...");
  
  const taskFiles = discoverEvaluations();
  
  if (taskFiles.length === 0) {
    console.error(`${colors.red}No evaluation task files found in benchmarks/evals/${colors.reset}`);
    process.exit(1);
  }
  
  console.log(`${colors.green}Found ${taskFiles.length} evaluation(s)${colors.reset}`);
  console.log("");
  
  const stats: EvalStats = {
    total: taskFiles.length,
    successful: 0,
    failed: 0,
  };
  
  // Run each evaluation
  taskFiles.forEach((taskFile, index) => {
    const success = runEvaluation(taskFile, index + 1, stats.total);
    
    if (success) {
      stats.successful++;
    } else {
      stats.failed++;
    }
  });
  
  // Print summary
  console.log("================================================");
  console.log("Evaluation Summary:");
  console.log(`  Total:      ${stats.total}`);
  console.log(`  Successful: ${stats.successful}`);
  console.log(`  Failed:     ${stats.failed}`);
  console.log("================================================");
  
  // Exit with error only if all evals failed
  if (stats.successful === 0 && stats.total > 0) {
    console.error(`${colors.red}All evaluations failed${colors.reset}`);
    process.exit(1);
  }
  
  console.log(`${colors.green}Evaluation run complete${colors.reset}`);
  process.exit(0);
}

main();
