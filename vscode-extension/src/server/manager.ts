import * as vscode from 'vscode';
import { spawn, ChildProcess } from 'child_process';
import { EventEmitter } from 'events';

export interface ServerManagerConfig {
    serverPath: string;
    logLevel: string;
}

export interface ServerManagerEvents {
    stdout: (data: Buffer) => void;
    stderr: (data: Buffer) => void;
    exit: (code: number | null) => void;
    error: (error: Error) => void;
}

/**
 * Manages the forge-app-server process lifecycle
 * - Spawns and monitors the server process
 * - Auto-restarts on crash (up to max attempts)
 * - Provides stdio streams for communication
 */
export class ServerManager extends EventEmitter {
    private process: ChildProcess | null = null;
    private config: ServerManagerConfig;
    private restartAttempts = 0;
    private maxRestartAttempts = 3;
    private isManualStop = false;
    private outputChannel: vscode.OutputChannel;

    constructor(config: ServerManagerConfig, outputChannel: vscode.OutputChannel) {
        super();
        this.config = config;
        this.outputChannel = outputChannel;
    }

    /**
     * Start the server process
     */
    async start(): Promise<void> {
        if (this.process) {
            throw new Error('Server is already running');
        }

        this.isManualStop = false;
        this.outputChannel.appendLine(`Starting forge-app-server: ${this.config.serverPath}`);

        try {
            this.process = spawn(this.config.serverPath, [], {
                stdio: ['pipe', 'pipe', 'pipe'],
                env: {
                    ...process.env,
                    RUST_LOG: this.config.logLevel,
                },
            });

            const pid = this.process.pid;
            this.outputChannel.appendLine(`Server process started with PID: ${pid}`);

            // Set up event handlers
            this.setupProcessHandlers();

            // Wait for process to stabilize
            await this.waitForStabilization();

        } catch (error) {
            this.outputChannel.appendLine(`Failed to start server: ${error}`);
            throw error;
        }
    }

    /**
     * Stop the server process
     */
    async stop(): Promise<void> {
        if (!this.process) {
            return;
        }

        this.isManualStop = true;
        this.outputChannel.appendLine('Stopping server...');

        return new Promise((resolve) => {
            if (!this.process) {
                resolve();
                return;
            }

            this.process.once('exit', () => {
                this.outputChannel.appendLine('Server stopped');
                this.process = null;
                resolve();
            });

            // Try graceful shutdown first
            this.process.kill('SIGTERM');

            // Force kill after 5 seconds
            setTimeout(() => {
                if (this.process) {
                    this.outputChannel.appendLine('Force killing server...');
                    this.process.kill('SIGKILL');
                }
            }, 5000);
        });
    }

    /**
     * Restart the server process
     */
    async restart(): Promise<void> {
        await this.stop();
        await this.start();
    }

    /**
     * Check if server is running
     */
    isRunning(): boolean {
        return this.process !== null && !this.process.killed;
    }

    /**
     * Get stdin stream for writing to server
     */
    getStdin(): NodeJS.WritableStream | null {
        return this.process?.stdin || null;
    }

    /**
     * Get stdout stream for reading from server
     */
    getStdout(): NodeJS.ReadableStream | null {
        return this.process?.stdout || null;
    }

    /**
     * Get stderr stream for logging
     */
    getStderr(): NodeJS.ReadableStream | null {
        return this.process?.stderr || null;
    }

    /**
     * Set up process event handlers
     */
    private setupProcessHandlers(): void {
        if (!this.process) {
            return;
        }

        // Handle stdout data
        this.process.stdout?.on('data', (data: Buffer) => {
            this.emit('stdout', data);
        });

        // Handle stderr data (logs)
        this.process.stderr?.on('data', (data: Buffer) => {
            const text = data.toString();
            this.outputChannel.append(`Server stderr: ${text}`);
            this.emit('stderr', data);
        });

        // Handle process exit
        this.process.on('exit', (code) => {
            this.outputChannel.appendLine(`Server exited with code: ${code}`);
            this.emit('exit', code);
            this.process = null;

            // Auto-restart if not manual stop
            if (!this.isManualStop && code !== 0) {
                this.handleCrash();
            }
        });

        // Handle process errors
        this.process.on('error', (error) => {
            this.outputChannel.appendLine(`Server error: ${error.message}`);
            this.emit('error', error);
        });
    }

    /**
     * Handle server crash and attempt restart
     */
    private async handleCrash(): Promise<void> {
        this.restartAttempts++;

        if (this.restartAttempts <= this.maxRestartAttempts) {
            this.outputChannel.appendLine(
                `Server crashed. Restarting (attempt ${this.restartAttempts}/${this.maxRestartAttempts})...`
            );

            // Wait before restarting
            await new Promise(resolve => setTimeout(resolve, 1000 * this.restartAttempts));

            try {
                await this.start();
                this.restartAttempts = 0; // Reset on successful start
            } catch (error) {
                this.outputChannel.appendLine(`Failed to restart: ${error}`);
            }
        } else {
            this.outputChannel.appendLine(
                `Server crashed ${this.maxRestartAttempts} times. Giving up.`
            );
            vscode.window.showErrorMessage(
                'ForgeCode server crashed multiple times. Please check logs and restart manually.'
            );
        }
    }

    /**
     * Wait for process to stabilize after startup
     */
    private async waitForStabilization(): Promise<void> {
        return new Promise((resolve) => {
            setTimeout(resolve, 500);
        });
    }

    /**
     * Dispose of resources
     */
    dispose(): void {
        this.isManualStop = true;
        if (this.process) {
            this.process.kill('SIGTERM');
            this.process = null;
        }
        this.removeAllListeners();
    }
}
