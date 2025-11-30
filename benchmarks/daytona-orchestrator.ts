import { Daytona } from "@daytonaio/sdk";
import * as fs from "fs";
import * as path from "path";
import type { Logger } from "pino";
import { execSync } from "child_process";
import * as crypto from "crypto";

export interface DaytonaConfig {
  apiKey: string;
  language?: string;
  image?: string;
  githubUsername?: string; // GitHub username for GHCR push
  autoPublish?: boolean; // Auto-publish to GHCR (default: true)
}

export interface WorkspaceContext {
  sandboxId: string;
  sandbox: any; // Daytona Sandbox instance
}

/**
 * Orchestrates Daytona workspace lifecycle for distributed task execution
 */
export class DaytonaOrchestrator {
  private daytona: Daytona;
  private logger: Logger;
  private workspaces: Map<string, WorkspaceContext> = new Map();
  private customImage: string | null = null;
  private sourcePath: string;
  private config: DaytonaConfig;

  constructor(config: DaytonaConfig, logger: Logger, sourcePath: string) {
    this.daytona = new Daytona({ apiKey: config.apiKey });
    this.logger = logger;
    this.sourcePath = sourcePath;
    this.config = config;
  }

  /**
   * Builds a custom Docker image with the forge binary pre-compiled
   * This runs locally on the user's machine and creates a Linux binary
   * Optionally pushes to GitHub Container Registry for Daytona access
   */
  async buildCustomImage(): Promise<string> {
    this.logger.info("Building custom Docker image with forge binary...");

    try {
      // Generate a unique tag based on source code hash
      const sourceHash = this.getSourceHash();
      const imageTag = `forge-eval:${sourceHash}`;

      // Check if we should publish to GHCR
      const autoPublish = this.config.autoPublish !== false; // default true
      
      if (autoPublish) {
        // Use GHCR for Daytona access
        const ghcrImage = await this.buildAndPushToGHCR(sourceHash);
        this.customImage = ghcrImage;
        return ghcrImage;
      } else {
        // Build local image only (won't work with Daytona)
        this.logger.warn("autoPublish is disabled - image will be local only and won't work with Daytona");
        return await this.buildLocalImage(imageTag);
      }
    } catch (error) {
      this.logger.error(
        { error: error instanceof Error ? error.message : String(error) },
        "Failed to build custom Docker image"
      );
      throw error;
    }
  }

  /**
   * Builds Docker image and pushes to GitHub Container Registry
   */
  private async buildAndPushToGHCR(sourceHash: string): Promise<string> {
    this.logger.info("Building and pushing to GitHub Container Registry...");

    // Get GitHub username
    let githubUsername = this.config.githubUsername;
    
    if (!githubUsername) {
      this.logger.info("Getting GitHub username from gh CLI...");
      try {
        githubUsername = execSync("gh api user --jq .login", { 
          encoding: "utf-8",
          stdio: ["pipe", "pipe", "pipe"]
        }).trim();
        this.logger.info({ username: githubUsername }, "GitHub username detected");
      } catch (error) {
        throw new Error(
          "Failed to get GitHub username. Please:\n" +
          "1. Install GitHub CLI: https://cli.github.com\n" +
          "2. Run: gh auth login\n" +
          "Or provide githubUsername in config"
        );
      }
    }

    // Use :latest tag for simpler caching and deployment
    const localTag = `forge-eval:latest`;
    const ghcrImage = `ghcr.io/${githubUsername}/forge-eval:latest`;
    const ghcrLatest = `ghcr.io/${githubUsername}/forge-eval:latest`;

    // Check if image already exists in GHCR
    this.logger.info({ image: ghcrImage }, "Checking if image exists in GHCR...");
    try {
      execSync(`docker manifest inspect ${ghcrImage}`, { 
        stdio: "pipe",
        encoding: "utf-8" 
      });
      this.logger.info({ image: ghcrImage }, "Image already exists in GHCR, skipping build");
      return ghcrImage;
    } catch {
      // Image doesn't exist, need to build and push
    }

    // Build the image locally for linux/amd64 (Daytona uses x86_64, not ARM)
    this.logger.info({ image: localTag }, "Building Docker image for linux/amd64...");
    execSync(
      `docker build --platform linux/amd64 -f Dockerfile.eval -t ${localTag} .`,
      {
        cwd: this.sourcePath,
        stdio: "inherit",
      }
    );

    // Tag for GHCR
    this.logger.info({ ghcr_image: ghcrImage }, "Tagging for GHCR...");
    execSync(`docker tag ${localTag} ${ghcrImage}`);
    execSync(`docker tag ${localTag} ${ghcrLatest}`);

    // Login to GHCR using gh CLI
    this.logger.info("Authenticating with GHCR using gh CLI...");
    try {
      const token = execSync("gh auth token", { 
        encoding: "utf-8",
        stdio: ["pipe", "pipe", "pipe"]
      }).trim();
      
      execSync(`echo "${token}" | docker login ghcr.io -u ${githubUsername} --password-stdin`, {
        stdio: "pipe",
        shell: "/bin/bash"
      });
      
      this.logger.info("GHCR authentication successful");
    } catch (error) {
      throw new Error(
        "Failed to authenticate with GHCR. Please run: gh auth login"
      );
    }

    // Push to GHCR
    this.logger.info({ image: ghcrImage }, "Pushing to GHCR...");
    try {
      execSync(`docker push ${ghcrImage}`, { stdio: "inherit" });
    } catch (error) {
      throw new Error(
        "Failed to push to GHCR. Please ensure your GitHub token has 'write:packages' scope.\n" +
        "Run: gh auth refresh -h github.com -s write:packages"
      );
    }
    
    this.logger.info({ image: ghcrLatest }, "Pushing latest tag...");
    try {
      execSync(`docker push ${ghcrLatest}`, { stdio: "inherit" });
    } catch (error) {
      // Latest tag push failure is not critical
      this.logger.warn({ error: error instanceof Error ? error.message : String(error) }, "Failed to push latest tag");
    }

    this.logger.info(
      { 
        image: ghcrImage,
        latest: ghcrLatest 
      },
      "Image published to GHCR successfully"
    );

    // Make the package public so Daytona can access it
    // Note: GitHub API doesn't provide a simple way to set package visibility
    // We'll provide instructions for manual setup
    this.logger.warn(
      "\n" +
      "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n" +
      "⚠️  IMPORTANT: Make the package public for Daytona to access it\n" +
      "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n" +
      "\n" +
      `1. Open: https://github.com/users/${githubUsername}/packages/container/package/forge-eval\n` +
      "2. Click 'Package settings' (bottom right)\n" +
      "3. Scroll to 'Danger Zone' → Click 'Change visibility'\n" +
      "4. Select 'Public' and confirm\n" +
      "\n" +
      "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n"
    );

    return ghcrImage;
  }

  /**
   * Builds Docker image locally only (for development/testing)
   */
  private async buildLocalImage(imageTag: string): Promise<string> {
    // Check if image already exists locally
    try {
      execSync(`docker image inspect ${imageTag}`, { stdio: "pipe" });
      this.logger.info({ image: imageTag }, "Local image already exists, skipping build");
      this.customImage = imageTag;
      return imageTag;
    } catch {
      // Image doesn't exist, build it
    }

    this.logger.info({ image: imageTag }, "Building Docker image locally for linux/amd64...");
    
    execSync(
      `docker build --platform linux/amd64 -f Dockerfile.eval -t ${imageTag} .`,
      {
        cwd: this.sourcePath,
        stdio: "inherit",
      }
    );

    this.logger.info({ image: imageTag }, "Docker image built successfully");
    this.customImage = imageTag;
    return imageTag;
  }

  /**
   * Generates a hash of the source code to create unique image tags
   */
  private getSourceHash(): string {
    const hash = crypto.createHash("sha256");
    
    // Hash key files to detect code changes
    const filesToHash = [
      "Cargo.toml",
      "Cargo.lock",
    ];

    for (const file of filesToHash) {
      const filePath = path.join(this.sourcePath, file);
      if (fs.existsSync(filePath)) {
        hash.update(fs.readFileSync(filePath));
      }
    }

    return hash.digest("hex").substring(0, 12);
  }

  /**
   * Creates a new Daytona workspace
   */
  async createWorkspace(workspaceId: string): Promise<string> {
    this.logger.info({ workspace_id: workspaceId }, "Creating Daytona workspace");

    try {
      // Use custom image if available, otherwise use default
      const image = this.customImage || "debian:bookworm-slim";
      
      this.logger.info({ workspace_id: workspaceId, image }, "Using image for workspace");

      // Create sandbox with extended timeout for custom image pulls
      // NOTE: Daytona requires images from public registries (Docker Hub, GHCR, etc.)
      // Local images (e.g., forge-eval:702422fc3742) will timeout
      const sandbox = await this.daytona.create({
        image,
        resources: {
          cpu: 2,
          memory: 2, // 2GB RAM
        },
        autoStopInterval: 60, // Auto-stop after 60 minutes
        labels: {
          "workspace-id": workspaceId,
          "project": "code-forge-evals",
        },
      });

      const context: WorkspaceContext = {
        sandboxId: sandbox.id,
        sandbox,
      };

      this.workspaces.set(workspaceId, context);

      this.logger.info(
        { workspace_id: workspaceId, sandbox_id: sandbox.id },
        "Workspace created successfully"
      );

      return sandbox.id;
    } catch (error) {
      this.logger.error(
        {
          workspace_id: workspaceId,
          error: error instanceof Error ? error.message : String(error),
        },
        "Failed to create workspace"
      );
      throw error;
    }
  }

  /**
   * Executes a command on the workspace
   */
  async executeCommand(
    workspaceId: string,
    command: string,
    options?: { timeout?: number }
  ): Promise<{ stdout: string; stderr: string; exitCode: number }> {
    const context = this.workspaces.get(workspaceId);
    if (!context) {
      throw new Error(`Workspace ${workspaceId} not found`);
    }

    this.logger.debug(
      { workspace_id: workspaceId, command },
      "Executing command"
    );

    try {
      const result = await context.sandbox.process.executeCommand(command);

      const stdout = result.artifacts?.stdout || "";
      const stderr = result.artifacts?.stderr || "";
      const exitCode = result.code || 0;

      this.logger.debug(
        { 
          workspace_id: workspaceId, 
          exit_code: exitCode,
          stdout_length: stdout.length,
          stderr_length: stderr.length,
          has_stdout: stdout.length > 0,
          has_stderr: stderr.length > 0
        },
        "Command executed"
      );

      return {
        stdout,
        stderr,
        exitCode,
      };
    } catch (error) {
      this.logger.error(
        {
          workspace_id: workspaceId,
          command,
          error: error instanceof Error ? error.message : String(error),
        },
        "Command execution failed"
      );
      throw error;
    }
  }

  /**
   * Downloads a file from the workspace
   */
  async downloadFile(
    workspaceId: string,
    remotePath: string
  ): Promise<Buffer> {
    const context = this.workspaces.get(workspaceId);
    if (!context) {
      throw new Error(`Workspace ${workspaceId} not found`);
    }

    try {
      const content = await context.sandbox.fs.downloadFile(remotePath);
      return Buffer.from(content);
    } catch (error) {
      this.logger.error(
        {
          workspace_id: workspaceId,
          remote_path: remotePath,
          error: error instanceof Error ? error.message : String(error),
        },
        "Failed to download file"
      );
      throw error;
    }
  }

  /**
   * Destroys a workspace and cleans up resources
   */
  async destroyWorkspace(workspaceId: string): Promise<void> {
    const context = this.workspaces.get(workspaceId);
    if (!context) {
      this.logger.warn(
        { workspace_id: workspaceId },
        "Workspace not found, skipping cleanup"
      );
      return;
    }

    this.logger.info({ workspace_id: workspaceId }, "Destroying workspace");

    try {
      await context.sandbox.stop();
      this.workspaces.delete(workspaceId);

      this.logger.info(
        { workspace_id: workspaceId },
        "Workspace destroyed successfully"
      );
    } catch (error) {
      this.logger.error(
        {
          workspace_id: workspaceId,
          error: error instanceof Error ? error.message : String(error),
        },
        "Failed to destroy workspace"
      );
      // Don't throw - best effort cleanup
    }
  }

  /**
   * Destroys all active workspaces
   */
  async destroyAllWorkspaces(): Promise<void> {
    this.logger.info("Destroying all workspaces");

    const workspaceIds = Array.from(this.workspaces.keys());
    await Promise.allSettled(
      workspaceIds.map((id) => this.destroyWorkspace(id))
    );

    this.logger.info("All workspaces destroyed");
  }
}
