import * as vscode from 'vscode';
import { JsonRpcClient } from '../server/client';

/**
 * Provider configuration manager
 */
export class ProviderManager {
    constructor(
        private rpcClient: JsonRpcClient,
        private context: vscode.ExtensionContext,
        private outputChannel: vscode.OutputChannel
    ) {}

    /**
     * Select provider
     */
    async selectProvider(): Promise<void> {
        try {
            // Get available providers from server
            const response = await this.rpcClient.request<{ providers: Provider[] }>(
                'provider/list',
                {}
            );

            const providers = response.providers || [];

            // Show quick pick
            const items = providers.map(provider => ({
                label: provider.id,
                description: provider.configured ? '$(check) Configured' : '$(warning) Not configured',
                detail: provider.authMethod,
                provider: provider
            }));

            const selected = await vscode.window.showQuickPick(items, {
                placeHolder: 'Select a provider',
                matchOnDescription: true
            });

            if (selected) {
                const provider = selected.provider;

                // If not configured, offer to configure
                if (!provider.configured) {
                    const configure = await vscode.window.showInformationMessage(
                        `Provider "${provider.id}" is not configured. Configure now?`,
                        'Configure',
                        'Cancel'
                    );

                    if (configure === 'Configure') {
                        await this.configureProvider(provider);
                    }
                    return;
                }

                // Set active provider on server
                await this.rpcClient.request('provider/set', {
                    providerId: provider.id
                });

                this.outputChannel.appendLine(`[ProviderManager] Selected: ${provider.id}`);
                vscode.window.showInformationMessage(`Provider: ${provider.id}`);
            }

        } catch (error) {
            this.outputChannel.appendLine(`[ProviderManager] Error: ${error}`);
            vscode.window.showErrorMessage(`Failed to select provider: ${error}`);
        }
    }

    /**
     * Configure provider
     */
    async configureProvider(provider: Provider): Promise<void> {
        this.outputChannel.appendLine(`[ProviderManager] Configuring: ${provider.id}`);

        try {
            if (provider.authMethod === 'api_key') {
                await this.configureApiKey(provider);
            } else if (provider.authMethod === 'oauth') {
                await this.configureOAuth(provider);
            } else {
                vscode.window.showWarningMessage(`Unknown auth method: ${provider.authMethod}`);
            }

        } catch (error) {
            this.outputChannel.appendLine(`[ProviderManager] Configuration error: ${error}`);
            vscode.window.showErrorMessage(`Failed to configure provider: ${error}`);
        }
    }

    /**
     * Configure API key authentication
     */
    private async configureApiKey(provider: Provider): Promise<void> {
        const apiKey = await vscode.window.showInputBox({
            prompt: `Enter API key for ${provider.id}`,
            password: true,
            placeHolder: 'sk-...',
            validateInput: (value) => {
                if (!value || value.trim().length === 0) {
                    return 'API key is required';
                }
                return null;
            }
        });

        if (apiKey) {
            // Store in secure storage
            await this.context.secrets.store(`provider.${provider.id}.apiKey`, apiKey);

            this.outputChannel.appendLine(`[ProviderManager] API key stored for: ${provider.id}`);
            vscode.window.showInformationMessage(`Configured ${provider.id}`);

            // Notify server (if needed)
            // The server should read from environment or we need to send it securely
        }
    }

    /**
     * Configure OAuth authentication
     */
    private async configureOAuth(provider: Provider): Promise<void> {
        vscode.window.showInformationMessage(
            `Opening browser for ${provider.id} authentication...`,
            'OK'
        );

        // TODO: Implement OAuth flow
        // 1. Get authorization URL from server
        // 2. Open browser
        // 3. Poll for completion or use callback
        // 4. Store tokens securely

        this.outputChannel.appendLine(`[ProviderManager] OAuth not yet implemented for: ${provider.id}`);
        vscode.window.showWarningMessage('OAuth authentication coming soon!');
    }

    /**
     * List configured providers
     */
    async listProviders(): Promise<void> {
        try {
            const response = await this.rpcClient.request<{ providers: Provider[] }>(
                'provider/list',
                {}
            );

            const providers = response.providers || [];

            if (providers.length === 0) {
                vscode.window.showInformationMessage('No providers available');
                return;
            }

            const output = providers
                .map(p => `${p.configured ? '✓' : '✗'} ${p.id} (${p.authMethod})`)
                .join('\n');

            const document = await vscode.workspace.openTextDocument({
                content: `# Configured Providers\n\n${output}`,
                language: 'markdown'
            });

            await vscode.window.showTextDocument(document, { preview: true });

        } catch (error) {
            this.outputChannel.appendLine(`[ProviderManager] Error listing: ${error}`);
            vscode.window.showErrorMessage(`Failed to list providers: ${error}`);
        }
    }

    /**
     * Remove provider configuration
     */
    async removeProvider(providerId: string): Promise<void> {
        const confirm = await vscode.window.showWarningMessage(
            `Remove configuration for ${providerId}?`,
            { modal: true },
            'Remove'
        );

        if (confirm === 'Remove') {
            try {
                // Remove from secure storage
                await this.context.secrets.delete(`provider.${providerId}.apiKey`);

                this.outputChannel.appendLine(`[ProviderManager] Removed: ${providerId}`);
                vscode.window.showInformationMessage(`Removed ${providerId} configuration`);

            } catch (error) {
                this.outputChannel.appendLine(`[ProviderManager] Remove error: ${error}`);
                vscode.window.showErrorMessage(`Failed to remove provider: ${error}`);
            }
        }
    }

    /**
     * Get stored API key for provider
     */
    async getApiKey(providerId: string): Promise<string | undefined> {
        return await this.context.secrets.get(`provider.${providerId}.apiKey`);
    }
}

interface Provider {
    id: string;
    authMethod: string;
    configured: boolean;
}
