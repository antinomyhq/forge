import type { Logger } from "pino";
import { execSync } from "child_process";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";

/**
 * Configuration for Cloud Run orchestrator
 */
export interface CloudRunConfig {
  projectId: string;
  region: string;
  image: string;
  serviceAccount?: string;
  maxRetries?: number;
}

/**
 * Result from executing a command on Cloud Run
 */
export interface CloudRunExecutionResult {
  stdout: string;
  stderr: string;
  exitCode: number;
  executionName: string;
  logs: string;
}

/**
 * Orchestrates task execution on Google Cloud Run Jobs
 * 
 * Uses Cloud Run Jobs to execute tasks in parallel with proper isolation.
 * Each task runs in its own job execution with the forge binary from Docker image.
 */
export class CloudRunOrchestrator {
  private logger: Logger;
  private config: CloudRunConfig;
  private jobNames: Map<string, string> = new Map(); // taskId -> jobName
  private executionNames: Map<string, string> = new Map(); // taskId -> executionName
  private accessToken: string | null = null;

  constructor(config: CloudRunConfig, logger: Logger) {
    this.config = {
      ...config,
      maxRetries: config.maxRetries ?? 0,
    };
    this.logger = logger;
  }

  /**
   * Gets the GCP access token for authentication
   */
  private getAccessToken(): string {
    if (!this.accessToken) {
      this.accessToken = execSync("gcloud auth print-access-token", {
        encoding: "utf-8",
      }).trim();
    }
    return this.accessToken;
  }

  /**
   * Creates a Cloud Run Job for a task
   */
  async createJob(
    taskId: string,
    command: string,
    env: Record<string, string>,
    timeout: number = 3600
  ): Promise<string> {
    const timestamp = Date.now();
    const jobName = `forge-eval-${taskId.replace(/[^a-z0-9-]/g, "-")}-${timestamp}`.toLowerCase();
    
    this.logger.info(
      { task_id: taskId, job_name: jobName },
      "Creating Cloud Run Job"
    );

    try {
      // Get access token for authentication
      const accessToken = this.getAccessToken();

      // Create Cloud Run Job using gcloud CLI
      // Write env vars to a temporary YAML file to avoid escaping issues
      const envFilePath = path.join(os.tmpdir(), `env-${jobName}.yaml`);
      
      // Add the command as TASK_COMMAND env var
      const allEnvVars = {
        ...env,
        TASK_COMMAND: command
      };
      
      // Create YAML content for env vars
      const envYaml = Object.entries(allEnvVars)
        .map(([key, value]) => `${key}: ${JSON.stringify(value)}`)
        .join("\n");
      fs.writeFileSync(envFilePath, envYaml);
      
      // Create and execute the job in one command with --execute-now (without --wait for parallel execution)
      const createCommand = [
        "gcloud run jobs create",
        jobName,
        `--image=${this.config.image}`,
        `--region=${this.config.region}`,
        `--project=${this.config.projectId}`,
        `--max-retries=${this.config.maxRetries}`,
        `--task-timeout=${timeout}s`,
        `--memory=2Gi`,
        `--cpu=2`,
        `--env-vars-file=${envFilePath}`,
        `--execute-now`
      ];

      if (this.config.serviceAccount) {
        createCommand.push(`--service-account=${this.config.serviceAccount}`);
      }

      // Execute gcloud command and capture output
      // gcloud writes to stderr, so we redirect stderr to stdout to capture everything
      const createOutput = execSync(createCommand.join(" ") + " 2>&1", {
        encoding: "utf-8",
        maxBuffer: 10 * 1024 * 1024,
        shell: "/bin/bash",
        env: {
          ...process.env,
          CLOUDSDK_AUTH_ACCESS_TOKEN: accessToken,
        },
      });

      // Parse execution name from output
      // Format: "Job [job-name] has successfully been created and completed execution [execution-name]."
      this.logger.debug({ 
        task_id: taskId, 
        output_length: createOutput.length,
        output_preview: createOutput.slice(0, 500) 
      }, "Parsing execution name from output");
      
      const executionMatches = createOutput.match(/\[([a-z0-9-]+)\]/g);
      let executionName = jobName;
      
      if (executionMatches && executionMatches.length > 0) {
        this.logger.debug({ task_id: taskId, matches: executionMatches }, "Found execution name matches");
        
        // Look for execution name pattern (job-name-xxxxx)
        for (const match of executionMatches) {
          const name = match.slice(1, -1);
          if (name.startsWith(jobName + '-') && name.length > jobName.length + 1) {
            executionName = name;
            this.logger.debug({ task_id: taskId, execution_name: executionName }, "Found execution-specific name");
            break;
          }
        }
        
        // If no execution-specific name found, use the last match
        if (executionName === jobName && executionMatches.length > 0) {
          const lastMatch = executionMatches[executionMatches.length - 1];
          executionName = lastMatch.slice(1, -1);
          this.logger.debug({ task_id: taskId, execution_name: executionName }, "Using last match as execution name");
        }
      } else {
        this.logger.warn({ task_id: taskId }, "No execution name matches found in output");
      }

      this.logger.info({
        task_id: taskId,
        execution_name: executionName
      }, "Cloud Run Job created and execution started");

      this.jobNames.set(taskId, jobName);
      this.executionNames.set(taskId, executionName);

      // Clean up temp env file
      try {
        fs.unlinkSync(envFilePath);
      } catch (e) {
        // Ignore cleanup errors
      }

      this.logger.info(
        { 
          task_id: taskId, 
          job_name: jobName,
          execution_name: executionName 
        },
        "Cloud Run Job created and execution started (not waiting)"
      );

      return executionName;
    } catch (error) {
      this.logger.error(
        {
          task_id: taskId,
          job_name: jobName,
          error: error instanceof Error ? error.message : String(error),
        },
        "Failed to create Cloud Run Job"
      );
      throw error;
    }
  }

  /**
   * Waits for an execution to complete by polling its status
   */
  async waitForExecution(
    executionName: string,
    taskId: string,
    timeoutSeconds: number
  ): Promise<void> {
    const startTime = Date.now();
    const maxWaitMs = (timeoutSeconds + 60) * 1000; // Add 60s buffer for job overhead
    
    while (Date.now() - startTime < maxWaitMs) {
      try {
        const accessToken = this.getAccessToken();
        const describeCommand = [
          "gcloud run jobs executions describe",
          executionName,
          `--region=${this.config.region}`,
          `--project=${this.config.projectId}`,
          `--format=json`,
        ];

        const executionInfo = JSON.parse(
          execSync(describeCommand.join(" "), {
            encoding: "utf-8",
            env: {
              ...process.env,
              CLOUDSDK_AUTH_ACCESS_TOKEN: accessToken,
            },
          })
        );

        // Check if execution is complete
        const status = executionInfo.status;
        if (status?.completionTime) {
          this.logger.debug(
            { task_id: taskId, execution_name: executionName },
            "Execution completed"
          );
          return;
        }

        // Wait 2 seconds before next poll
        await new Promise((resolve) => setTimeout(resolve, 2000));
      } catch (error) {
        // Execution might not be visible yet, continue polling
        await new Promise((resolve) => setTimeout(resolve, 2000));
      }
    }

    throw new Error(`Execution ${executionName} timed out after ${timeoutSeconds}s`);
  }

  /**
   * Gets the execution result including logs
   */
  async getExecutionResult(taskId: string): Promise<CloudRunExecutionResult> {
    const executionName = this.executionNames.get(taskId);
    const jobName = this.jobNames.get(taskId);

    if (!executionName || !jobName) {
      throw new Error(`No execution found for task ${taskId}`);
    }

    this.logger.debug(
      { task_id: taskId, execution_name: executionName },
      "Fetching execution result"
    );

    try {
      // Get access token
      const accessToken = this.getAccessToken();
      
      // Get execution details
      const describeCommand = [
        "gcloud run jobs executions describe",
        executionName,
        `--region=${this.config.region}`,
        `--project=${this.config.projectId}`,
        `--format=json`,
      ];

      const executionInfo = JSON.parse(
        execSync(describeCommand.join(" "), { 
          encoding: "utf-8",
          env: {
            ...process.env,
            CLOUDSDK_AUTH_ACCESS_TOKEN: accessToken,
          },
        })
      );

      // Get logs from Cloud Logging - filter by execution name to avoid duplicates
      // Note: Cloud Logging has a delay, so we retry a few times
      let logs: any[] = [];
      let retries = 3;
      
      while (retries > 0 && logs.length === 0) {
        try {
          const logsCommand = [
            "gcloud logging read",
            `'resource.type="cloud_run_job" AND labels."run.googleapis.com/execution_name"="${executionName}"'`,
            `--project=${this.config.projectId}`,
            `--limit=1000`,
            `--format=json`,
          ];

          const logsJson = execSync(logsCommand.join(" "), { 
            encoding: "utf-8",
            env: {
              ...process.env,
              CLOUDSDK_AUTH_ACCESS_TOKEN: accessToken,
            },
          });
          logs = JSON.parse(logsJson || "[]");
          
          if (logs.length === 0 && retries > 1) {
            // Wait 2 seconds before retry
            await new Promise(resolve => setTimeout(resolve, 2000));
          }
        } catch (error) {
          this.logger.warn(
            { task_id: taskId, retries_left: retries - 1 },
            "Failed to fetch logs, retrying..."
          );
          if (retries > 1) {
            await new Promise(resolve => setTimeout(resolve, 2000));
          }
        }
        retries--;
      }

      // Extract stdout/stderr from logs
      let stdout = "";
      let stderr = "";
      let fullLogs = "";

      for (const entry of logs) {
        const text = entry.textPayload || entry.jsonPayload?.message || "";
        fullLogs += text + "\n";
        
        if (entry.severity === "ERROR" || entry.severity === "WARNING") {
          stderr += text + "\n";
        } else {
          stdout += text + "\n";
        }
      }

      // Extract exit code from execution status
      // Check if all tasks succeeded
      const taskCount = executionInfo.status?.taskCount || 0;
      const succeededCount = executionInfo.status?.succeededCount || 0;
      const failedCount = executionInfo.status?.failedCount || 0;
      
      // Exit code 0 if execution succeeded (succeededCount > 0 and no failures)
      // or if all tasks succeeded when taskCount is available
      const exitCode = 
        (succeededCount > 0 && failedCount === 0) ? 0 :
        (taskCount > 0 && succeededCount === taskCount) ? 0 : -1;

      return {
        stdout: stdout.trim(),
        stderr: stderr.trim(),
        exitCode,
        executionName,
        logs: fullLogs.trim(),
      };
    } catch (error) {
      this.logger.error(
        {
          task_id: taskId,
          execution_name: executionName,
          error: error instanceof Error ? error.message : String(error),
        },
        "Failed to get execution result"
      );
      throw error;
    }
  }

  /**
   * Deletes a Cloud Run Job
   */
  async deleteJob(taskId: string): Promise<void> {
    const jobName = this.jobNames.get(taskId);

    if (!jobName) {
      this.logger.debug({ task_id: taskId }, "No job to delete");
      return;
    }

    this.logger.debug({ task_id: taskId, job_name: jobName }, "Deleting job");

    try {
      const accessToken = this.getAccessToken();
      
      execSync(
        [
          "gcloud run jobs delete",
          jobName,
          `--region=${this.config.region}`,
          `--project=${this.config.projectId}`,
          "--quiet",
        ].join(" "),
        { 
          encoding: "utf-8",
          env: {
            ...process.env,
            CLOUDSDK_AUTH_ACCESS_TOKEN: accessToken,
          },
        }
      );

      this.jobNames.delete(taskId);
      this.executionNames.delete(taskId);

      this.logger.info({ task_id: taskId, job_name: jobName }, "Job deleted");
    } catch (error) {
      this.logger.error(
        {
          task_id: taskId,
          job_name: jobName,
          error: error instanceof Error ? error.message : String(error),
        },
        "Failed to delete job"
      );
      // Don't throw - cleanup is best effort
    }
  }

  /**
   * Cleans up all jobs
   */
  async cleanup(): Promise<void> {
    this.logger.info("Cleaning up all Cloud Run Jobs");

    const taskIds = Array.from(this.jobNames.keys());
    
    await Promise.all(
      taskIds.map((taskId) => this.deleteJob(taskId).catch(() => {}))
    );

    this.logger.info("Cleanup completed");
  }
}
