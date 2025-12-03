#!/usr/bin/env node
/**
 * Task executor runner for Cloud Run containers
 * Reads task config from file and executes using task-executor
 */

import { executeTask } from "./task-executor.js";
import * as fs from "fs";
import * as path from "path";

async function main() {
  const configFile = process.argv[2];
  
  if (!configFile) {
    console.error("Usage: task-executor-runner.js <config-file>");
    process.exit(1);
  }
  
  const config = JSON.parse(fs.readFileSync(configFile, "utf-8"));
  
  // Create task object from config
  const task = {
    before_run: [],
    run: config.command,
    timeout: config.timeout,
    early_exit: config.early_exit,
    validations: config.validations,
    sources: [],
  };
  
  const debugDir = "/workspace/debug";
  const logFile = path.join(debugDir, "task_run.log");
  
  // Create debug directory
  if (!fs.existsSync(debugDir)) {
    fs.mkdirSync(debugDir, { recursive: true });
  }
  
  // If context contains context_input, write it to a file for validations
  if (config.context && config.context.context_input) {
    const contextFile = path.join(debugDir, "context_input");
    fs.writeFileSync(contextFile, config.context.context_input);
    // Update context to reference the file
    config.context.context_input = contextFile;
  }
  
  console.log("=".repeat(80));
  console.log(`Command: ${config.command}`);
  console.log("=".repeat(80));
  
  try {
    const result = await executeTask(
      config.command,
      1,
      logFile,
      "/workspace",
      task,
      config.context,
      false
    );
    
    // Wait for log file to stabilize (especially important for early_exit)
    let lastSize = 0;
    let stableCount = 0;
    const maxWait = 5000; // 5 seconds max wait
    const checkInterval = 100; // Check every 100ms
    let totalWait = 0;
    
    while (stableCount < 10 && totalWait < maxWait) { // 10 consecutive checks with same size = stable
      await new Promise(resolve => setTimeout(resolve, checkInterval));
      totalWait += checkInterval;
      
      if (fs.existsSync(logFile)) {
        const stats = fs.statSync(logFile);
        const currentSize = stats.size;
        
        if (currentSize === lastSize) {
          stableCount++;
        } else {
          stableCount = 0;
          lastSize = currentSize;
        }
      }
    }
    
    // Print the log file content (which has the command output)
    if (fs.existsSync(logFile)) {
      const logContent = fs.readFileSync(logFile, "utf-8");
      console.log(logContent);
    }
    
    if (result.error || result.isTimeout) {
      console.error("Task failed:", result.error);
      process.exit(1);
    }
    
    process.exit(0);
  } catch (error) {
    console.error("Task execution failed:", error);
    process.exit(1);
  }
}

main();
