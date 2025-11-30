import type { DaytonaOrchestrator } from "./daytona-orchestrator.js";
import type { Logger } from "pino";
import * as path from "path";

/**
 * Build status for a workspace
 */
export interface BuildStatus {
  workspaceId: string;
  isBuilt: boolean;
  buildTime?: number;
  binaryPath: string;
  error?: string;
}

/**
 * Manages remote builds with caching across workspaces
 */
export class RemoteBuilder {
  private orchestrator: DaytonaOrchestrator;
  private logger: Logger;
  private buildCache: Map<string, BuildStatus> = new Map();
  private sourcePath: string;

  constructor(
    orchestrator: DaytonaOrchestrator,
    sourcePath: string,
    logger: Logger
  ) {
    this.orchestrator = orchestrator;
    this.sourcePath = sourcePath;
    this.logger = logger;
  }

  /**
   * Ensures binary is built on the workspace, using cache if available
   */
  async ensureBinaryBuilt(workspaceId: string): Promise<BuildStatus> {
    // Check if already built
    const cached = this.buildCache.get(workspaceId);
    if (cached && cached.isBuilt) {
      this.logger.info(
        { workspace_id: workspaceId },
        "Using cached binary"
      );
      return cached;
    }

    this.logger.info(
      { workspace_id: workspaceId },
      "Building binary on workspace"
    );

    const startTime = Date.now();

    try {
      // Transfer source code
      await this.orchestrator.transferSourceCode(
        workspaceId,
        this.sourcePath
      );

      // Build the binary
      await this.orchestrator.buildBinary(workspaceId);

      const buildTime = Date.now() - startTime;

      const status: BuildStatus = {
        workspaceId,
        isBuilt: true,
        buildTime,
        binaryPath: "/tmp/code-forge/target/debug/forge",
      };

      this.buildCache.set(workspaceId, status);

      this.logger.info(
        {
          workspace_id: workspaceId,
          build_time_ms: buildTime,
        },
        "Binary built successfully"
      );

      return status;
    } catch (error) {
      const buildTime = Date.now() - startTime;
      const errorMessage =
        error instanceof Error ? error.message : String(error);

      const status: BuildStatus = {
        workspaceId,
        isBuilt: false,
        buildTime,
        binaryPath: "",
        error: errorMessage,
      };

      this.buildCache.set(workspaceId, status);

      this.logger.error(
        {
          workspace_id: workspaceId,
          build_time_ms: buildTime,
          error: errorMessage,
        },
        "Binary build failed"
      );

      throw error;
    }
  }

  /**
   * Gets build status for a workspace
   */
  getBuildStatus(workspaceId: string): BuildStatus | undefined {
    return this.buildCache.get(workspaceId);
  }

  /**
   * Clears build cache for a workspace
   */
  clearCache(workspaceId: string): void {
    this.buildCache.delete(workspaceId);
    this.logger.debug(
      { workspace_id: workspaceId },
      "Build cache cleared"
    );
  }

  /**
   * Gets statistics about builds
   */
  getStats(): {
    totalWorkspaces: number;
    successfulBuilds: number;
    failedBuilds: number;
    avgBuildTime: number;
  } {
    const statuses = Array.from(this.buildCache.values());
    const successful = statuses.filter((s) => s.isBuilt);
    const failed = statuses.filter((s) => !s.isBuilt);

    const avgBuildTime =
      successful.length > 0
        ? successful.reduce((sum, s) => sum + (s.buildTime || 0), 0) /
          successful.length
        : 0;

    return {
      totalWorkspaces: statuses.length,
      successfulBuilds: successful.length,
      failedBuilds: failed.length,
      avgBuildTime,
    };
  }

  /**
   * Pre-builds binaries on multiple workspaces in parallel
   */
  async preBuildWorkspaces(
    workspaceIds: string[],
    concurrency: number = 3
  ): Promise<void> {
    this.logger.info(
      { workspace_count: workspaceIds.length, concurrency },
      "Pre-building workspaces"
    );

    const chunks: string[][] = [];
    for (let i = 0; i < workspaceIds.length; i += concurrency) {
      chunks.push(workspaceIds.slice(i, i + concurrency));
    }

    for (const chunk of chunks) {
      await Promise.allSettled(
        chunk.map((id) => this.ensureBinaryBuilt(id))
      );
    }

    const stats = this.getStats();
    this.logger.info(
      {
        successful: stats.successfulBuilds,
        failed: stats.failedBuilds,
        avg_build_time_ms: Math.round(stats.avgBuildTime),
      },
      "Pre-build completed"
    );
  }
}
