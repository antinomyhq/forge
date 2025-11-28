import * as vscode from 'vscode';

/**
 * Webview provider for the chat interface
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
                vscode.Uri.joinPath(this.extensionUri, 'webview')
            ]
        };

        // Set HTML content
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
        console.log('[WebviewProvider] ========== POSTING MESSAGE ==========');
        console.log('[WebviewProvider] Message:', JSON.stringify(message));
        console.log('[WebviewProvider] View exists:', !!this.view);
        console.log('[WebviewProvider] Webview exists:', !!this.view?.webview);
        
        if (this.view) {
            this.view.webview.postMessage(message);
            console.log('[WebviewProvider] Message posted successfully');
        } else {
            console.log('[WebviewProvider] ERROR: View is null, message not sent!');
        }
        console.log('[WebviewProvider] =========================================');
    }

    /**
     * Update state in webview
     */
    public updateState(state: any): void {
        this.postMessage({
            type: 'state',
            ...state
        });
    }

    /**
     * Show streaming message
     */
    public streamStart(): void {
        console.log('[WebviewProvider] streamStart called');
        this.postMessage({ type: 'streamStart' });
    }

    /**
     * Add delta to streaming message
     */
    public streamDelta(delta: string): void {
        console.log(`[WebviewProvider] streamDelta called: ${delta.substring(0, 50)}...`);
        this.postMessage({ type: 'streamDelta', delta });
    }

    /**
     * End streaming
     */
    public streamEnd(): void {
        console.log('[WebviewProvider] streamEnd called');
        this.postMessage({ type: 'streamEnd' });
    }

    /**
     * Show tool execution
     */
    public showTool(tool: unknown): void {
        this.postMessage({ type: 'tool', data: tool });
    }

    /**
     * Show reasoning
     */
    public showReasoning(text: string): void {
        this.postMessage({ type: 'reasoning', text });
    }

    /**
     * Request approval
     */
    public requestApproval(approval: unknown): void {
        this.postMessage({ type: 'approval', data: approval });
    }

    /**
     * Update header info
     */
    public updateHeader(data: unknown): void {
        this.postMessage({ type: 'updateHeader', data });
    }

    /**
     * Handle messages from webview
     */
    private handleWebviewMessage(message: any): void {
        this.outputChannel.appendLine(`[Webview] ${message.type}`);

        // Call callbacks
        switch (message.type) {
            case 'ready':
                this.outputChannel.appendLine('[Webview] Calling onReady callback');
                if (this.onReadyCallback) {
                    this.onReadyCallback();
                }
                break;
            case 'sendMessage':
                this.outputChannel.appendLine(`[Webview] Calling onSendMessage callback with: ${message.text}`);
                if (this.onSendMessageCallback) {
                    this.onSendMessageCallback(message.text);
                }
                break;
            case 'approval':
                this.outputChannel.appendLine('[Webview] Calling onApproval callback');
                if (this.onApprovalCallback) {
                    this.onApprovalCallback({
                        id: message.id,
                        decision: message.decision
                    });
                }
                break;
        }
    }

    /**
     * Get HTML content for webview
     */
    private getHtmlContent(webview: vscode.Webview): string {
        // Get URIs for resources
        const styleUri = webview.asWebviewUri(
            vscode.Uri.joinPath(this.extensionUri, 'webview', 'style.css')
        );
        const scriptUri = webview.asWebviewUri(
            vscode.Uri.joinPath(this.extensionUri, 'webview', 'main.js')
        );

        // Generate nonce for CSP
        const nonce = getNonce();

        // CSP source
        const cspSource = webview.cspSource;
        
        // For now, inline the HTML (in production, would load from file)
        return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${cspSource} 'unsafe-inline'; script-src 'nonce-${nonce}'; font-src ${cspSource};">
    <link rel="stylesheet" href="${styleUri}">
    <title>ForgeCode Chat</title>
</head>
<body>
    <div class="chat-container">
        <div class="chat-header">
            <div class="header-info">
                <span class="header-item">
                    <span class="codicon codicon-person"></span>
                    <span id="agent-name">Forge</span>
                </span>
                <span class="header-separator">|</span>
                <span class="header-item">
                    <span class="codicon codicon-circuit-board"></span>
                    <span id="model-name">Claude 3.5 Sonnet</span>
                </span>
            </div>
            <div class="header-stats">
                <span class="header-item" id="token-count">0 / 200K tokens</span>
                <span class="header-separator">|</span>
                <span class="header-item" id="cost-display">$0.00</span>
            </div>
        </div>

        <div class="messages-container" id="messages">
            <div class="welcome-screen" id="welcome">
                <div class="welcome-logo">
                    <span class="codicon codicon-sparkle"></span>
                </div>
                <h2>Welcome to ForgeCode</h2>
                <p>Start a conversation to get help with your code.</p>
            </div>
        </div>

        <div class="input-container">
            <div class="input-wrapper">
                <textarea 
                    id="message-input" 
                    class="message-input" 
                    placeholder="Ask ForgeCode anything..."
                    rows="1"
                ></textarea>
                <button id="send-button" class="send-button" title="Send message (Ctrl+Enter)">
                    <span class="codicon codicon-send"></span>
                </button>
            </div>
            <div class="input-footer">
                <span class="input-hint">Press Ctrl+Enter to send</span>
                <span id="char-counter" class="char-counter">0</span>
            </div>
        </div>
    </div>

    <script nonce="${nonce}" src="${scriptUri}"></script>
</body>
</html>`;
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
