import * as vscode from 'vscode';
import { randomUUID } from 'crypto';
import { JsonRpcClient } from './server/client';
import { ChatWebviewProvider } from './webview/provider';
import { ConversationTreeProvider } from './conversation/treeProvider';
import { FileContextManager } from './file/contextManager';
import {
    ConversationId,
    Agent,
    Model,
    EnvironmentInfo,
    ChatResponseContent,
    Usage,
    TokenCount,
    TitleFormat,
    ToolCallFull,
    ToolResult
} from './generated';

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
    private currentTurnId: string | null = null;
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
        
        // Send models and agents lists to webview
        await this.sendModelsList();
        await this.sendAgentsList();
        
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
     * Handle cancel from webview
     */
    public async handleCancel(): Promise<void> {
        this.outputChannel.appendLine('[Controller] Cancelling current turn');

        try {
            if (!this.currentThreadId || !this.currentTurnId) {
                this.outputChannel.appendLine('[Controller] No active turn to cancel');
                return;
            }

            await this.rpcClient.request('turn/cancel', {
                thread_id: this.currentThreadId,
                turn_id: this.currentTurnId
            });

            this.outputChannel.appendLine('[Controller] Turn cancelled');

        } catch (error) {
            this.outputChannel.appendLine(`[Controller] Error cancelling: ${error}`);
            vscode.window.showErrorMessage(`Failed to cancel: ${error}`);
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
            // Returns ConversationId (string)
            const conversationId = await this.rpcClient.request<ConversationId>(
                'thread/start',
                {}
            );

            this.outputChannel.appendLine(`[Controller] Thread started successfully`);
            this.outputChannel.appendLine(`[Controller] Thread ID: ${conversationId}`);
            
            return conversationId;
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
        this.currentTurnId = turnId;
        
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
            case 'chat/event':
                this.handleChatEvent(params);
                break;
            case 'turn/started':
                this.handleTurnStarted(params);
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
     * Handle chat/event notification (NEW - mirrors terminal UI pattern)
     * Reference: crates/forge_main/src/ui.rs:2349-2390
     */
    private handleChatEvent(params: unknown): void {
        if (!params || typeof params !== 'object') {
            this.outputChannel.appendLine('[Controller] Invalid chat/event params');
            return;
        }

        const eventData = params as { thread_id: string; turn_id: string; event: any };
        const { turn_id, event } = eventData;

        this.outputChannel.appendLine(`[Controller] Chat event for turn ${turn_id}: ${JSON.stringify(event)}`);

        // Pattern match on ChatResponse variants (TypeScript discriminated union)
        if (typeof event === 'object' && event !== null) {
            if ('TaskMessage' in event) {
                this.handleTaskMessage(event.TaskMessage.content);
            } else if ('TaskReasoning' in event) {
                this.handleTaskReasoning(event.TaskReasoning.content);
            } else if ('ToolCallStart' in event) {
                this.handleToolCallStart(event.ToolCallStart);
            } else if ('ToolCallEnd' in event) {
                this.handleToolCallEnd(event.ToolCallEnd);
            } else if ('Usage' in event) {
                this.handleUsageEvent(event.Usage);
            } else if ('RetryAttempt' in event) {
                this.handleRetryAttempt(event.RetryAttempt.cause, event.RetryAttempt.duration);
            } else if ('Interrupt' in event) {
                this.handleInterrupt(event.Interrupt.reason);
            }
        } else if (event === 'TaskComplete') {
            this.handleTaskComplete();
        }
    }

    /**
     * Handle TaskMessage event
     */
    private handleTaskMessage(content: ChatResponseContent): void {
        // Start streaming on first message
        if (!this.isStreamingStarted) {
            this.isStreamingStarted = true;
            this.webviewProvider.streamStart();
        }

        // Handle different content types
        if ('Title' in content) {
            this.handleTitle(content.Title);
        } else if ('PlainText' in content) {
            this.webviewProvider.postMessage({
                type: 'streamDelta',
                delta: content.PlainText
            });
        } else if ('Markdown' in content) {
            this.webviewProvider.postMessage({
                type: 'streamDelta',
                delta: content.Markdown
            });
        }
    }

    /**
     * Handle Title format
     */
    private handleTitle(titleFormat: TitleFormat): void {
        // Title can be PlainText or WithTimestamp
        if ('PlainText' in titleFormat) {
            const title = titleFormat.PlainText;
            this.outputChannel.appendLine(`[Controller] Title: ${title}`);
            // Could update conversation title here
        } else if ('WithTimestamp' in titleFormat) {
            // TypeScript discriminated union - WithTimestamp is an object with fields
            const data = (titleFormat as any).WithTimestamp;
            this.outputChannel.appendLine(`[Controller] Title: ${data.title} (${data.timestamp})`);
            // Could update conversation title here
        }
    }

    /**
     * Handle TaskReasoning event
     */
    private handleTaskReasoning(content: ChatResponseContent): void {
        if ('PlainText' in content) {
            this.webviewProvider.showReasoning(content.PlainText);
        } else if ('Markdown' in content) {
            this.webviewProvider.showReasoning(content.Markdown);
        }
    }

    /**
     * Handle ToolCallStart event
     */
    private handleToolCallStart(toolCall: ToolCallFull): void {
        // toolCall: { name: ToolName, call_id: ToolCallId | null, arguments: any }
        const toolName = toolCall.name;
        const callId = toolCall.call_id || 'unknown';
        
        this.outputChannel.appendLine(`[Controller] Tool call started: ${toolName} (${callId})`);
        
        // Show tool call in UI
        this.webviewProvider.postMessage({
            type: 'toolCallStart',
            tool: toolName,
            callId: callId,
            arguments: toolCall.arguments
        });
    }

    /**
     * Handle ToolCallEnd event
     */
    private handleToolCallEnd(result: ToolResult): void {
        // result: { name: ToolName, call_id: ToolCallId | null, output: ToolOutput }
        // ToolOutput: { is_error: boolean, values: Array<ToolValue> }
        const toolName = result.name;
        const callId = result.call_id || 'unknown';
        const isError = result.output.is_error;
        
        // Extract text from ToolValue array
        let outputText = '';
        if (result.output.values && result.output.values.length > 0) {
            for (const value of result.output.values) {
                if (typeof value === 'object' && value !== null) {
                    if ('text' in value) {
                        outputText += value.text;
                    } else if ('image' in value) {
                        outputText += `[Image: ${value.image.mime_type}]`;
                    }
                }
                // Skip "empty" variant
            }
        }
        
        this.outputChannel.appendLine(`[Controller] Tool call ended: ${toolName} (${callId}) - ${isError ? 'ERROR' : 'SUCCESS'}`);
        
        // Show tool result in UI
        this.webviewProvider.postMessage({
            type: 'toolCallEnd',
            tool: toolName,
            callId: callId,
            output: outputText,
            isError: isError
        });
    }

    /**
     * Handle TaskComplete event
     */
    private handleTaskComplete(): void {
        this.outputChannel.appendLine('[Controller] Task complete');
        
        // Reset streaming state
        this.isStreamingStarted = false;
        
        // End streaming - this finalizes the message in the UI
        this.webviewProvider.streamEnd();
    }

    /**
     * Handle Usage event (from chat/event)
     */
    private handleUsageEvent(usage: Usage): void {
        // Format token counts with Actual vs Approx
        // Note: Usage uses snake_case fields (prompt_tokens, completion_tokens, etc.)
        const inputTokens = this.formatTokenCount(usage.prompt_tokens);
        const outputTokens = this.formatTokenCount(usage.completion_tokens);
        
        let totalUsed = parseInt(inputTokens) + parseInt(outputTokens);
        
        // Use total_tokens if available
        if (usage.total_tokens) {
            const total = this.formatTokenCount(usage.total_tokens);
            totalUsed = parseInt(total);
        }
        
        this.tokens.used = totalUsed;
        
        // Update header with usage info
        let tokensDisplay = `${inputTokens} in / ${outputTokens} out`;
        
        if (usage.cached_tokens) {
            const cached = this.formatTokenCount(usage.cached_tokens);
            tokensDisplay += ` (${cached} cached)`;
        }
        
        this.webviewProvider.updateHeader({
            tokens: tokensDisplay
        });
        
        this.outputChannel.appendLine(`[Controller] Usage: ${tokensDisplay}`);
    }

    /**
     * Format TokenCount enum (Actual vs Approx)
     */
    private formatTokenCount(tokenCount: TokenCount): string {
        if ('Actual' in tokenCount) {
            return tokenCount.Actual.toString();
        } else if ('Approx' in tokenCount) {
            return `~${tokenCount.Approx}`;
        }
        return '0';
    }

    /**
     * Handle RetryAttempt event
     */
    private handleRetryAttempt(cause: any, duration: number): void {
        this.outputChannel.appendLine(`[Controller] Retry attempt: ${JSON.stringify(cause)}, duration: ${duration}ms`);
        // Could show retry notification in UI
    }

    /**
     * Handle Interrupt event
     */
    private handleInterrupt(reason: any): void {
        this.outputChannel.appendLine(`[Controller] Interrupted: ${JSON.stringify(reason)}`);
        // Could show interruption notification in UI
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
     * Handle turn started notification
     */
    private handleTurnStarted(params: any): void {
        this.outputChannel.appendLine(`[Controller] Turn started: ${JSON.stringify(params)}`);
        
        // Forward to webview so it can track the turn ID for cancellation
        this.webviewProvider?.postMessage({
            type: 'turn/started',
            threadId: params.thread_id,
            turnId: params.turn_id,
        });
    }

    /**
     * Handle turn completed notification
     */
    private handleTurnCompleted(params: any): void {
        this.outputChannel.appendLine(`[Controller] Turn completed with status: ${params.status}`);
        
        // Reset streaming state
        this.isStreamingStarted = false;
        
        // Clear current turn
        this.currentTurnId = null;
        
        // Forward to webview
        this.webviewProvider.postMessage({
            type: 'turn/completed',
            threadId: params.thread_id,
            turnId: params.turn_id,
            status: params.status,
        });
        
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

    // ========================================================================
    // REQUEST HANDLERS (Updated to use domain types with snake_case fields)
    // ========================================================================

    /**
     * Send models list to webview
     */
    public async sendModelsList(): Promise<void> {
        try {
            this.outputChannel.appendLine('[Controller] Fetching models list');
            
            // Returns Model[] directly
            const models = await this.rpcClient.request<Model[]>(
                'model/list',
                undefined
            );
            
            const modelsForWebview = models.map(model => ({
                id: model.id,
                name: model.name || undefined,
                description: model.description || undefined,
                context_length: model.context_length ? Number(model.context_length) : undefined,
                tools_supported: model.tools_supported || undefined,
                supports_parallel_tool_calls: model.supports_parallel_tool_calls || undefined,
                supports_reasoning: model.supports_reasoning || undefined,
            }));
            
            this.outputChannel.appendLine(`[Controller] Sending ${models.length} models to webview`);
            this.webviewProvider.sendModelsList(modelsForWebview);
            
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
     * Send agents list to webview
     */
    public async sendAgentsList(): Promise<void> {
        try {
            this.outputChannel.appendLine('[Controller] Fetching agents list');
            
            // Returns Agent[] directly
            const agents = await this.rpcClient.request<Agent[]>(
                'agent/list',
                undefined
            );
            
            const agentsForWebview = agents.map(agent => ({
                id: agent.id,
                name: agent.title || agent.id, // Agent uses 'title' not 'name'
                description: agent.description,
                provider: agent.provider, // Agent has provider field
                model: agent.model, // Agent has model field
            }));
            
            this.outputChannel.appendLine(`[Controller] Sending ${agents.length} agents to webview`);
            this.webviewProvider.sendAgentsList(agentsForWebview);
            
        } catch (error) {
            this.outputChannel.appendLine(`[Controller] Failed to fetch agents list: ${error}`);
        }
    }

    /**
     * Handle agent change from webview
     */
    public async handleAgentChange(agentId: string): Promise<void> {
        try {
            this.outputChannel.appendLine(`[Controller] Changing agent to: ${agentId}`);
            
            // Set active agent on server
            await this.rpcClient.request('agent/set', {
                agent_id: agentId
            });
            
            // Refresh agent and model to update display
            await this.refreshAgentAndModel();
            
            this.outputChannel.appendLine(`[Controller] Agent changed successfully`);
            
        } catch (error) {
            this.outputChannel.appendLine(`[Controller] Failed to change agent: ${error}`);
            vscode.window.showErrorMessage(`Failed to change agent: ${error}`);
        }
    }

    /**
     * Refresh agent and model from server
     */
    public async refreshAgentAndModel(): Promise<void> {
        try {
            this.outputChannel.appendLine('[Controller] Fetching current agent and model from server');
            
            // Returns EnvironmentInfo directly (snake_case fields!)
            const envInfo = await this.rpcClient.request<EnvironmentInfo>(
                'env/info',
                undefined
            );
            
            this.outputChannel.appendLine(`[Controller] Environment info: ${JSON.stringify(envInfo)}`);
            
            // Fetch agent list to get display names
            const agents = await this.rpcClient.request<Agent[]>(
                'agent/list',
                undefined
            );
            
            // Fetch model list to get display names  
            const models = await this.rpcClient.request<Model[]>(
                'model/list',
                undefined
            );
            
            // Store agent and model IDs
            // Note: EnvironmentInfo uses camelCase fields (activeAgent, defaultModel)
            let agentId = envInfo.activeAgent || '';
            let modelId = envInfo.defaultModel || '';
            
            // Update agent if available
            if (envInfo.activeAgent) {
                // Find agent display name
                const agent = agents.find(a => a.id === envInfo.activeAgent);
                this.agent = agent?.title || envInfo.activeAgent;
                this.outputChannel.appendLine(`[Controller] Updated agent: ${this.agent}`);
            }
            
            // Update model if available
            if (envInfo.defaultModel) {
                // Find model display name
                const model = models.find(m => m.id === envInfo.defaultModel);
                this.model = model?.name || model?.id || envInfo.defaultModel;
                this.outputChannel.appendLine(`[Controller] Updated model: ${this.model}`);
            }
            
            // Update header in webview with both names and IDs
            this.webviewProvider.updateHeader({
                agent: this.agent,
                agent_id: agentId,
                model: this.model,
                model_id: modelId
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
