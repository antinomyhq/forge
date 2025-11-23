import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";
import { parse as parseYaml } from "yaml";
import { parse as parseCsv } from "csv-parse/sync";
import { spawn, execSync } from "child_process";
import chalk from "chalk";
import ora from "ora";
import Table from "cli-table3";
import pLimit from "p-limit";
import Handlebars from "handlebars";
import yargs from "yargs";
import { hideBin } from "yargs/helpers";

// ESM compatibility for __dirname
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

type Task = {
  before_run: Array<string>;
  run: { command: string; parallelism?: number };
  validations?: Array<Validation>;
  sources: Array<Source>;
};

type Validation = {
  name: string;
  type: "matches_regex";
  regex: string;
};

type Source = { csv: string } | { cmd: string };

async function main() {
  // Parse command line arguments
  const argv = await yargs(hideBin(process.argv))
    .usage("Usage: $0 <eval-name> [options]")
    .command("$0 <eval-name>", "Run an evaluation")
    .positional("eval-name", {
      describe: "Name of the evaluation to run",
      type: "string",
    })
    .option("dry-run", {
      describe: "Validate YAML configuration without executing commands",
      type: "boolean",
      default: false,
    })
    .help()
    .alias("h", "help")
    .parseAsync();

  const evalName = argv["eval-name"];
  const dryRun = argv["dry-run"];

  // Support both directory path and direct task.yml path
  let evalDir: string;
  let taskFile: string;

  if (evalName.endsWith("task.yml") || evalName.endsWith(".yml") || evalName.endsWith(".yaml")) {
    // Direct path to task file
    taskFile = path.isAbsolute(evalName) ? evalName : path.join(__dirname, evalName);
    evalDir = path.dirname(taskFile);
  } else {
    // Directory path (original behavior)
    evalDir = path.join(__dirname, evalName);
    taskFile = path.join(evalDir, "task.yml");
  }

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

  // If dry-run mode, validate YAML and exit silently
  if (dryRun) {
    // Validate that sources exist
    for (const source of task.sources) {
      if ("csv" in source) {
        const csvPath = path.join(evalDir, source.csv);
        if (!fs.existsSync(csvPath)) {
          console.error(chalk.red.bold(`✗ Error: CSV file not found: ${csvPath}`));
          process.exit(1);
        }
      }
    }
    // YAML is valid, exit silently with success
    process.exit(0);
  }

  // Display header
  const displayName = path.relative(__dirname, evalDir) || evalName;
  console.log();
  console.log(chalk.cyan.bold(`Running Eval: ${displayName}`));
  console.log();

  // Create debug directory with timestamp
  const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
  const debugDir = path.join(evalDir, "debug", timestamp);
  fs.mkdirSync(debugDir, { recursive: true });
  
  console.log(chalk.gray(`📁 Debug logs: ${path.relative(process.cwd(), debugDir)}\n`));

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

  // Load data from sources and create cross product
  let data: Record<string, string>[] = [];
  
  const sourcesData: Record<string, string>[][] = [];
  
  for (const source of task.sources) {
    if ("csv" in source) {
      const csvPath = path.join(evalDir, source.csv);
      if (!fs.existsSync(csvPath)) {
        console.error(chalk.red.bold(`✗ Error: CSV file not found: ${csvPath}`));
        process.exit(1);
      }
      
      const csvContent = fs.readFileSync(csvPath, "utf-8");
      const csvData: Record<string, string>[] = parseCsv(csvContent, {
        columns: true,
        skip_empty_lines: true,
      });
      sourcesData.push(csvData);
    } else if ("cmd" in source) {
      console.error(chalk.red.bold("✗ cmd source type not yet implemented"));
      process.exit(1);
    }
  }
  
  // Create cross product of all sources
  if (sourcesData.length === 0) {
    console.error(chalk.red.bold("✗ Error: No sources configured"));
    process.exit(1);
  }
  
  // Cross product implementation
  data = sourcesData.reduce((acc, sourceData) => {
    if (acc.length === 0) {
      return sourceData;
    }
    
    const result: Record<string, string>[] = [];
    for (const accItem of acc) {
      for (const sourceItem of sourceData) {
        result.push({ ...accItem, ...sourceItem });
      }
    }
    return result;
  }, [] as Record<string, string>[]);
  
  console.log(
    chalk.blue.bold(
      `\n📊 Loaded ${data.length} tasks from ${task.sources.length} source(s) (cross product)\n`
    )
  );

  const results: { 
    index: number; 
    status: string; 
    command: string; 
    duration: number;
    validationResults?: Array<{ name: string; passed: boolean; message: string }>;
  }[] = [];

  // Get parallelism setting (default to 1 for sequential execution)
  const parallelism = task.run.parallelism ?? 1;
  const limit = pLimit(parallelism);

  // Execute run command for each data row
  console.log(
    chalk.magenta.bold(
      `🚀 Executing tasks (parallelism: ${parallelism})...\n`
    )
  );

  // Create spinner map for tracking individual task progress
  const spinners = new Map<number, ReturnType<typeof ora>>();

  // Create promises for all tasks
  const taskPromises = data.map((row, i) => {
    return limit(async () => {
      // Compile and render command template with Handlebars
      const template = Handlebars.compile(task.run.command);
      const command = template(row ?? {});
      
      const spinner = ora(chalk.gray(`[${i + 1}/${data.length}] ${command}`)).start();
      spinners.set(i, spinner);
      const startTime = Date.now();
      
      // Create log file for this task
      const logFile = path.join(debugDir, `task_run_${i + 1}.log`);
      const logStream = fs.createWriteStream(logFile);
      
      // Write command at the top of the log file
      logStream.write(`Command: ${command}\n`);
      logStream.write(`Started: ${new Date().toISOString()}\n`);
      logStream.write(`${"=".repeat(80)}\n\n`);
      
      try {
        // Execute command and stream output to log file
        const output = await new Promise<string>((resolve, reject) => {
          const child = spawn(command, {
            shell: true,
            cwd: path.dirname(evalDir),
            stdio: ["ignore", "pipe", "pipe"]
          });
          
          let stdout = "";
          let stderr = "";
          
          // Stream stdout to both log file and capture for validation
          child.stdout?.on("data", (data) => {
            const text = data.toString();
            stdout += text;
            logStream.write(text);
          });
          
          // Stream stderr to both log file and capture for validation
          child.stderr?.on("data", (data) => {
            const text = data.toString();
            stderr += text;
            logStream.write(text);
          });
          
          child.on("close", (code) => {
            logStream.write(`\n${"=".repeat(80)}\n`);
            logStream.write(`Finished: ${new Date().toISOString()}\n`);
            logStream.write(`Exit Code: ${code}\n`);
            logStream.end();
            
            if (code === 0) {
              resolve(stdout + stderr);
            } else {
              reject(new Error(`Command failed with exit code ${code}`));
            }
          });
          
          child.on("error", (err) => {
            logStream.write(`\nError: ${err.message}\n`);
            logStream.end();
            reject(err);
          });
        });
        
        const duration = Date.now() - startTime;
        
        // Perform all validations if configured
        const validationResults: Array<{ name: string; passed: boolean; message: string }> = [];
        let allValidationsPassed = true;
        
        if (task.validations && task.validations.length > 0) {
          for (const validation of task.validations) {
            if (validation.type === "matches_regex") {
              const regex = new RegExp(validation.regex);
              const passed = regex.test(output);
              allValidationsPassed = allValidationsPassed && passed;
              
              validationResults.push({
                name: validation.name,
                passed,
                message: passed 
                  ? `Matched: ${validation.regex}`
                  : `Did not match: ${validation.regex}`
              });
            }
          }
        }
        
        // Determine overall status
        const status = allValidationsPassed ? "✓" : "⚠";
        const color = allValidationsPassed ? chalk.green : chalk.yellow;
        
        // Build validation summary for display
        let validationSummary = "";
        if (validationResults.length > 0) {
          const passedCount = validationResults.filter(v => v.passed).length;
          const totalCount = validationResults.length;
          validationSummary = ` ${chalk.gray(`[Validations: ${passedCount}/${totalCount}]`)}`;
        }
        
        spinner.succeed(
          color(
            `[${i + 1}/${data.length}] ${command} ${chalk.gray(`(${duration}ms)`)}${validationSummary}`
          )
        );
        
        return { 
          index: i + 1, 
          status, 
          command, 
          duration,
          validationResults
        };
      } catch (error) {
        const duration = Date.now() - startTime;
        spinner.fail(
          chalk.red(
            `[${i + 1}/${data.length}] ${command} ${chalk.gray(`(${duration}ms)`)} - Command failed`
          )
        );
        return { 
          index: i + 1, 
          status: "✗", 
          command, 
          duration,
          validationResults: []
        };
      }
    });
  });

  // Wait for all tasks to complete
  const taskResults = await Promise.all(taskPromises);
  results.push(...taskResults);

  // Display summary table
  const successCount = results.filter((r) => r.status === "✓").length;
  const warningCount = results.filter((r) => r.status === "⚠").length;
  const failCount = results.filter((r) => r.status === "✗").length;
  const totalDuration = results.reduce((sum, r) => sum + r.duration, 0);

  const summaryParts = [
    chalk.bold("Summary\n"),
  ];
  
  if (successCount > 0) {
    summaryParts.push(chalk.green(`✓ Passed: ${successCount}\n`));
  }
  
  if (warningCount > 0) {
    summaryParts.push(chalk.yellow(`⚠ Validation Failed: ${warningCount}\n`));
  }
  
  if (failCount > 0) {
    summaryParts.push(chalk.red(`✗ Failed: ${failCount}\n`));
  }
  
  summaryParts.push(
    chalk.blue(`⏱  Total Time: ${totalDuration}ms\n`),
    chalk.gray(`📋 Total Tasks: ${results.length}\n`),
    chalk.magenta(`⚡ Parallelism: ${parallelism}`)
  );
  
  if (task.validations && task.validations.length > 0) {
    summaryParts.push(
      `\n${chalk.cyan(`🔍 Validations: ${task.validations.length}`)}`
    );
  }

  console.log();
  console.log(summaryParts.join(""));
  console.log();

  // Exit with error code if any task failed
  if (failCount > 0) {
    process.exit(1);
  }
}

main();
