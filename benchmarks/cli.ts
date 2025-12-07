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
import { execSync } from "child_process";
import pLimit from "p-limit";
import pino from "pino";
import { TaskStatus, type Task } from "./model.js";
import {
  getContextsFromSources,
  generateCommand,
} from "./command-generator.js";
import { parseCliArgs } from "./parse.js";
import { executeTask, type TaskExecutionResult } from "./task-executor.js";
import { processValidations, type ValidationResult } from "./verification.js";

export type TaskResult = {
  index: number;
  status: TaskStatus;
  command: string;
  duration: number;
  validationResults: ValidationResult[];
};

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

  const { evalName, evalDir, taskFile, cloudrun, gcpProject, gcpRegion, parallelism, apiKey, provider, model } = args;

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
  
  // Set eval_dir on task for later use
  task.eval_dir = evalDir;

  // Display header
  const displayName = path.relative(__dirname, evalDir) || evalName;

  // Create debug directory with timestamp
  const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
  const debugDir = path.join(evalDir, "debug", timestamp);
  fs.mkdirSync(debugDir, { recursive: true });

  // Execute before_run commands (only for local execution)
  if (task.before_run && task.before_run.length > 0 && !args.cloudrun) {
    logger.info("Executing before_run commands");
    for (const cmd of task.before_run) {
      try {
        logger.info({ command: cmd }, "Running setup command");
        execSync(cmd, {
          stdio: "inherit",
          cwd: task.cwd ?? path.dirname(evalDir),
        });
      } catch (error) {
        logger.error({ command: cmd }, "Setup command failed");
        process.exit(1);
      }
    }
  } else if (task.before_run && task.before_run.length > 0) {
    logger.info("Skipping before_run commands (using pre-built container image)");
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
    } else if ("value" in source) {
      sourcesData.push(source.value);
    }
  }

  // Create cross product of all sources
  if (sourcesData.length === 0) {
    logger.error("No sources configured");
    process.exit(1);
  }

  // Handle Cloud Run execution
  if (cloudrun) {
    // Try to get project from ADC if not provided
    let projectId = gcpProject;
    
    if (!projectId) {
      try {
        // Get project from gcloud config
        const { execSync } = await import("child_process");
        projectId = execSync("gcloud config get-value project", { 
          encoding: "utf-8" 
        }).trim();
        
        if (projectId && projectId !== "(unset)") {
          logger.info({ project: projectId }, "Using GCP project from ADC/gcloud config");
        } else {
          projectId = undefined;
        }
      } catch (error) {
        // ADC not configured
      }
    }

    if (!projectId) {
      logger.error(
        "GCP project ID is required for Cloud Run execution. Either:\n" +
        "  1. Set GCP_PROJECT env var\n" +
        "  2. Use --gcp-project flag\n" +
        "  3. Run: gcloud config set project YOUR_PROJECT_ID"
      );
      process.exit(1);
    }

    logger.info("Running evaluation on Google Cloud Run");

    // Import orchestrator factory
    const { createOrchestrator } = await import("./orchestrator-factory.js");
    const { distributeTask } = await import("./task-distributor.js");
    
    // Build environment overrides
    const envOverrides: Record<string, string> = {};
    
    // Add API key
    if (apiKey) {
      const providerLower = provider?.toLowerCase().replace(/_/g, '');
      if (providerLower?.includes("openrouter")) {
        envOverrides.OPENROUTER_API_KEY = apiKey;
      } else if (providerLower === "anthropic") {
        envOverrides.ANTHROPIC_API_KEY = apiKey;
      } else if (providerLower === "openai") {
        envOverrides.OPENAI_API_KEY = apiKey;
      } else {
        // Default to provider-specific key
        envOverrides[`${provider?.toUpperCase()}_API_KEY`] = apiKey;
      }
    }

    // Add provider and model overrides
    if (provider) {
      envOverrides.FORGE_OVERRIDE_PROVIDER = provider;
    }
    if (model) {
      envOverrides.FORGE_OVERRIDE_MODEL = model;
    }

    logger.info({ envOverrides }, "Environment overrides configured for Cloud Run");

    // Build and push Docker image to Artifact Registry
    const region = gcpRegion || "us-central1";
    const imageTag = `${region}-docker.pkg.dev/${projectId}/forge-eval/forge-eval:latest`;
    const localTag = "forge-eval:latest";
    
    logger.info({ image: imageTag }, "Building Docker image for Cloud Run...");
    
    // Get the root directory (parent of benchmarks directory)
    // evalDir is like /path/to/benchmarks/evals/create_skill
    // We need to go up to /path/to/code-forge
    const rootDir = path.resolve(evalDir, "../../..");
    
    try {
      // Build the binary using cross for linux/amd64
      logger.info("Building forge binary with cross for x86_64-unknown-linux-gnu...");
      execSync(
        `cross build --release --bin forge --target x86_64-unknown-linux-gnu`,
        {
          cwd: rootDir,
          stdio: "inherit",
        }
      );

      // Install node_modules for benchmarks if not already installed
      logger.info("Installing node_modules for benchmarks...");
      const benchmarksDir = path.join(rootDir, "benchmarks");
      const fsModule = await import("fs");
      if (!fsModule.existsSync(path.join(benchmarksDir, "node_modules"))) {
        execSync(`npm ci`, {
          cwd: benchmarksDir,
          stdio: "inherit",
        });
      }

      // Build TypeScript to JavaScript
      logger.info("Building TypeScript to JavaScript...");
      execSync(`npx tsc --project tsconfig.container.json --skipLibCheck`, {
        cwd: benchmarksDir,
        stdio: "inherit",
      });

      // Build the Docker image with the cross-compiled binary
      // Use a custom dockerignore that allows the target directory
      logger.info({ tag: localTag }, "Building Docker image with cross-compiled binary for linux/amd64...");
      
      // Temporarily rename .dockerignore and use .dockerignore.eval
      const fs = await import("fs");
      const dockerignoreBackup = path.join(rootDir, ".dockerignore.backup");
      const dockerignoreOriginal = path.join(rootDir, ".dockerignore");
      const dockerignoreEval = path.join(rootDir, ".dockerignore.eval");
      
      try {
        // Backup original .dockerignore if it exists
        if (fs.existsSync(dockerignoreOriginal)) {
          execSync(`cp ${dockerignoreOriginal} ${dockerignoreBackup}`, { cwd: rootDir });
        }
        
        // Use .dockerignore.eval as .dockerignore
        if (fs.existsSync(dockerignoreEval)) {
          execSync(`cp ${dockerignoreEval} ${dockerignoreOriginal}`, { cwd: rootDir });
        }
        
        execSync(
          `docker build --platform linux/amd64 -f Dockerfile.eval -t ${localTag} .`,
          {
            cwd: rootDir,
            stdio: "inherit",
          }
        );
      } finally {
        // Restore original .dockerignore
        if (fs.existsSync(dockerignoreBackup)) {
          execSync(`mv ${dockerignoreBackup} ${dockerignoreOriginal}`, { cwd: rootDir });
        }
      }

      // Tag for Artifact Registry
      logger.info({ artifact_registry_image: imageTag }, "Tagging for Artifact Registry...");
      execSync(`docker tag ${localTag} ${imageTag}`, { stdio: "inherit" });

      // Authenticate with Artifact Registry
      logger.info("Authenticating with Artifact Registry...");
      execSync(`gcloud auth configure-docker ${region}-docker.pkg.dev --quiet`, {
        stdio: "pipe",
      });

      // Push to Artifact Registry
      logger.info({ image: imageTag }, "Pushing image to Artifact Registry...");
      execSync(`docker push ${imageTag}`, { stdio: "inherit" });

      logger.info({ image: imageTag }, "Image successfully pushed to Artifact Registry");
    } catch (error: any) {
      logger.error({ error: error.message }, "Failed to build or push Docker image");
      process.exit(1);
    }

    // Create Cloud Run orchestrator
    const orchestrator = createOrchestrator(
      {
        type: 'cloudrun',
        cloudrun: {
          projectId: projectId, // Use detected or provided project ID
          region: region,
          image: imageTag,
        },
      },
      logger
    );

    try {
      // Distribute task into individual task units
      const flattenedSources = sourcesData.flat();
      const taskUnits = distributeTask(task, flattenedSources, evalDir);
      
      logger.info({ task_count: taskUnits.length }, "Distributed task into units");

      // Run tasks using orchestrator
      const result = await orchestrator.runTasks({
        tasks: taskUnits,
        maxConcurrency: parallelism ?? task.parallelism ?? 3,
        debugDir,
        envOverrides,
      });

      logger.info(
        {
          total: result.total,
          passed: result.passed,
          validation_failed: result.validation_failed,
          timeout: result.timeout,
          failed: result.failed,
          validations_passed: result.validationsPassed,
          validations_failed: result.validationsFailed,
          validations_total: result.validationsTotal,
        },
        "Cloud Run evaluation completed"
      );

      // Exit with error code if any task failed
      if (result.failed > 0) {
        process.exit(1);
      }

      return;
    } catch (error) {
      logger.error(
        {
          error: error instanceof Error ? error.message : String(error),
        },
        "Cloud Run execution failed"
      );
      process.exit(1);
    }
  }

  // Local execution (default)
  logger.info("Running evaluation locally");

  // Import orchestrator factory and task distributor
  const { createOrchestrator } = await import("./orchestrator-factory.js");
  const { distributeTask } = await import("./task-distributor.js");
  
  // Create local orchestrator
  const orchestrator = createOrchestrator({ type: 'local' }, logger);
  
  // Distribute task into individual task units
  const flattenedSources = sourcesData.flat();
  const taskUnits = distributeTask(task, flattenedSources, debugDir);
  
  logger.info({ task_count: taskUnits.length }, "Distributed task into units");

  // Run tasks using orchestrator
  try {
    logger.info("Calling orchestrator.runTasks...");
    const result = await orchestrator.runTasks({
      tasks: taskUnits,
      maxConcurrency: parallelism ?? task.parallelism ?? 1,
      debugDir,
    });
    logger.info("Orchestrator.runTasks completed");

    // Print summary
    logger.info(
    {
      total: result.total,
      passed: result.passed,
      validation_failed: result.validation_failed,
      timeout: result.timeout,
      failed: result.failed,
      validations_passed: result.validationsPassed,
      validations_failed: result.validationsFailed,
      validations_total: result.validationsTotal,
    },
    "Local evaluation completed"
  );

  // Write summary file
  const summaryFile = path.join(debugDir, 'summary.json');
  fs.writeFileSync(summaryFile, JSON.stringify(result, null, 2), 'utf-8');
  logger.info({ summaryFile }, "Summary written");

  // Exit with error code if any task failed
  if (result.failed > 0) {
    process.exit(1);
  }
  } catch (error: unknown) {
    const err = error instanceof Error ? error : new Error(String(error));
    logger.error({ error: err.message, stack: err.stack }, "Orchestrator.runTasks failed");
    throw error;
  }
}

main();
