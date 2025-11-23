import * as fs from "fs";
import * as path from "path";
import { parse as parseYaml } from "yaml";
import { parse as parseCsv } from "csv-parse/sync";
import { execSync } from "child_process";
import chalk from "chalk";
import ora from "ora";
import Table from "cli-table3";
import boxen from "boxen";

type Task = {
  before_run: Array<string>;
  run: { command: string };
  source: Source;
};

type Source = { csv: string } | { cmd: string };

function main() {
  // Get eval name from command line arguments
  const evalName = process.argv[2];
  
  if (!evalName) {
    console.error(chalk.red.bold("✗ Error: Please provide an eval name"));
    console.error(chalk.gray("Usage: npm run eval -- <eval-name>"));
    process.exit(1);
  }

  const evalDir = path.join(__dirname, evalName);
  const taskFile = path.join(evalDir, "task.yml");

  // Check if eval directory and task file exist
  if (!fs.existsSync(evalDir)) {
    console.error(chalk.red.bold(`✗ Error: Eval directory not found: ${evalDir}`));
    process.exit(1);
  }

  if (!fs.existsSync(taskFile)) {
    console.error(chalk.red.bold(`✗ Error: task.yml not found in: ${evalDir}`));
    process.exit(1);
  }

  // Read and parse task.yml
  const taskContent = fs.readFileSync(taskFile, "utf-8");
  const task: Task = parseYaml(taskContent);

  // Display header
  console.log(
    boxen(chalk.cyan.bold(`Running Eval: ${evalName}`), {
      padding: 1,
      margin: 1,
      borderStyle: "round",
      borderColor: "cyan",
    })
  );

  // Execute before_run commands
  if (task.before_run && task.before_run.length > 0) {
    console.log(chalk.yellow.bold("\n📦 Executing setup commands...\n"));
    for (const cmd of task.before_run) {
      const spinner = ora(chalk.gray(cmd)).start();
      try {
        execSync(cmd, { stdio: "pipe", cwd: path.dirname(evalDir) });
        spinner.succeed(chalk.green(`Completed: ${cmd}`));
      } catch (error) {
        spinner.fail(chalk.red(`Failed: ${cmd}`));
        process.exit(1);
      }
    }
  }

  // Load data from source
  let data: Record<string, string>[] = [];
  
  if ("csv" in task.source) {
    const csvPath = path.join(evalDir, task.source.csv);
    if (!fs.existsSync(csvPath)) {
      console.error(chalk.red.bold(`✗ Error: CSV file not found: ${csvPath}`));
      process.exit(1);
    }
    
    const csvContent = fs.readFileSync(csvPath, "utf-8");
    data = parseCsv(csvContent, {
      columns: true,
      skip_empty_lines: true,
    });
    
    console.log(chalk.blue.bold(`\n📊 Loaded ${data.length} tasks from ${task.source.csv}\n`));
  } else if ("cmd" in task.source) {
    console.error(chalk.red.bold("✗ cmd source type not yet implemented"));
    process.exit(1);
  }

  // Create results table
  const results: { index: number; status: string; command: string; duration: number }[] = [];

  // Execute run command for each data row
  console.log(chalk.magenta.bold("🚀 Executing tasks...\n"));
  
  for (let i = 0; i < data.length; i++) {
    const row = data[i];
    let command = task.run.command;
    
    // Replace placeholders with values from CSV row
    for (const [key, value] of Object.entries(row ?? {})) {
      command = command.replace(new RegExp(`\\{${key}\\}`, "g"), value);
    }
    
    const spinner = ora(chalk.gray(`[${i + 1}/${data.length}] ${command}`)).start();
    const startTime = Date.now();
    
    try {
      execSync(command, { stdio: "pipe", cwd: path.dirname(evalDir) });
      const duration = Date.now() - startTime;
      spinner.succeed(chalk.green(`[${i + 1}/${data.length}] ${command} ${chalk.gray(`(${duration}ms)`)}`));
      results.push({ index: i + 1, status: "✓", command, duration });
    } catch (error) {
      const duration = Date.now() - startTime;
      spinner.fail(chalk.red(`[${i + 1}/${data.length}] ${command} ${chalk.gray(`(${duration}ms)`)}`));
      results.push({ index: i + 1, status: "✗", command, duration });
    }
  }

  // Display summary table
  const successCount = results.filter((r) => r.status === "✓").length;
  const failCount = results.filter((r) => r.status === "✗").length;
  const totalDuration = results.reduce((sum, r) => sum + r.duration, 0);

  console.log(
    boxen(
      chalk.bold("Summary\n\n") +
        chalk.green(`✓ Passed: ${successCount}\n`) +
        chalk.red(`✗ Failed: ${failCount}\n`) +
        chalk.blue(`⏱  Total Time: ${totalDuration}ms\n`) +
        chalk.gray(`📋 Total Tasks: ${results.length}`),
      {
        padding: 1,
        margin: { top: 1, bottom: 1, left: 0, right: 0 },
        borderStyle: "round",
        borderColor: successCount === results.length ? "green" : failCount === results.length ? "red" : "yellow",
      }
    )
  );

  // Exit with error code if any task failed
  if (failCount > 0) {
    process.exit(1);
  }
}

main();
