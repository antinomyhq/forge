import * as vscode from 'vscode';
import * as fs from 'fs';

/**
 * Webview provider for the React-based chat interface
 * Manages webview lifecycle, CSP, and messaging
 */
export class ChatWebviewProvider implements vscode.WebviewViewProvider {
    public static readonly viewType = 'forgecode.chatView';

    private view?: vscode.WebviewView;
    private extensionUri: vscode.Uri;
    private outputChannel: vscode.OutputChannel;
    private onReadyCallback?: () => void;
    private onSendMessageCallback?: (text: string) => void;
    private onApprovalCallback?: (data: any) => void;
    private onModelChangeCallback?: (modelId: string) => void;
    private onAgentChangeCallback?: (agentId: string) => void;
    private onCancelCallback?: () => void;

    constructor(extensionUri: vscode.Uri, outputChannel: vscode.OutputChannel) {
        this.extensionUri = extensionUri;
        this.outputChannel = outputChannel;
    }

    /**
     * Set callback for ready event
     */
    public onReady(callback: () => void): void {
        this.onReadyCallback = callback;
    }

    /**
     * Set callback for sendMessage event
     */
    public onSendMessage(callback: (text: string) => void): void {
        this.onSendMessageCallback = callback;
    }

    /**
     * Set callback for approval event
     */
    public onApproval(callback: (data: any) => void): void {
        this.onApprovalCallback = callback;
    }

    /**
     * Set callback for model change event
     */
    public onModelChange(callback: (modelId: string) => void): void {
        this.onModelChangeCallback = callback;
    }

    /**
     * Set callback for agent change event
     */
    public onAgentChange(callback: (agentId: string) => void): void {
        this.onAgentChangeCallback = callback;
    }

    /**
     * Set callback for cancel event
     */
    public onCancel(callback: () => void): void {
        this.onCancelCallback = callback;
    }

    /**
     * Resolve the webview view
     */
    public resolveWebviewView(
        webviewView: vscode.WebviewView,
        _context: vscode.WebviewViewResolveContext,
        _token: vscode.CancellationToken
    ): void | Thenable<void> {
        this.view = webviewView;

        // Configure webview options
        webviewView.webview.options = {
            enableScripts: true,
            localResourceRoots: [
                vscode.Uri.joinPath(this.extensionUri, 'webview-ui', 'dist')
            ]
        };

        // Set HTML content from React build
        webviewView.webview.html = this.getHtmlContent(webviewView.webview);

        // Handle messages from webview
        webviewView.webview.onDidReceiveMessage(
            message => this.handleWebviewMessage(message),
            undefined,
            []
        );

        // Preserve state across hide/show
        webviewView.onDidChangeVisibility(() => {
            if (webviewView.visible) {
                this.outputChannel.appendLine('Chat view became visible');
            }
        });
    }

    /**
     * Post message to webview
     */
    public postMessage(message: unknown): void {
        if (this.view) {
            this.view.webview.postMessage(message);
            this.outputChannel.appendLine(`[WebviewProvider] Posted message: ${JSON.stringify(message).substring(0, 100)}`);
        } else {
            this.outputChannel.appendLine('[WebviewProvider] ERROR: View is null, message not sent!');
        }
    }

    /**
     * Update state in webview
     */
    public updateState(state: any): void {
        this.postMessage({
            jsonrpc: '2.0',
            method: 'state/update',
            params: state
        });
    }

    /**
     * Show streaming message
     */
    public streamStart(): void {
        this.postMessage({ 
            jsonrpc: '2.0',
            method: 'stream/start',
            params: {}
        });
    }

    /**
     * Add delta to streaming message
     */
    public streamDelta(delta: string): void {
        this.postMessage({ 
            jsonrpc: '2.0',
            method: 'stream/delta',
            params: { delta }
        });
    }

    /**
     * End streaming
     */
    public streamEnd(): void {
        this.postMessage({ 
            jsonrpc: '2.0',
            method: 'stream/end',
            params: {}
        });
    }

    /**
     * Show tool execution
     */
    public showTool(tool: unknown): void {
        this.postMessage({ 
            jsonrpc: '2.0',
            method: 'tool/show',
            params: { tool }
        });
    }

    /**
     * Show reasoning
     */
    public showReasoning(text: string): void {
        this.postMessage({ 
            jsonrpc: '2.0',
            method: 'reasoning/show',
            params: { text }
        });
    }

    /**
     * Request approval
     */
    public requestApproval(approval: unknown): void {
        this.postMessage({ 
            jsonrpc: '2.0',
            method: 'approval/request',
            params: { approval }
        });
    }

    /**
     * Update header info
     */
    public updateHeader(data: unknown): void {
        this.postMessage({ 
            jsonrpc: '2.0',
            method: 'header/update',
            params: data
        });
    }

    /**
     * Send models list to webview
     */
    public sendModelsList(models: unknown[]): void {
        this.postMessage({ 
            jsonrpc: '2.0',
            method: 'models/list',
            params: { models }
        });
    }

    /**
     * Send agents list to webview
     */
    public sendAgentsList(agents: unknown[]): void {
        this.postMessage({ 
            jsonrpc: '2.0',
            method: 'agents/list',
            params: { agents }
        });
    }

    /**
     * Handle messages from webview (JSON-RPC requests)
     */
    private handleWebviewMessage(message: any): void {
        this.outputChannel.appendLine(`[Webview] Received message: ${JSON.stringify(message).substring(0, 200)}`);

        // Support both legacy (type) and JSON-RPC (method) formats
        const method = message.method || message.type;
        const params = message.params || message;

        // Handle messages based on method
        switch (method) {
            case 'webview/ready':
            case 'ready':
                this.outputChannel.appendLine('[Webview] Calling onReady callback');
                if (this.onReadyCallback) {
                    this.onReadyCallback();
                }
                break;
            case 'chat/sendMessage':
            case 'sendMessage':
                this.outputChannel.appendLine(`[Webview] Calling onSendMessage callback with: ${params.text || message.text}`);
                if (this.onSendMessageCallback) {
                    this.onSendMessageCallback(params.text || message.text);
                }
                break;
            case 'turn/cancel':
            case 'cancel':
                this.outputChannel.appendLine('[Webview] Calling onCancel callback');
                if (this.onCancelCallback) {
                    this.onCancelCallback();
                }
                break;
            case 'approval':
                this.outputChannel.appendLine('[Webview] Calling onApproval callback');
                if (this.onApprovalCallback) {
                    this.onApprovalCallback({
                        id: params.id || message.id,
                        decision: params.decision || message.decision
                    });
                }
                break;
            case 'model/change':
            case 'modelChange':
                this.outputChannel.appendLine(`[Webview] Model change requested: ${params.modelId || message.modelId}`);
                if (this.onModelChangeCallback) {
                    this.onModelChangeCallback(params.modelId || message.modelId);
                }
                break;
            case 'agent/change':
            case 'agentChange':
                this.outputChannel.appendLine(`[Webview] Agent change requested: ${params.agentId || message.agentId}`);
                if (this.onAgentChangeCallback) {
                    this.onAgentChangeCallback(params.agentId || message.agentId);
                }
                break;
            case 'models/request':
            case 'requestModels':
                this.outputChannel.appendLine('[Webview] Requesting models list');
                // This will be handled by controller
                break;
            case 'agents/request':
            case 'requestAgents':
                this.outputChannel.appendLine('[Webview] Requesting agents list');
                // This will be handled by controller
                break;
        }
    }

    /**
     * Get HTML content for webview from React build
     */
    private getHtmlContent(webview: vscode.Webview): string {
        // Path to React build
        const distPath = vscode.Uri.joinPath(this.extensionUri, 'webview-ui', 'dist');
        const indexPath = vscode.Uri.joinPath(distPath, 'index.html');

        // Read the built index.html
        let html = fs.readFileSync(indexPath.fsPath, 'utf8');

        // Replace asset paths with webview URIs
        const assetsPath = vscode.Uri.joinPath(distPath, 'assets');
        html = html.replace(
            /src="\/assets\//g,
            `src="${webview.asWebviewUri(assetsPath)}/`
        );
        html = html.replace(
            /href="\/assets\//g,
            `href="${webview.asWebviewUri(assetsPath)}/`
        );

        // Generate nonce for CSP
        const nonce = getNonce();

        // Update CSP to allow scripts with nonce
        const cspSource = webview.cspSource;
        const csp = `default-src 'none'; 
            style-src ${cspSource} 'unsafe-inline'; 
            script-src 'nonce-${nonce}'; 
            font-src ${cspSource}; 
            img-src ${cspSource} data:;`;

        // Add nonce to all script tags
        html = html.replace(/<script/g, `<script nonce="${nonce}"`);

        // Add CSP meta tag
        html = html.replace(
            '<head>',
            `<head>\n    <meta http-equiv="Content-Security-Policy" content="${csp}">`
        );

        return html;
    }
}

/**
 * Generate a nonce for CSP
 */
function getNonce(): string {
    let text = '';
    const possible = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
    for (let i = 0; i < 32; i++) {
        text += possible.charAt(Math.floor(Math.random() * possible.length));
    }
    return text;
}
