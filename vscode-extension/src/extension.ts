import * as vscode from 'vscode';
import { ServerManager } from './server/manager';
import { JsonRpcClient } from './server/client';
import { ChatWebviewProvider } from './webview/provider';
import { Controller } from './controller';
import { ConversationTreeProvider } from './conversation/treeProvider';
import { FileContextManager } from './file/contextManager';
import { SettingsWebviewProvider } from './settings/webviewProvider';
import { AgentModelSelector } from './config/agentModelSelector';
import { ProviderManager } from './config/providerManager';
import { ErrorHandler } from './error/errorHandler';
import { ClientInfo, ServerCapabilities } from './generated';

let serverManager: ServerManager | null = null;
let rpcClient: JsonRpcClient | null = null;
let webviewProvider: ChatWebviewProvider | null = null;
let conversationTree: ConversationTreeProvider | null = null;
let fileContext: FileContextManager | null = null;
let settingsProvider: SettingsWebviewProvider | null = null;
let agentModelSelector: AgentModelSelector | null = null;
let providerManager: ProviderManager | null = null;
let controller: Controller | null = null;
let outputChannel: vscode.OutputChannel | null = null;
// @ts-expect-error - Will be used for error recovery in future enhancements
let errorHandler: ErrorHandler | null = null;

/**
 * Extension activation
 */
export async function activate(context: vscode.ExtensionContext): Promise<void> {
    console.log('🚀 ForgeCode extension activate() called!');
    
    // Create output channel for logging
    outputChannel = vscode.window.createOutputChannel('ForgeCode');
    outputChannel.show(); // Force show the output channel
    outputChannel.appendLine('🚀 ForgeCode extension activating...');
    outputChannel.appendLine(`Working directory: ${process.cwd()}`);

    // Create error handler
    errorHandler = new ErrorHandler(outputChannel);
    // errorHandler will be used for error recovery in future enhancements

    try {
        // Get configuration
        const config = vscode.workspace.getConfiguration('forgecode');
        const serverPath = getServerPath(config);
        const logLevel = config.get<string>('logLevel') || 'info';

        outputChannel.appendLine(`Using server path: ${serverPath}`);

        // Create server manager
        serverManager = new ServerManager(
            { serverPath, logLevel },
            outputChannel
        );

        // Start server
        await serverManager.start();

        // Create JSON-RPC client
        const stdin = serverManager.getStdin();
        const stdout = serverManager.getStdout();

        if (!stdin || !stdout) {
            throw new Error('Failed to get server stdio streams');
        }

        rpcClient = new JsonRpcClient(stdin, stdout);

        // Set up notification handler
        rpcClient.on('notification', (method: string, params: unknown) => {
            outputChannel?.appendLine(`Notification: ${method}`);
            handleNotification(method, params);
        });

        // Set up error handler
        rpcClient.on('error', (error: Error) => {
            outputChannel?.appendLine(`RPC Error: ${error.message}`);
        });

        rpcClient.on('warning', (message: string) => {
            outputChannel?.appendLine(`RPC Warning: ${message}`);
        });

        // Initialize connection
        await initializeConnection(rpcClient);

        // Create webview provider
        webviewProvider = new ChatWebviewProvider(context.extensionUri, outputChannel);
        
        // Register webview provider
        context.subscriptions.push(
            vscode.window.registerWebviewViewProvider(
                ChatWebviewProvider.viewType,
                webviewProvider
            )
        );

        // Create conversation tree provider
        conversationTree = new ConversationTreeProvider(outputChannel);
        context.subscriptions.push(
            vscode.window.registerTreeDataProvider('forgecode.conversations', conversationTree)
        );

        // Create file context manager
        fileContext = new FileContextManager(context, outputChannel);
        context.subscriptions.push(fileContext);

        // Create settings provider
        settingsProvider = new SettingsWebviewProvider(context.extensionUri, outputChannel);
        context.subscriptions.push(
            vscode.window.registerWebviewViewProvider(
                SettingsWebviewProvider.viewType,
                settingsProvider
            )
        );

        // Create agent/model selector
        agentModelSelector = new AgentModelSelector(rpcClient, outputChannel);
        context.subscriptions.push(agentModelSelector);

        // Create provider manager
        providerManager = new ProviderManager(rpcClient, context, outputChannel);

        // Create controller
        controller = new Controller(
            rpcClient,
            webviewProvider,
            conversationTree,
            fileContext,
            outputChannel
        );

        // Connect webview events to controller
        webviewProvider.onReady(() => {
            controller?.handleWebviewReady().catch((error) => {
                outputChannel?.appendLine(`Error handling webview ready: ${error}`);
            });
        });

        webviewProvider.onSendMessage((text: string) => {
            controller?.handleSendMessage(text).catch((error) => {
                outputChannel?.appendLine(`Error handling send message: ${error}`);
            });
        });

        webviewProvider.onApproval((data: any) => {
            controller?.handleApproval(data.id, data.decision).catch((error) => {
                outputChannel?.appendLine(`Error handling approval: ${error}`);
            });
        });

        // Register commands
        registerCommands(context, controller);

        // Show success message
        outputChannel.appendLine('ForgeCode extension activated successfully!');
        vscode.window.showInformationMessage('ForgeCode extension activated!');

    } catch (error) {
        outputChannel?.appendLine(`Failed to activate: ${error}`);
        vscode.window.showErrorMessage(`Failed to activate ForgeCode: ${error}`);
        throw error;
    }
}

/**
 * Extension deactivation
 */
export async function deactivate(): Promise<void> {
    outputChannel?.appendLine('[INFO] ForgeCode extension deactivating');

    // Dispose RPC client
    if (rpcClient) {
        rpcClient.dispose();
        rpcClient = null;
    }

    // Stop server
    if (serverManager) {
        await serverManager.stop();
        serverManager.dispose();
        serverManager = null;
    }

    // Dispose output channel
    if (outputChannel) {
        outputChannel.appendLine('[INFO] ForgeCode extension deactivated');
        outputChannel.dispose();
        outputChannel = null;
    }
}

/**
 * Initialize JSON-RPC connection with server
 */
async function initializeConnection(client: JsonRpcClient): Promise<void> {
    outputChannel?.appendLine('Initializing connection...');

    // Get client info
    const clientInfo: ClientInfo = {
        name: 'forgecode-vscode',
        title: 'ForgeCode VSCode Extension',
        version: '0.1.0',
    };

    // Send initialize request
    try {
        const capabilities = await client.request<ServerCapabilities>(
            'initialize',
            { clientInfo }
        );

        outputChannel?.appendLine(`Server capabilities: ${JSON.stringify(capabilities)}`);

        // Send initialized notification
        client.notify('initialized', {});

        outputChannel?.appendLine('Connection initialized successfully');

    } catch (error) {
        outputChannel?.appendLine(`Failed to initialize: ${error}`);
        throw error;
    }
}

/**
 * Handle server notifications
 */
function handleNotification(method: string, params: unknown): void {
    // Log all notifications
    outputChannel?.appendLine(`[Notification] ${method}: ${JSON.stringify(params)}`);

    // Pass to controller
    if (controller) {
        controller.handleServerNotification(method, params);
    }
}

/**
 * Register VSCode commands
 */
function registerCommands(
    context: vscode.ExtensionContext,
    controller: Controller
): void {
    // Focus chat view command
    context.subscriptions.push(
        vscode.commands.registerCommand('forgecode.focusChat', async () => {
            // The webview will be shown automatically when the view is focused
            await vscode.commands.executeCommand('forgecode.chatView.focus');
        })
    );

    // Send message command
    context.subscriptions.push(
        vscode.commands.registerCommand('forgecode.sendMessage', async (text?: string) => {
            if (!text) {
                text = await vscode.window.showInputBox({
                    prompt: 'Enter your message',
                    placeHolder: 'Ask ForgeCode anything...'
                });
            }
            
            if (text) {
                await controller.handleSendMessage(text);
            }
        })
    );

    // New conversation command
    context.subscriptions.push(
        vscode.commands.registerCommand('forgecode.newConversation', async () => {
            await controller.startNewConversation();
            await vscode.commands.executeCommand('forgecode.chatView.focus');
        })
    );

    // Open conversation command
    context.subscriptions.push(
        vscode.commands.registerCommand('forgecode.openConversation', async (threadId: string) => {
            await controller.openConversation(threadId);
            await vscode.commands.executeCommand('forgecode.chatView.focus');
        })
    );

    // Delete conversation command
    context.subscriptions.push(
        vscode.commands.registerCommand('forgecode.deleteConversation', async (threadId: string) => {
            const confirm = await vscode.window.showWarningMessage(
                'Delete this conversation?',
                { modal: true },
                'Delete'
            );
            
            if (confirm === 'Delete') {
                await controller.deleteConversation(threadId);
            }
        })
    );

    // Tag file command
    context.subscriptions.push(
        vscode.commands.registerCommand('forgecode.tagFile', async (uri?: vscode.Uri) => {
            if (!uri && vscode.window.activeTextEditor) {
                uri = vscode.window.activeTextEditor.document.uri;
            }
            
            if (uri) {
                await fileContext?.tagFile(uri);
            }
        })
    );

    // Show tagged files command
    context.subscriptions.push(
        vscode.commands.registerCommand('forgecode.showTaggedFiles', async () => {
            await fileContext?.showTaggedFiles();
        })
    );

    // Clear tagged files command
    context.subscriptions.push(
        vscode.commands.registerCommand('forgecode.clearTaggedFiles', async () => {
            await fileContext?.clearAll();
        })
    );

    // Discover files command
    context.subscriptions.push(
        vscode.commands.registerCommand('forgecode.discoverFiles', async () => {
            await fileContext?.showFilePicker();
        })
    );

    // Agent selection command
    context.subscriptions.push(
        vscode.commands.registerCommand('forgecode.selectAgent', async () => {
            await agentModelSelector?.selectAgent();
        })
    );

    // Model selection command
    context.subscriptions.push(
        vscode.commands.registerCommand('forgecode.selectModel', async () => {
            await agentModelSelector?.selectModel();
        })
    );

    // Provider selection command
    context.subscriptions.push(
        vscode.commands.registerCommand('forgecode.selectProvider', async () => {
            await providerManager?.selectProvider();
        })
    );

    // List providers command
    context.subscriptions.push(
        vscode.commands.registerCommand('forgecode.listProviders', async () => {
            await providerManager?.listProviders();
        })
    );

    // Show logs command
    context.subscriptions.push(
        vscode.commands.registerCommand('forgecode.showLogs', () => {
            outputChannel?.show();
        })
    );
}

/**
 * Get server path from configuration
 * Checks for:
 * 1. Custom path from settings
 * 2. Development binary in workspace
 * 3. System PATH
 */
function getServerPath(config: vscode.WorkspaceConfiguration): string {
    // Check custom path
    const customPath = config.get<string>('serverPath');
    if (customPath && customPath !== 'forge-app-server') {
        return customPath;
    }

    // Check for development binary in workspace
    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (workspaceFolder) {
        const devPath = vscode.Uri.joinPath(
            workspaceFolder.uri,
            'target/debug/forge-app-server'
        );
        // Note: We can't check if file exists synchronously in VSCode API
        // So we just return the dev path if we're in the workspace
        return devPath.fsPath;
    }

    // Default to system PATH
    return 'forge-app-server';
}
