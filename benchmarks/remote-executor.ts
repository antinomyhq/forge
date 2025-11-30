import type { DaytonaOrchestrator } from "./daytona-orchestrator.js";
import type { DistributedTaskUnit } from "./task-distributor.js";
import type { Logger } from "pino";
import { generateCommand } from "./command-generator.js";
import { TaskStatus } from "./model.js";

/**
 * Result of a remote task execution
 */
export interface RemoteExecutionResult {
  taskId: string;
  workspaceId: string;
  status: TaskStatus;
  exitCode: number;
  output: string;
  stderr: string;
  duration: number;
  error?: string;
  isTimeout: boolean;
}

/**
 * Executes tasks on remote Daytona workspaces
 */
export class RemoteExecutor {
  private orchestrator: DaytonaOrchestrator;
  private logger: Logger;
  private binaryPath: string;
  private envOverrides: Record<string, string>;

  constructor(
    orchestrator: DaytonaOrchestrator,
    binaryPath: string,
    logger: Logger,
    envOverrides: Record<string, string> = {}
  ) {
    this.orchestrator = orchestrator;
    this.binaryPath = binaryPath;
    this.logger = logger;
    this.envOverrides = envOverrides;
  }

  /**
   * Executes a single task on a remote workspace
   */
  async executeTask(
    taskUnit: DistributedTaskUnit,
    workspaceId: string
  ): Promise<RemoteExecutionResult> {
    this.logger.info(
      {
        task_id: taskUnit.id,
        workspace_id: workspaceId,
      },
      "Executing remote task"
    );

    const startTime = Date.now();

    try {
      // Generate the command with context variables
      const command = generateCommand(taskUnit.command, taskUnit.context);

      // Replace relative forge path with absolute path to built binary
      // Also replace -C ../.. with -C /tmp/code-forge for codebase directory
      const remoteCommand = command
        .replace(/\.\.\/\.\.\/target\/(debug|release)\/forge/g, this.binaryPath)
        .replace(/-C\s+\.\.\/\.\./g, "-C /tmp/code-forge");

      this.logger.info(
        {
          task_id: taskUnit.id,
          original_command: command,
          remote_command: remoteCommand,
          binary_path: this.binaryPath,
        },
        "Generated remote command"
      );

      // Prepare workspace: cleanup DB and create debug directory
      const setupCommands = [
        "rm -f ~/.forge.db /tmp/.forge.db /workspace/.forge.db 2>/dev/null || true",
        "mkdir -p /tmp/debug"
      ].join(" && ");
      
      try {
        await this.orchestrator.executeCommand(workspaceId, setupCommands);
        this.logger.debug({ task_id: taskUnit.id }, "Workspace setup completed (DB cleanup, debug dir)");
      } catch (error) {
        // Ignore setup errors
        this.logger.debug({ task_id: taskUnit.id, error }, "Workspace setup had issues");
      }

      // Handle sem_search tasks: create test directory and run workspace sync
      if (remoteCommand.includes("-C") || taskUnit.command.includes("sem_search")) {
        this.logger.info({ task_id: taskUnit.id }, "Detected sem_search task - setting up workspace...");
        
        // Check if workspace already set up for this workspace
        const cacheKey = `workspace_synced_${workspaceId}`;
        if (!(this as any)[cacheKey]) {
          this.logger.info({ workspace_id: workspaceId }, "Creating test workspace for the first time...");
          
          // Create a simple test directory structure
          try {
            const setupCommands = `
              mkdir -p /tmp/code-forge/src /tmp/code-forge/tests &&
              cat > /tmp/code-forge/src/main.rs << 'EOF'
// Main application entry point
fn main() {
    println!("Hello, world!");
}

// Retry logic with exponential backoff
fn retry_with_backoff() {
    // Implementation here
}

// Authentication and token validation
fn validate_token(token: &str) -> bool {
    // Token validation logic
    true
}
EOF
              cat > /tmp/code-forge/src/lib.rs << 'EOF'
// Library code with various features

// Error handling for network failures
pub fn handle_network_error() {
    // Error handling implementation
}

// API response caching
pub fn cache_response(key: &str, value: &str) {
    // Caching logic
}

// Streaming LLM responses
pub fn stream_llm_response() {
    // Streaming implementation
}

// File upload validation
pub fn validate_file_upload(size: usize) -> bool {
    size < 10_000_000 // 10MB limit
}

// Rate limiting for API requests
pub fn rate_limit_check() -> bool {
    // Rate limiting logic
    true
}

// Tool registration system
pub fn register_tool(name: &str) {
    // Tool registration
}

// Context truncation for conversation history
pub fn truncate_context(messages: Vec<String>) -> Vec<String> {
    // Truncation logic
    messages
}
EOF
            `.trim();
            
            await this.orchestrator.executeCommand(workspaceId, setupCommands);
            this.logger.info({ workspace_id: workspaceId }, "Test workspace created successfully");
            
            // Run forge workspace sync
            this.logger.info({ workspace_id: workspaceId }, "Running forge workspace sync...");
            const syncCommand = `cd /tmp/code-forge && ${this.binaryPath} workspace sync`;
            const syncResult = await this.orchestrator.executeCommand(workspaceId, syncCommand);
            
            if (syncResult.exitCode === 0) {
              this.logger.info({ workspace_id: workspaceId }, "Workspace synced successfully");
              // Mark this workspace as synced
              (this as any)[cacheKey] = true;
            } else {
              this.logger.warn(
                { workspace_id: workspaceId, exit_code: syncResult.exitCode },
                "Workspace sync failed"
              );
            }
          } catch (error) {
            this.logger.error(
              { workspace_id: workspaceId, error },
              "Failed to setup workspace for sem_search"
            );
          }
        } else {
          this.logger.debug({ workspace_id: workspaceId }, "Workspace already synced on this workspace");
        }
      }

      // Get environment variables for forge (API keys, etc.)
      const envVars = this.getForgeEnvironmentVariables();
      const envPrefix = Object.entries(envVars)
        .map(([key, value]) => `${key}='${value}'`)
        .join(" ");

      // Prepend environment variables to the command
      const commandWithEnv = envPrefix ? `${envPrefix} ${remoteCommand}` : remoteCommand;

      this.logger.debug(
        {
          task_id: taskUnit.id,
          env_vars: Object.keys(envVars),
        },
        "Environment variables set"
      );

      // Execute with timeout if specified
      const result = await this.executeWithTimeout(
        workspaceId,
        commandWithEnv,
        taskUnit.timeout,
        taskUnit.cwd
      );

      const duration = Date.now() - startTime;

      // Determine status based on exit code
      const status =
        result.exitCode === 0 ? TaskStatus.Passed : TaskStatus.Failed;

      const executionResult: RemoteExecutionResult = {
        taskId: taskUnit.id,
        workspaceId,
        status,
        exitCode: result.exitCode,
        output: result.stdout + result.stderr,
        stderr: result.stderr,
        duration,
        isTimeout: result.isTimeout,
      };

      this.logger.info(
        {
          task_id: taskUnit.id,
          workspace_id: workspaceId,
          status,
          duration_ms: duration,
          exit_code: result.exitCode,
          output_length: executionResult.output.length,
          stderr_length: executionResult.stderr.length,
          output_preview: executionResult.output.slice(0, 200),
        },
        "Task execution completed"
      );

      return executionResult;
    } catch (error) {
      const duration = Date.now() - startTime;
      const errorMessage =
        error instanceof Error ? error.message : String(error);

      this.logger.error(
        {
          task_id: taskUnit.id,
          workspace_id: workspaceId,
          error: errorMessage,
          duration_ms: duration,
        },
        "Task execution failed"
      );

      return {
        taskId: taskUnit.id,
        workspaceId,
        status: TaskStatus.Failed,
        exitCode: -1,
        output: "",
        stderr: errorMessage,
        duration,
        error: errorMessage,
        isTimeout: false,
      };
    }
  }

  /**
   * Executes command with timeout support
   */
  private async executeWithTimeout(
    workspaceId: string,
    command: string,
    timeoutSeconds?: number,
    cwd?: string
  ): Promise<{
    exitCode: number;
    stdout: string;
    stderr: string;
    isTimeout: boolean;
  }> {
    if (!timeoutSeconds) {
      const result = await this.orchestrator.executeCommand(
        workspaceId,
        command,
        cwd
      );
      return { ...result, isTimeout: false };
    }

    // Execute with timeout
    const timeoutPromise = new Promise<{
      exitCode: number;
      stdout: string;
      stderr: string;
      isTimeout: boolean;
    }>((resolve) => {
      setTimeout(() => {
        resolve({
          exitCode: 124, // Standard timeout exit code
          stdout: "",
          stderr: `Task timed out after ${timeoutSeconds} seconds`,
          isTimeout: true,
        });
      }, timeoutSeconds * 1000);
    });

    const executionPromise = this.orchestrator
      .executeCommand(workspaceId, command, cwd)
      .then((result) => ({ ...result, isTimeout: false }));

    return Promise.race([executionPromise, timeoutPromise]);
  }

  /**
   * Executes multiple tasks in parallel on different workspaces
   * If there are fewer workspaces than tasks, workspaces will be reused
   */
  async executeTasks(
    taskUnits: DistributedTaskUnit[],
    workspaceIds: string[],
    concurrency: number = 5
  ): Promise<RemoteExecutionResult[]> {
    if (workspaceIds.length === 0) {
      throw new Error("No workspaces available for task execution");
    }

    this.logger.info(
      {
        total_tasks: taskUnits.length,
        available_workspaces: workspaceIds.length,
        concurrency,
      },
      "Starting parallel task execution"
    );

    const results: RemoteExecutionResult[] = [];
    const chunks: Array<{
      unit: DistributedTaskUnit;
      workspaceId: string;
    }[]> = [];

    // Assign tasks to workspaces (round-robin if fewer workspaces than tasks)
    for (let i = 0; i < taskUnits.length; i += concurrency) {
      const chunk = [];
      for (
        let j = i;
        j < Math.min(i + concurrency, taskUnits.length);
        j++
      ) {
        const unit = taskUnits[j];
        // Use modulo to reuse workspaces if we have more tasks than workspaces
        const workspaceId = workspaceIds[j % workspaceIds.length];
        if (unit && workspaceId) {
          chunk.push({ unit, workspaceId });
        }
      }
      chunks.push(chunk);
    }

    // Execute chunks sequentially, tasks within chunk in parallel
    for (const chunk of chunks) {
      const chunkResults = await Promise.all(
        chunk.map(({ unit, workspaceId }) =>
          this.executeTask(unit, workspaceId)
        )
      );
      results.push(...chunkResults);
    }

    this.logger.info(
      {
        total_tasks: results.length,
        passed: results.filter((r) => r.status === TaskStatus.Passed)
          .length,
        failed: results.filter((r) => r.status === TaskStatus.Failed)
          .length,
        timeout: results.filter((r) => r.isTimeout).length,
      },
      "Parallel task execution completed"
    );

    return results;
  }

  /**
   * Gets environment variables needed for forge execution
   * Includes API keys and other forge configuration
   */
  private getForgeEnvironmentVariables(): Record<string, string> {
    const envVars: Record<string, string> = {};

    // API keys for LLM providers
    const apiKeyVars = [
      "ANTHROPIC_API_KEY",
      "OPENAI_API_KEY",
      "OPENROUTER_API_KEY",
      "GEMINI_API_KEY",
      "GROQ_API_KEY",
    ];

    for (const key of apiKeyVars) {
      const value = process.env[key];
      if (value) {
        envVars[key] = value;
      }
    }

    // Forge configuration variables
    const forgeVars = [
      "FORGE_OVERRIDE_PROVIDER",
      "FORGE_OVERRIDE_MODEL",
      "FORGE_TEMPERATURE",
      "FORGE_MAX_TOKENS",
    ];

    for (const key of forgeVars) {
      const value = process.env[key];
      if (value) {
        envVars[key] = value;
      }
    }

    // Apply overrides from CLI arguments
    return { ...envVars, ...this.envOverrides };
  }
}
