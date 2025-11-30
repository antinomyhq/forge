import type { DaytonaOrchestrator } from "./daytona-orchestrator.js";
import type { RemoteBuilder } from "./remote-builder.js";
import type { DistributedTaskUnit } from "./task-distributor.js";
import type { Logger } from "pino";
import pLimit from "p-limit";

/**
 * Workspace state in the pool
 */
export enum WorkspaceState {
  Idle = "idle",
  Building = "building",
  Executing = "executing",
  Failed = "failed",
}

/**
 * Workspace information in the pool
 */
export interface PooledWorkspace {
  id: string;
  state: WorkspaceState;
  createdAt: number;
  lastUsedAt: number;
  taskCount: number;
  errors: string[];
}

/**
 * Configuration for workspace pool
 */
export interface PoolConfig {
  maxConcurrency: number;
  maxRetries?: number;
  retryDelayMs?: number;
  enableGracefulShutdown?: boolean;
}

/**
 * Manages a pool of Daytona workspaces with concurrency control
 */
export class WorkspacePool {
  private orchestrator: DaytonaOrchestrator;
  private builder: RemoteBuilder;
  private logger: Logger;
  private config: PoolConfig;
  private workspaces: Map<string, PooledWorkspace> = new Map();
  private limit: ReturnType<typeof pLimit>;
  private shutdownRequested: boolean = false;

  constructor(
    orchestrator: DaytonaOrchestrator,
    builder: RemoteBuilder,
    config: PoolConfig,
    logger: Logger
  ) {
    this.orchestrator = orchestrator;
    this.builder = builder;
    this.config = {
      maxRetries: 3,
      retryDelayMs: 1000,
      enableGracefulShutdown: true,
      ...config,
    };
    this.logger = logger;
    this.limit = pLimit(config.maxConcurrency);

    // Setup graceful shutdown
    if (this.config.enableGracefulShutdown) {
      this.setupGracefulShutdown();
    }
  }

  /**
   * Creates and prepares a workspace in the pool
   */
  async createWorkspace(retries: number = 0): Promise<string> {
    if (this.shutdownRequested) {
      throw new Error("Pool is shutting down");
    }

    const workspaceId = `workspace-${Date.now()}-${Math.random()
      .toString(36)
      .substring(7)}`;

    try {
      this.logger.info({ workspace_id: workspaceId }, "Creating workspace");

      // Add to pool with building state
      this.workspaces.set(workspaceId, {
        id: workspaceId,
        state: WorkspaceState.Building,
        createdAt: Date.now(),
        lastUsedAt: Date.now(),
        taskCount: 0,
        errors: [],
      });

      // Create workspace
      await this.orchestrator.createWorkspace(workspaceId);

      // Build binary (skip if using custom image with pre-built binary)
      if (this.builder) {
        await this.builder.ensureBinaryBuilt(workspaceId);
      }

      // Update state to idle
      const workspace = this.workspaces.get(workspaceId)!;
      workspace.state = WorkspaceState.Idle;

      this.logger.info(
        { workspace_id: workspaceId },
        "Workspace ready in pool"
      );

      return workspaceId;
    } catch (error) {
      const errorMessage =
        error instanceof Error ? error.message : String(error);

      this.logger.error(
        {
          workspace_id: workspaceId,
          error: errorMessage,
          retries,
        },
        "Workspace creation failed"
      );

      // Update workspace with error
      const workspace = this.workspaces.get(workspaceId);
      if (workspace) {
        workspace.state = WorkspaceState.Failed;
        workspace.errors.push(errorMessage);
      }

      // Retry if configured
      if (retries < (this.config.maxRetries || 0)) {
        this.logger.info(
          {
            workspace_id: workspaceId,
            retry_attempt: retries + 1,
            max_retries: this.config.maxRetries,
          },
          "Retrying workspace creation"
        );

        await new Promise((resolve) =>
          setTimeout(resolve, this.config.retryDelayMs)
        );

        return this.createWorkspace(retries + 1);
      }

      throw error;
    }
  }

  /**
   * Creates multiple workspaces in parallel
   */
  async createWorkspaces(count: number): Promise<string[]> {
    this.logger.info(
      { count, max_concurrency: this.config.maxConcurrency },
      "Creating workspace pool"
    );

    const promises: Promise<string>[] = [];

    for (let i = 0; i < count; i++) {
      promises.push(this.limit(() => this.createWorkspace()));
    }

    const results = await Promise.allSettled(promises);

    const successful = results
      .filter((r) => r.status === "fulfilled")
      .map((r) => (r as PromiseFulfilledResult<string>).value);

    const failed = results.filter((r) => r.status === "rejected").length;

    this.logger.info(
      {
        requested: count,
        successful: successful.length,
        failed,
      },
      "Workspace pool created"
    );

    if (successful.length === 0) {
      throw new Error("Failed to create any workspaces");
    }

    return successful;
  }

  /**
   * Assigns a task to a workspace
   */
  assignTask(workspaceId: string, taskUnit: DistributedTaskUnit): void {
    const workspace = this.workspaces.get(workspaceId);
    if (!workspace) {
      throw new Error(`Workspace ${workspaceId} not found in pool`);
    }

    workspace.state = WorkspaceState.Executing;
    workspace.lastUsedAt = Date.now();
    workspace.taskCount++;

    this.logger.debug(
      {
        workspace_id: workspaceId,
        task_id: taskUnit.id,
        task_count: workspace.taskCount,
      },
      "Task assigned to workspace"
    );
  }

  /**
   * Marks a workspace as idle after task completion
   */
  releaseWorkspace(workspaceId: string, error?: string): void {
    const workspace = this.workspaces.get(workspaceId);
    if (!workspace) {
      this.logger.warn(
        { workspace_id: workspaceId },
        "Workspace not found for release"
      );
      return;
    }

    if (error) {
      workspace.state = WorkspaceState.Failed;
      workspace.errors.push(error);
      this.logger.warn(
        { workspace_id: workspaceId, error },
        "Workspace released with error"
      );
    } else {
      workspace.state = WorkspaceState.Idle;
      this.logger.debug(
        { workspace_id: workspaceId },
        "Workspace released"
      );
    }
  }

  /**
   * Gets pool statistics
   */
  getStats(): {
    total: number;
    idle: number;
    building: number;
    executing: number;
    failed: number;
    totalTasks: number;
  } {
    const workspaces = Array.from(this.workspaces.values());

    return {
      total: workspaces.length,
      idle: workspaces.filter((w) => w.state === WorkspaceState.Idle)
        .length,
      building: workspaces.filter(
        (w) => w.state === WorkspaceState.Building
      ).length,
      executing: workspaces.filter(
        (w) => w.state === WorkspaceState.Executing
      ).length,
      failed: workspaces.filter((w) => w.state === WorkspaceState.Failed)
        .length,
      totalTasks: workspaces.reduce((sum, w) => sum + w.taskCount, 0),
    };
  }

  /**
   * Destroys all workspaces in the pool
   */
  async destroyAll(): Promise<void> {
    this.logger.info(
      { count: this.workspaces.size },
      "Destroying all workspaces in pool"
    );

    const workspaceIds = Array.from(this.workspaces.keys());

    await Promise.allSettled(
      workspaceIds.map((id) =>
        this.limit(() => this.orchestrator.destroyWorkspace(id))
      )
    );

    this.workspaces.clear();

    this.logger.info("All workspaces destroyed");
  }

  /**
   * Requests graceful shutdown
   */
  async shutdown(): Promise<void> {
    this.logger.info("Initiating graceful shutdown");
    this.shutdownRequested = true;

    // Wait for pending tasks to complete (with timeout)
    const maxWaitMs = 30000; // 30 seconds
    const startTime = Date.now();

    while (Date.now() - startTime < maxWaitMs) {
      const stats = this.getStats();
      if (stats.executing === 0 && stats.building === 0) {
        break;
      }

      this.logger.info(
        {
          executing: stats.executing,
          building: stats.building,
        },
        "Waiting for tasks to complete"
      );

      await new Promise((resolve) => setTimeout(resolve, 1000));
    }

    // Destroy all workspaces
    await this.destroyAll();

    this.logger.info("Shutdown complete");
  }

  /**
   * Sets up graceful shutdown handlers
   */
  private setupGracefulShutdown(): void {
    const shutdownHandler = async () => {
      this.logger.info("Received shutdown signal");
      await this.shutdown();
      process.exit(0);
    };

    process.on("SIGINT", shutdownHandler);
    process.on("SIGTERM", shutdownHandler);
  }
}
