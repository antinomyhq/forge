import yargs from "yargs";
import { hideBin } from "yargs/helpers";
import path from "path";

export type CliArgs = {
  evalName: string;
  evalDir: string;
  taskFile: string;
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

  return {
    evalName,
    evalDir,
    taskFile,
  };
}
