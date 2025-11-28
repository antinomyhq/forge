import * as vscode from 'vscode';

/**
 * Settings webview provider
 */
export class SettingsWebviewProvider implements vscode.WebviewViewProvider {
    public static readonly viewType = 'forgecode.settings';
    
    private _view?: vscode.WebviewView;

    constructor(
        private readonly extensionUri: vscode.Uri,
        private outputChannel: vscode.OutputChannel
    ) {}

    /**
     * Resolve webview view
     */
    public resolveWebviewView(
        webviewView: vscode.WebviewView,
        _context: vscode.WebviewViewResolveContext,
        _token: vscode.CancellationToken
    ) {
        this._view = webviewView;

        webviewView.webview.options = {
            enableScripts: true,
            localResourceRoots: [this.extensionUri]
        };

        webviewView.webview.html = this.getHtmlForWebview(webviewView.webview);

        // Handle messages from webview
        webviewView.webview.onDidReceiveMessage(async (data) => {
            switch (data.type) {
                case 'updateSetting':
                    await this.updateSetting(data.key, data.value);
                    break;
                case 'getSetting':
                    const value = this.getSetting(data.key);
                    webviewView.webview.postMessage({
                        type: 'settingValue',
                        key: data.key,
                        value: value
                    });
                    break;
                case 'getAllSettings':
                    const settings = this.getAllSettings();
                    webviewView.webview.postMessage({
                        type: 'allSettings',
                        settings: settings
                    });
                    break;
            }
        });

        // Send initial settings
        this.sendAllSettings();
    }

    /**
     * Update a setting
     */
    private async updateSetting(key: string, value: any): Promise<void> {
        try {
            const config = vscode.workspace.getConfiguration('forgecode');
            await config.update(key, value, vscode.ConfigurationTarget.Workspace);
            
            this.outputChannel.appendLine(`[Settings] Updated ${key} = ${value}`);
            vscode.window.showInformationMessage(`Updated: ${key}`);
            
            // Send updated settings
            this.sendAllSettings();
        } catch (error) {
            this.outputChannel.appendLine(`[Settings] Error updating ${key}: ${error}`);
            vscode.window.showErrorMessage(`Failed to update ${key}`);
        }
    }

    /**
     * Get a setting value
     */
    private getSetting(key: string): any {
        const config = vscode.workspace.getConfiguration('forgecode');
        return config.get(key);
    }

    /**
     * Get all settings
     */
    private getAllSettings(): Record<string, any> {
        const config = vscode.workspace.getConfiguration('forgecode');
        return {
            serverPath: config.get('serverPath'),
            autoStart: config.get('autoStart'),
            defaultAgent: config.get('defaultAgent'),
            defaultModel: config.get('defaultModel'),
            logLevel: config.get('logLevel')
        };
    }

    /**
     * Send all settings to webview
     */
    private sendAllSettings(): void {
        if (this._view) {
            const settings = this.getAllSettings();
            this._view.webview.postMessage({
                type: 'allSettings',
                settings: settings
            });
        }
    }

    /**
     * Get HTML for webview
     */
    private getHtmlForWebview(_webview: vscode.Webview): string {
        return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>ForgeCode Settings</title>
    <style>
        body {
            padding: 10px;
            color: var(--vscode-foreground);
            font-family: var(--vscode-font-family);
            font-size: var(--vscode-font-size);
        }
        
        .setting {
            margin-bottom: 20px;
        }
        
        .setting-label {
            display: block;
            margin-bottom: 5px;
            font-weight: 600;
        }
        
        .setting-description {
            display: block;
            margin-bottom: 8px;
            font-size: 0.9em;
            color: var(--vscode-descriptionForeground);
        }
        
        input[type="text"],
        select {
            width: 100%;
            padding: 6px 8px;
            background: var(--vscode-input-background);
            color: var(--vscode-input-foreground);
            border: 1px solid var(--vscode-input-border);
            border-radius: 2px;
            font-family: inherit;
            font-size: inherit;
        }
        
        input[type="text"]:focus,
        select:focus {
            outline: 1px solid var(--vscode-focusBorder);
        }
        
        input[type="checkbox"] {
            margin-right: 8px;
        }
        
        .checkbox-container {
            display: flex;
            align-items: center;
            cursor: pointer;
        }
        
        button {
            padding: 6px 14px;
            background: var(--vscode-button-background);
            color: var(--vscode-button-foreground);
            border: none;
            border-radius: 2px;
            cursor: pointer;
            font-family: inherit;
            font-size: inherit;
        }
        
        button:hover {
            background: var(--vscode-button-hoverBackground);
        }
        
        .section-title {
            font-size: 1.1em;
            font-weight: 600;
            margin: 20px 0 10px 0;
            padding-bottom: 5px;
            border-bottom: 1px solid var(--vscode-panel-border);
        }
    </style>
</head>
<body>
    <h2>ForgeCode Settings</h2>
    
    <div class="section-title">Server Configuration</div>
    
    <div class="setting">
        <label class="setting-label" for="serverPath">Server Path</label>
        <span class="setting-description">Path to forge-app-server binary</span>
        <input type="text" id="serverPath" />
    </div>
    
    <div class="setting">
        <label class="checkbox-container">
            <input type="checkbox" id="autoStart" />
            <span>Auto-start server on activation</span>
        </label>
    </div>
    
    <div class="section-title">Defaults</div>
    
    <div class="setting">
        <label class="setting-label" for="defaultAgent">Default Agent</label>
        <span class="setting-description">Agent to use for new conversations</span>
        <select id="defaultAgent">
            <option value="">None (prompt each time)</option>
            <option value="forge">Forge</option>
            <option value="sage">Sage</option>
            <option value="muse">Muse</option>
        </select>
    </div>
    
    <div class="setting">
        <label class="setting-label" for="defaultModel">Default Model</label>
        <span class="setting-description">Model to use for new conversations</span>
        <input type="text" id="defaultModel" placeholder="e.g., claude-3-5-sonnet-20241022" />
    </div>
    
    <div class="section-title">Logging</div>
    
    <div class="setting">
        <label class="setting-label" for="logLevel">Log Level</label>
        <span class="setting-description">Verbosity of logs in Output panel</span>
        <select id="logLevel">
            <option value="error">Error</option>
            <option value="warn">Warning</option>
            <option value="info">Info</option>
            <option value="debug">Debug</option>
        </select>
    </div>
    
    <script>
        const vscode = acquireVsCodeApi();
        
        // Get all settings on load
        vscode.postMessage({ type: 'getAllSettings' });
        
        // Handle messages from extension
        window.addEventListener('message', event => {
            const message = event.data;
            
            if (message.type === 'allSettings') {
                updateUI(message.settings);
            }
        });
        
        // Update UI with settings
        function updateUI(settings) {
            document.getElementById('serverPath').value = settings.serverPath || '';
            document.getElementById('autoStart').checked = settings.autoStart || false;
            document.getElementById('defaultAgent').value = settings.defaultAgent || '';
            document.getElementById('defaultModel').value = settings.defaultModel || '';
            document.getElementById('logLevel').value = settings.logLevel || 'info';
        }
        
        // Add change listeners
        document.getElementById('serverPath').addEventListener('change', (e) => {
            vscode.postMessage({
                type: 'updateSetting',
                key: 'serverPath',
                value: e.target.value
            });
        });
        
        document.getElementById('autoStart').addEventListener('change', (e) => {
            vscode.postMessage({
                type: 'updateSetting',
                key: 'autoStart',
                value: e.target.checked
            });
        });
        
        document.getElementById('defaultAgent').addEventListener('change', (e) => {
            vscode.postMessage({
                type: 'updateSetting',
                key: 'defaultAgent',
                value: e.target.value
            });
        });
        
        document.getElementById('defaultModel').addEventListener('change', (e) => {
            vscode.postMessage({
                type: 'updateSetting',
                key: 'defaultModel',
                value: e.target.value
            });
        });
        
        document.getElementById('logLevel').addEventListener('change', (e) => {
            vscode.postMessage({
                type: 'updateSetting',
                key: 'logLevel',
                value: e.target.value
            });
        });
    </script>
</body>
</html>`;
    }
}
