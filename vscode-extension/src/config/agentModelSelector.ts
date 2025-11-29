import * as vscode from 'vscode';
import { JsonRpcClient } from '../server/client';
import { EventEmitter } from 'events';
import { Agent, Model } from '../generated';

/**
 * Agent and Model selector
 */
export class AgentModelSelector extends EventEmitter {
    private statusBarAgent: vscode.StatusBarItem;
    private statusBarModel: vscode.StatusBarItem;
    private currentAgent = 'Forge';
    private currentModel = 'Claude 3.5 Sonnet';

    constructor(
        private rpcClient: JsonRpcClient,
        private outputChannel: vscode.OutputChannel
    ) {
        super();
        // Create status bar items
        this.statusBarAgent = vscode.window.createStatusBarItem(
            vscode.StatusBarAlignment.Left,
            100
        );
        this.statusBarAgent.command = 'forgecode.selectAgent';
        this.statusBarAgent.tooltip = 'Click to change agent';

        this.statusBarModel = vscode.window.createStatusBarItem(
            vscode.StatusBarAlignment.Left,
            99
        );
        this.statusBarModel.command = 'forgecode.selectModel';
        this.statusBarModel.tooltip = 'Click to change model';

        this.updateStatusBar();
        this.statusBarAgent.show();
        this.statusBarModel.show();
    }

    /**
     * Convert ID to UpperCamelCase for display
     * Examples: "forge" -> "Forge", "code-reviewer" -> "CodeReviewer"
     */
    private toUpperCamelCase(id: string): string {
        return id
            .split(/[-_]/)
            .map(word => word.charAt(0).toUpperCase() + word.slice(1).toLowerCase())
            .join('');
    }

    /**
     * Update status bar display
     */
    private updateStatusBar(): void {
        this.statusBarAgent.text = `$(person) ${this.currentAgent}`;
        this.statusBarModel.text = `$(circuit-board) ${this.currentModel}`;
    }

    /**
     * Select agent
     */
    async selectAgent(): Promise<void> {
        try {
            // Get available agents from server
            const response = await this.rpcClient.request<{ agents: Agent[] }>(
                'agent/list',
                {}
            );

            const agents = response.agents || [];

            // Show quick pick
            const items = agents.map(agent => ({
                label: agent.id,  // ID is the main label
                description: agent.title || undefined,  // Title as subtext
                detail: `${agent.description || ''}\nProvider: ${agent.provider}, Model: ${agent.model}`,
                agent: agent
            }));

            const selected = await vscode.window.showQuickPick(items, {
                placeHolder: 'Select an agent',
                matchOnDescription: true,
                matchOnDetail: true
            });

            if (selected) {
                // Set active agent on server
                await this.rpcClient.request('agent/set', {
                    agent_id: selected.agent.id
                });

                this.currentAgent = this.toUpperCamelCase(selected.agent.id);
                this.updateStatusBar();

                this.outputChannel.appendLine(`[AgentSelector] Selected: ${selected.agent.id}`);
                vscode.window.showInformationMessage(`Agent: ${this.toUpperCamelCase(selected.agent.id)}`);
                
                // Emit event for controller to refresh
                this.emit('agentChanged', selected.agent.id);
            }

        } catch (error) {
            this.outputChannel.appendLine(`[AgentSelector] Error: ${error}`);
            vscode.window.showErrorMessage(`Failed to select agent: ${error}`);
        }
    }

    /**
     * Select model
     */
    async selectModel(): Promise<void> {
        try {
            // Get available models from server
            const response = await this.rpcClient.request<{ models: Model[] }>(
                'model/list',
                {}
            );

            const models = response.models || [];

            // Show quick pick
            const items = models.map(model => ({
                label: model.id,  // ID is the main label
                description: model.name || undefined,  // Name as subtext
                detail: `${model.description || ''}\nContext: ${model.context_length || 'Unknown'}, Tools: ${model.tools_supported ? 'Yes' : 'No'}`,
                model: model
            }));

            const selected = await vscode.window.showQuickPick(items, {
                placeHolder: 'Select a model',
                matchOnDescription: true,
                matchOnDetail: true
            });

            if (selected) {
                // Set active model on server
                await this.rpcClient.request('model/set', {
                    model_id: selected.model.id
                });

                this.currentModel = this.toUpperCamelCase(selected.model.id);
                this.updateStatusBar();

                this.outputChannel.appendLine(`[ModelSelector] Selected: ${selected.model.id}`);
                vscode.window.showInformationMessage(`Model: ${this.toUpperCamelCase(selected.model.id)}`);
                
                // Emit event for controller to refresh
                this.emit('modelChanged', selected.model.id);
            }

        } catch (error) {
            this.outputChannel.appendLine(`[ModelSelector] Error: ${error}`);
            vscode.window.showErrorMessage(`Failed to select model: ${error}`);
        }
    }

    /**
     * Get current agent
     */
    getCurrentAgent(): string {
        return this.currentAgent;
    }

    /**
     * Get current model
     */
    getCurrentModel(): string {
        return this.currentModel;
    }

    /**
     * Dispose of resources
     */
    dispose(): void {
        this.statusBarAgent.dispose();
        this.statusBarModel.dispose();
    }
}
