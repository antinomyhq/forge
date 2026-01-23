#!/usr/bin/env tsx

/**
 * Formats benchmark evaluation results from JSON logs into a Markdown table
 * 
 * Usage: tsx benchmarks/format-results.ts <log-file>
 * 
 * Reads NDJSON logs from the evaluation runs and generates a Markdown table
 * with summary statistics and per-evaluation breakdowns.
 */

import * as fs from "fs";
import * as path from "path";

interface LogEntry {
  level: number;
  time: number;
  msg: string;
  [key: string]: any;
}

interface EvalSummary {
  evalName: string;
  total: number;
  passed: number;
  failed: number;
  timeout: number;
  validation_failed: number;
  duration: number;
}

/**
 * Format duration in milliseconds to human-readable string
 */
function formatDuration(ms: number): string {
  if (ms < 1000) {
    return `${ms}ms`;
  }
  
  const seconds = Math.floor(ms / 1000);
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  
  if (hours > 0) {
    const remainingMinutes = minutes % 60;
    const remainingSeconds = seconds % 60;
    return `${hours}h ${remainingMinutes}m ${remainingSeconds}s`;
  }
  
  if (minutes > 0) {
    const remainingSeconds = seconds % 60;
    return `${minutes}m ${remainingSeconds}s`;
  }
  
  return `${seconds}s`;
}

/**
 * Get status emoji for visual indication
 */
function getStatusEmoji(passed: number, failed: number, timeout: number, validationFailed: number): string {
  if (failed > 0 || timeout > 0) {
    return "❌";
  }
  if (validationFailed > 0) {
    return "⚠️";
  }
  if (passed > 0) {
    return "✅";
  }
  return "⭕";
}

/**
 * Parse NDJSON log file and extract evaluation summaries
 */
function parseLogFile(filePath: string): EvalSummary[] {
  const content = fs.readFileSync(filePath, "utf-8");
  const lines = content.trim().split("\n").filter(line => line.trim());
  
  const summaries: EvalSummary[] = [];
  
  for (const line of lines) {
    try {
      const entry: LogEntry = JSON.parse(line);
      
      // Look for "Evaluation completed" messages
      if (entry.msg === "Evaluation completed" && entry.total !== undefined) {
        // Use eval_name from log entry, fallback to "Unknown"
        const evalName = entry.eval_name || "Unknown";
        
        summaries.push({
          evalName,
          total: entry.total || 0,
          passed: entry.passed || 0,
          failed: entry.failed || 0,
          timeout: entry.timeout || 0,
          validation_failed: entry.validation_failed || 0,
          duration: entry.total_duration || 0,
        });
      }
    } catch (error) {
      // Skip invalid JSON lines
      continue;
    }
  }
  
  return summaries;
}

/**
 * Generate Markdown table from evaluation summaries
 */
function generateMarkdownTable(summaries: EvalSummary[]): string {
  if (summaries.length === 0) {
    return "## Benchmark Results\n\n⚠️ No evaluation results found.\n";
  }
  
  // Calculate totals
  const totals = summaries.reduce(
    (acc, summary) => ({
      total: acc.total + summary.total,
      passed: acc.passed + summary.passed,
      failed: acc.failed + summary.failed,
      timeout: acc.timeout + summary.timeout,
      validation_failed: acc.validation_failed + summary.validation_failed,
      duration: acc.duration + summary.duration,
    }),
    { total: 0, passed: 0, failed: 0, timeout: 0, validation_failed: 0, duration: 0 }
  );
  
  const overallStatus = getStatusEmoji(
    totals.passed,
    totals.failed,
    totals.timeout,
    totals.validation_failed
  );
  
  let markdown = `## ${overallStatus} Benchmark Evaluation Results\n\n`;
  markdown += `**Overall:** ${totals.passed}/${totals.total} passed`;
  
  if (totals.failed > 0) {
    markdown += `, ${totals.failed} failed`;
  }
  if (totals.timeout > 0) {
    markdown += `, ${totals.timeout} timeout`;
  }
  if (totals.validation_failed > 0) {
    markdown += `, ${totals.validation_failed} validation failed`;
  }
  
  markdown += ` (${formatDuration(totals.duration)})\n\n`;
  
  // Table header
  markdown += "| Status | Eval | Total | ✅ Passed | ❌ Failed | ⏱️ Timeout | ⚠️ Val Failed | Duration |\n";
  markdown += "|--------|------|-------|-----------|-----------|-----------|---------------|----------|\n";
  
  // Table rows
  for (const summary of summaries) {
    const status = getStatusEmoji(
      summary.passed,
      summary.failed,
      summary.timeout,
      summary.validation_failed
    );
    
    markdown += `| ${status} | ${summary.evalName} | ${summary.total} | ${summary.passed} | ${summary.failed} | ${summary.timeout} | ${summary.validation_failed} | ${formatDuration(summary.duration)} |\n`;
  }
  
  // Summary row
  markdown += `| **${overallStatus}** | **TOTAL** | **${totals.total}** | **${totals.passed}** | **${totals.failed}** | **${totals.timeout}** | **${totals.validation_failed}** | **${formatDuration(totals.duration)}** |\n`;
  
  markdown += "\n---\n";
  markdown += "\n*Co-Authored-By: ForgeCode <noreply@forgecode.dev>*\n";
  
  return markdown;
}

/**
 * Main entry point
 */
function main() {
  const args = process.argv.slice(2);
  
  if (args.length === 0) {
    console.error("Usage: tsx benchmarks/format-results.ts <log-file>");
    process.exit(1);
  }
  
  const logFile = args[0];
  
  if (!fs.existsSync(logFile)) {
    console.error(`Error: Log file not found: ${logFile}`);
    process.exit(1);
  }
  
  try {
    const summaries = parseLogFile(logFile);
    const markdown = generateMarkdownTable(summaries);
    console.log(markdown);
  } catch (error) {
    console.error(`Error processing log file: ${error}`);
    process.exit(1);
  }
}

main();
