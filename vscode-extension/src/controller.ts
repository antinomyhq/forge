import * as vscode from 'vscode';
import { randomUUID } from 'crypto';
import { JsonRpcClient } from './server/client';
import { ChatWebviewProvider } from './webview/provider';
import { ConversationTreeProvider, ConversationItem } from './conversation/treeProvider';
import { FileContextManager } from './file/contextManager';

/**
 * Controller orchestrates communication between:
 * - VSCode Extension
 * - JSON-RPC Client (server communication)
 * - Webview Provider (UI)
 */
export class Controller {
    private rpcClient: JsonRpcClient;
    private webviewProvider: ChatWebviewProvider;
    private conversationTree: ConversationTreeProvider;
    private fileContext: FileContextManager;
    private outputChannel: vscode.OutputChannel;
    
    // State
    private currentThreadId: string | null = null;
    private messages: Message[] = [];
    private agent = 'Forge';
    private model = 'Claude 3.5 Sonnet';
    private tokens = { used: 0, total: 200000 };
    private cost = 0;
    private isStreamingStarted = false;

    constructor(
        rpcClient: JsonRpcClient,
        webviewProvider: ChatWebviewProvider,
        conversationTree: ConversationTreeProvider,
        fileContext: FileContextManager,
        outputChannel: vscode.OutputChannel
    ) {
        this.rpcClient = rpcClient;
        this.webviewProvider = webviewProvider;
        this.conversationTree = conversationTree;
        this.fileContext = fileContext;
        this.outputChannel = outputChannel;

        this.setupEventHandlers();
    }

    /**
     * Set up event handlers
     */
    private setupEventHandlers(): void {
        // Note: RPC notification handling is done by extension.ts which forwards to handleServerNotification()
        // We don't attach a listener here to avoid duplicate processing
        
        // Listen to webview events (handled through provider for now)
        // In production, would use proper event emitter pattern
    }

    /**
     * Handle webview ready event
     */
    public async handleWebviewReady(): Promise<void> {
        this.outputChannel.appendLine('[Controller] Webview ready, sending initial state');
        
        // Fetch current agent and model from server
        await this.refreshAgentAndModel();
        
        // Send models list to webview
        await this.sendModelsList();
        
        // Send current state to webview
        this.webviewProvider.updateState({
            messages: this.messages,
            agent: this.agent,
            model: this.model,
            tokens: `${this.tokens.used} / ${this.tokens.total} tokens`,
            cost: `$${this.cost.toFixed(2)}`
        });
    }

    /**
     * Handle send message from webview
     */
    public async handleSendMessage(text: string): Promise<void> {
        this.outputChannel.appendLine(`[Controller] Sending message: ${text}`);

        try {
            // Start thread if not exists
            if (!this.currentThreadId) {
                this.currentThreadId = await this.startThread();
            }

            // Start turn
            await this.startTurn(text);

            // Server will send notifications as it processes

        } catch (error) {
            this.outputChannel.appendLine(`[Controller] Error: ${error}`);
            vscode.window.showErrorMessage(`Failed to send message: ${error}`);
            
            // Re-enable input on error
            this.webviewProvider.streamEnd();
        }
    }

    /**
     * Handle approval response from webview
     */
    public async handleApproval(id: string, decision: 'accept' | 'reject'): Promise<void> {
        this.outputChannel.appendLine(`[Controller] Approval ${decision} for ${id}`);

        try {
            // Send approval to server
            await this.rpcClient.request('approval/fileChange', {
                decision: decision
            });
        } catch (error) {
            this.outputChannel.appendLine(`[Controller] Approval error: ${error}`);
        }
    }

    /**
     * Start a new thread
     */
    private async startThread(): Promise<string> {
        this.outputChannel.appendLine('[Controller] Starting new thread');
        
        try {
            const response = await this.rpcClient.request<{ threadId: string }>(
                'thread/start',
                {}
            );

            this.outputChannel.appendLine(`[Controller] Thread started successfully`);
            this.outputChannel.appendLine(`[Controller] Thread ID: ${response.threadId}`);
            this.outputChannel.appendLine(`[Controller] Full response: ${JSON.stringify(response)}`);
            
            return response.threadId;
        } catch (error) {
            this.outputChannel.appendLine(`[Controller] Thread start failed: ${error}`);
            throw error;
        }
    }

    /**
     * Start a new turn
     */
    private async startTurn(message: string): Promise<string> {
        this.outputChannel.appendLine('[Controller] Starting new turn');
        
        // Generate a unique turn ID
        const turnId = randomUUID();
        
        const response = await this.rpcClient.request<{ turnId: string }>(
            'turn/start',
            {
                thread_id: this.currentThreadId,
                turn_id: turnId,
                message: message,
                files: []
            }
        );

        this.outputChannel.appendLine(`[Controller] Turn started: ${response.turnId}`);
        return response.turnId;
    }

    /**
     * Handle server notifications
     */
    public handleServerNotification(method: string, params: unknown): void {
        this.outputChannel.appendLine(`[Controller] Notification: ${method}`);

        switch (method) {
            case 'thread/started':
                this.handleThreadStarted(params);
                break;
            case 'turn/started':
                this.handleTurnStarted(params);
                break;
            case 'item/started':
                this.handleItemStarted(params);
                break;
            case 'item/completed':
                this.handleItemCompleted(params);
                break;
            case 'item/agentMessage/delta':
                this.handleAgentMessageDelta(params);
                break;
            case 'item/agentReasoning/delta':
                this.handleReasoningDelta(params);
                break;
            case 'tool/started':
                this.handleToolStarted(params);
                break;
            case 'tool/completed':
                this.handleToolCompleted(params);
                break;
            case 'turn/usage':
                this.handleUsage(params);
                break;
            case 'turn/completed':
                this.handleTurnCompleted(params);
                break;
            case 'error':
                this.handleError(params);
                break;
        }
    }

    /**
     * Start new conversation
     */
    public async startNewConversation(): Promise<void> {
        this.outputChannel.appendLine('[Controller] Starting new conversation');
        
        // Clear current state
        this.currentThreadId = null;
        this.messages = [];
        this.tokens = { used: 0, total: 200000 };
        this.cost = 0;
        
        // Update UI
        this.webviewProvider.updateState({
            messages: [],
            agent: this.agent,
            model: this.model,
            tokens: `${this.tokens.used} / ${this.tokens.total} tokens`,
            cost: `$${this.cost.toFixed(2)}`
        });
    }

    /**
     * Open existing conversation
     */
    public async openConversation(threadId: string): Promise<void> {
        this.outputChannel.appendLine(`[Controller] Opening conversation: ${threadId}`);
        
        // Set active conversation
        this.currentThreadId = threadId;
        this.conversationTree.setActiveConversation(threadId);
        
        // TODO: Load conversation history from server
        // For now, just clear and start fresh
        this.messages = [];
        this.webviewProvider.updateState({
            messages: [],
            agent: this.agent,
            model: this.model,
            tokens: `0 / 200000 tokens`,
            cost: `$0.00`
        });
    }

    /**
     * Delete conversation
     */
    public async deleteConversation(threadId: string): Promise<void> {
        this.outputChannel.appendLine(`[Controller] Deleting conversation: ${threadId}`);
        
        // Remove from tree
        this.conversationTree.deleteConversation(threadId);
        
        // If it's the active conversation, clear it
        if (this.currentThreadId === threadId) {
            this.currentThreadId = null;
        }
    }

    /**
     * Get tagged files context
     */
    public async getFileContext(): Promise<string> {
        const taggedFiles = await this.fileContext.getTaggedFileContents();
        
        if (taggedFiles.length === 0) {
            return '';
        }
        
        // Format as context
        const context = taggedFiles
            .map(file => `@[${file.path}]\n\`\`\`\n${file.content}\n\`\`\``)
            .join('\n\n');
        
        return context;
    }

        /**
     * Handle thread started notification
     */
    private handleThreadStarted(params: any): void {
        if (params.threadId) {
            this.currentThreadId = params.threadId;
            
            // Add to conversation tree
            const conversation: ConversationItem = {
                id: params.threadId,
                title: `Conversation ${new Date().toLocaleTimeString()}`,
                messageCount: 0,
                timestamp: Date.now()
            };
            this.conversationTree.addConversation(conversation);
            this.conversationTree.setActiveConversation(params.threadId);
        }
    }

    /**
     * Handle turn started notification
     */
    private handleTurnStarted(_params: any): void {
        // Turn started - UI will be prepared when item starts
    }

    /**
     * Handle item started notification
     */
    private handleItemStarted(params: any): void {
        this.outputChannel.appendLine(`[Controller] Item started: ${JSON.stringify(params)}`);
        
        // Forward to webview for display (tool calls, reasoning, etc.)
        this.webviewProvider?.postMessage({
            type: 'ItemStarted',
            itemId: params.item_id,
            itemType: params.item_type,
        });
        
        // Don't start streaming here - wait for first delta
        // Reasoning and message deltas share the same ItemStarted (AgentMessage)
        // So we can't distinguish them here
    }

    /**
     * Handle item completed notification
     */
    private handleItemCompleted(params: any): void {
        this.outputChannel.appendLine(`[Controller] Item completed: ${JSON.stringify(params)}`);
        
        // Forward to webview for display
        this.webviewProvider?.postMessage({
            type: 'ItemCompleted',
            itemId: params.item_id,
        });
    }

    /**
     * Handle agent message delta (streaming)
     */
    private handleAgentMessageDelta(params: any): void {
        this.outputChannel.appendLine(`[Controller] Agent message delta: ${JSON.stringify(params)}`);
        
        const delta = params?.delta || '';
        const itemId = params?.item_id;

        // Start streaming on first delta
        if (!this.isStreamingStarted) {
            this.isStreamingStarted = true;
            this.webviewProvider.streamStart();
        }

        // Pass both delta and item_id to webview so it can filter tool-related deltas
        this.webviewProvider.postMessage({
            type: 'streamDelta',
            delta: delta,
            itemId: itemId
        });
    }

    /**
     * Handle reasoning delta
     */
    private handleReasoningDelta(params: any): void {
        const text = params.delta || '';
        this.webviewProvider.showReasoning(text);
    }

    /**
     * Handle tool started notification
     */
    private handleToolStarted(params: any): void {
        this.webviewProvider.showTool({
            name: params.name || 'Tool',
            type: params.type || 'unknown',
            status: 'Running'
        });
    }

    /**
     * Handle tool completed notification
     */
    private handleToolCompleted(params: any): void {
        this.webviewProvider.showTool({
            name: params.name || 'Tool',
            type: params.type || 'unknown',
            status: 'Completed',
            result: params.result || ''
        });
    }

    /**
     * Handle usage notification
     */
    private handleUsage(params: any): void {
        if (params.input_tokens !== undefined && params.output_tokens !== undefined) {
            this.tokens.used = params.input_tokens + params.output_tokens;
            this.webviewProvider.updateHeader({
                tokens: `${this.tokens.used} / ${this.tokens.total} tokens`
            });
        }

        if (params.total_cost !== undefined) {
            this.cost = params.total_cost;
            this.webviewProvider.updateHeader({
                cost: `$${this.cost.toFixed(2)}`
            });
        }
    }

    /**
     * Handle turn completed notification
     */
    private handleTurnCompleted(_params: any): void {
        this.outputChannel.appendLine('[Controller] Turn completed');
        
        // Reset streaming state
        this.isStreamingStarted = false;
        
        // End streaming
        this.webviewProvider.streamEnd();
    }

    /**
     * Handle error notification
     */
    private handleError(params: any): void {
        const message = params.message || 'Unknown error';
        this.outputChannel.appendLine(`[Controller] Error: ${message}`);
        
        vscode.window.showErrorMessage(`ForgeCode error: ${message}`);
        
        // Reset streaming state
        this.isStreamingStarted = false;
        
        // End streaming on error
        this.webviewProvider.streamEnd();
    }

    /**
     * Send models list to webview
     */
    public async sendModelsList(): Promise<void> {
        try {
            this.outputChannel.appendLine('[Controller] Fetching models list');
            
            const response = await this.rpcClient.request<{ models: ModelInfo[] }>(
                'model/list',
                undefined
            );
            
            const models = response.models.map(model => ({
                id: model.id,
                name: model.name || model.id,
                provider: model.provider || 'Unknown',
                contextWindow: model.contextLength || 0
            }));
            
            this.outputChannel.appendLine(`[Controller] Sending ${models.length} models to webview`);
            this.webviewProvider.sendModelsList(models);
            
        } catch (error) {
            this.outputChannel.appendLine(`[Controller] Failed to fetch models list: ${error}`);
        }
    }

    /**
     * Handle model change from webview
     */
    public async handleModelChange(modelId: string): Promise<void> {
        try {
            this.outputChannel.appendLine(`[Controller] Changing model to: ${modelId}`);
            
            // Set active model on server
            await this.rpcClient.request('model/set', {
                model_id: modelId
            });
            
            // Refresh agent and model to update display
            await this.refreshAgentAndModel();
            
            this.outputChannel.appendLine(`[Controller] Model changed successfully`);
            
        } catch (error) {
            this.outputChannel.appendLine(`[Controller] Failed to change model: ${error}`);
            vscode.window.showErrorMessage(`Failed to change model: ${error}`);
        }
    }

    /**
     * Refresh agent and model from server
     */
    public async refreshAgentAndModel(): Promise<void> {
        try {
            this.outputChannel.appendLine('[Controller] Fetching current agent and model from server');
            
            const response = await this.rpcClient.request<EnvInfoResponse>(
                'env/info',
                undefined
            );
            
            this.outputChannel.appendLine(`[Controller] Environment info: ${JSON.stringify(response)}`);
            
            // Fetch agent list to get display names
            const agentListResponse = await this.rpcClient.request<{ agents: AgentInfo[] }>(
                'agent/list',
                undefined
            );
            
            // Fetch model list to get display names  
            const modelListResponse = await this.rpcClient.request<{ models: ModelInfo[] }>(
                'model/list',
                undefined
            );
            
            // Update agent if available
            if (response.activeAgent) {
                // Find agent display name
                const agent = agentListResponse.agents.find(a => a.id === response.activeAgent);
                this.agent = agent?.name || response.activeAgent;
                this.outputChannel.appendLine(`[Controller] Updated agent: ${this.agent}`);
            }
            
            // Update model if available
            if (response.defaultModel) {
                // Find model display name
                const model = modelListResponse.models.find(m => m.id === response.defaultModel);
                this.model = model?.name || model?.id || response.defaultModel;
                this.outputChannel.appendLine(`[Controller] Updated model: ${this.model}`);
            }
            
            // Update header in webview
            this.webviewProvider.updateHeader({
                agent: this.agent,
                model: this.model
            });
            
        } catch (error) {
            this.outputChannel.appendLine(`[Controller] Failed to fetch environment info: ${error}`);
            // Continue with default values on error
        }
    }

    /**
     * Dispose of resources
     */
    public dispose(): void {
        // Clean up resources
    }
}

interface Message {
    role: 'user' | 'assistant';
    content: string;
    timestamp: number;
}


interface EnvInfoResponse {
    cwd: string;
    os: string;
    shell: string;
    home: string;
    activeAgent?: string;
    defaultModel?: string;
}

interface AgentInfo {
    id: string;
    name: string;
    description?: string;
    provider?: string;
    model?: string;
}

interface ModelInfo {
    id: string;
    name?: string;
    provider?: string;
    contextLength?: number;
}
