#!/usr/bin/env node
/**
 * CLI wrapper for task-executor with JSON output support
 * Reads configuration from file and executes task
 */

import { executeTask } from "./task-executor.js";
import * as fs from "fs";
import * as path from "path";

async function main() {
  const configFile = process.argv[2];
  
  if (!configFile) {
    console.error("Usage: task-executor-cli.js <config-file>");
    process.exit(1);
  }
  
  const config = JSON.parse(fs.readFileSync(configFile, "utf-8"));
  
  // Create task object from config
  const task = {
    before_run: [],
    run: config.command,
    timeout: config.timeout,
    early_exit: config.early_exit,
    validations: config.validations || [],
    sources: [],
  };
  
  const debugDir = "/tmp/debug";
  const logFile = path.join(debugDir, "task_run.log");
  
  // Create debug directory
  if (!fs.existsSync(debugDir)) {
    fs.mkdirSync(debugDir, { recursive: true });
  }
  
  // Update context with the actual context_input path from config
  const context = config.context || {};
  
  try {
    const result = await executeTask(
      config.command,
      1,
      logFile,
      "/workspace/benchmarks",
      task,
      context,
      false,
      true // Enable JSON output
    );
    
    // Exit with appropriate code
    const exitCode = result.exitCode ?? (result.error ? 1 : 0);
    process.exit(exitCode);
  } catch (error) {
    console.error("Task execution failed:", error);
    process.exit(1);
  }
}

main();
