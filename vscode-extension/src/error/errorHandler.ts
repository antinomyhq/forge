import * as vscode from 'vscode';

/**
 * Error categories for better handling
 */
export enum ErrorCategory {
    Connection = 'connection',
    Authentication = 'authentication',
    RateLimit = 'rateLimit',
    Timeout = 'timeout',
    Validation = 'validation',
    Unknown = 'unknown'
}

/**
 * Enhanced error with recovery suggestions
 */
export class ForgeError extends Error {
    constructor(
        message: string,
        public category: ErrorCategory,
        public originalError?: unknown,
        public recoverySuggestions?: string[]
    ) {
        super(message);
        this.name = 'ForgeError';
    }
}

/**
 * Error handler with recovery suggestions
 */
export class ErrorHandler {
    constructor(private outputChannel: vscode.OutputChannel) {}

    /**
     * Handle error with user-friendly message and recovery options
     */
    async handleError(error: unknown, context?: string): Promise<void> {
        const forgeError = this.categorizeError(error);
        
        // Log detailed error
        this.outputChannel.appendLine(`[ERROR] ${context || 'Unknown context'}`);
        this.outputChannel.appendLine(`Category: ${forgeError.category}`);
        this.outputChannel.appendLine(`Message: ${forgeError.message}`);
        if (forgeError.originalError) {
            this.outputChannel.appendLine(`Original: ${JSON.stringify(forgeError.originalError)}`);
        }

        // Show user-friendly message with actions
        await this.showErrorWithActions(forgeError, context);
    }

    /**
     * Categorize error and add recovery suggestions
     */
    private categorizeError(error: unknown): ForgeError {
        const errorStr = String(error);
        const errorObj = error as any;

        // Connection errors
        if (errorStr.includes('ECONNREFUSED') || 
            errorStr.includes('ENOTFOUND') ||
            errorStr.includes('spawn') ||
            errorStr.includes('Server not running')) {
            return new ForgeError(
                'Cannot connect to forge-app-server',
                ErrorCategory.Connection,
                error,
                [
                    'Check if server binary exists',
                    'Verify server path in settings',
                    'Try restarting the extension',
                    'Check server logs in Output panel'
                ]
            );
        }

        // Authentication errors
        if (errorStr.includes('401') || 
            errorStr.includes('unauthorized') ||
            errorStr.includes('authentication') ||
            errorStr.includes('API key')) {
            return new ForgeError(
                'Authentication failed',
                ErrorCategory.Authentication,
                error,
                [
                    'Verify your API key is correct',
                    'Reconfigure provider credentials',
                    'Check if API key has expired'
                ]
            );
        }

        // Rate limit errors
        if (errorStr.includes('429') || 
            errorStr.includes('rate limit') ||
            errorStr.includes('too many requests')) {
            return new ForgeError(
                'Rate limit exceeded',
                ErrorCategory.RateLimit,
                error,
                [
                    'Wait a few minutes and try again',
                    'Switch to a different model',
                    'Use a different provider'
                ]
            );
        }

        // Timeout errors
        if (errorStr.includes('timeout') || 
            errorStr.includes('ETIMEDOUT') ||
            errorObj?.code === 'ETIMEDOUT') {
            return new ForgeError(
                'Request timed out',
                ErrorCategory.Timeout,
                error,
                [
                    'Check your internet connection',
                    'Try again in a moment',
                    'Increase timeout in settings'
                ]
            );
        }

        // Validation errors
        if (errorStr.includes('required') || 
            errorStr.includes('invalid') ||
            errorStr.includes('validation')) {
            return new ForgeError(
                'Invalid request',
                ErrorCategory.Validation,
                error,
                [
                    'Check your input',
                    'Verify file paths are correct',
                    'Report issue if persists'
                ]
            );
        }

        // Unknown errors
        return new ForgeError(
            errorStr.substring(0, 200), // Truncate long errors
            ErrorCategory.Unknown,
            error,
            [
                'Check Output panel for details',
                'Try restarting the extension',
                'Report issue on GitHub'
            ]
        );
    }

    /**
     * Show error message with recovery action buttons
     */
    private async showErrorWithActions(error: ForgeError, context?: string): Promise<void> {
        const prefix = context ? `${context}: ` : '';
        const message = `${prefix}${error.message}`;

        // Create action buttons based on error category
        const actions: string[] = [];

        switch (error.category) {
            case ErrorCategory.Connection:
                actions.push('Restart Server', 'Open Settings', 'View Logs');
                break;
            case ErrorCategory.Authentication:
                actions.push('Configure Provider', 'View Logs');
                break;
            case ErrorCategory.RateLimit:
                actions.push('Switch Model', 'View Logs');
                break;
            case ErrorCategory.Timeout:
                actions.push('Retry', 'View Logs');
                break;
            default:
                actions.push('View Logs', 'Report Issue');
        }

        // Show error with actions
        const selected = await vscode.window.showErrorMessage(
            message,
            { modal: false },
            ...actions
        );

        // Handle action selection
        if (selected) {
            await this.handleAction(selected, error);
        }
    }

    /**
     * Handle recovery action
     */
    private async handleAction(action: string, _error: ForgeError): Promise<void> {
        switch (action) {
            case 'Restart Server':
                await vscode.commands.executeCommand('workbench.action.reloadWindow');
                break;

            case 'Open Settings':
                await vscode.commands.executeCommand('workbench.action.openSettings', 'forgecode');
                break;

            case 'Configure Provider':
                await vscode.commands.executeCommand('forgecode.selectProvider');
                break;

            case 'Switch Model':
                await vscode.commands.executeCommand('forgecode.selectModel');
                break;

            case 'View Logs':
                this.outputChannel.show();
                break;

            case 'Retry':
                vscode.window.showInformationMessage('Please try your action again');
                break;

            case 'Report Issue':
                await vscode.env.openExternal(
                    vscode.Uri.parse('https://github.com/forgecode/forgecode/issues/new')
                );
                break;
        }
    }

    /**
     * Show warning with optional actions
     */
    async showWarning(message: string, ...actions: string[]): Promise<string | undefined> {
        return await vscode.window.showWarningMessage(message, ...actions);
    }

    /**
     * Show info message
     */
    showInfo(message: string): void {
        vscode.window.showInformationMessage(message);
    }

    /**
     * Log debug message
     */
    debug(message: string): void {
        this.outputChannel.appendLine(`[DEBUG] ${message}`);
    }

    /**
     * Log info message
     */
    info(message: string): void {
        this.outputChannel.appendLine(`[INFO] ${message}`);
    }

    /**
     * Log warning
     */
    warn(message: string): void {
        this.outputChannel.appendLine(`[WARN] ${message}`);
    }
}
