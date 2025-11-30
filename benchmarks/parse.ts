import yargs from "yargs";
import { hideBin } from "yargs/helpers";
import path from "path";

export type CliArgs = {
  evalName: string;
  evalDir: string;
  taskFile: string;
  distributed: boolean;
  daytonaApiKey: string | undefined;
  parallelism: number | undefined;
  apiKey: string | undefined;
  provider: string | undefined;
  model: string | undefined;
};

/**
 * Parses command line arguments and resolves paths
 */
export async function parseCliArgs(dirname: string): Promise<CliArgs> {
  const argv = await yargs(hideBin(process.argv))
    .usage("Usage: $0 <eval-name> [options]")
    .command("$0 <eval-name>", "Run an evaluation")
    .positional("eval-name", {
      describe: "Name of the evaluation to run",
      type: "string",
    })
    .option("distributed", {
      alias: "d",
      type: "boolean",
      description: "Run evaluation on remote Daytona workspaces",
      default: false,
    })
    .option("daytona-api-key", {
      type: "string",
      description: "Daytona API key (or set DAYTONA_API_KEY env var)",
    })
    .option("parallelism", {
      alias: "p",
      type: "number",
      description: "Number of parallel workspaces (overrides task.yml parallelism)",
    })
    .option("api-key", {
      type: "string",
      description: "LLM provider API key (or set ANTHROPIC_API_KEY/OPENAI_API_KEY/etc)",
    })
    .option("provider", {
      type: "string",
      description: "LLM provider (e.g., anthropic, openai, openrouter)",
    })
    .option("model", {
      type: "string",
      description: "LLM model to use (e.g., claude-3-5-sonnet-20241022, gpt-4)",
    })
    .help()
    .alias("h", "help")
    .parseAsync();

  const evalName = argv["eval-name"];

  // Ensure evalName is provided
  if (!evalName) {
    throw new Error("eval-name is required");
  }

  // Support both directory path and direct task.yml path
  let evalDir: string;
  let taskFile: string;

  if (evalName.endsWith("task.yml") || evalName.endsWith(".yml") || evalName.endsWith(".yaml")) {
    // Direct path to task file
    taskFile = path.isAbsolute(evalName) ? evalName : path.join(dirname, evalName);
    evalDir = path.dirname(taskFile);
  } else {
    // Directory path (original behavior)
    evalDir = path.join(dirname, evalName);
    taskFile = path.join(evalDir, "task.yml");
  }

  // Get Daytona API key from CLI or environment
  const daytonaApiKey =
    (argv["daytona-api-key"] as string) || process.env.DAYTONA_API_KEY;

  return {
    evalName,
    evalDir,
    taskFile,
    distributed: argv.distributed as boolean,
    daytonaApiKey,
    parallelism: argv.parallelism as number | undefined,
    apiKey: argv["api-key"] as string | undefined,
    provider: argv.provider as string | undefined,
    model: argv.model as string | undefined,
  };
}
