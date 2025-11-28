import * as vscode from 'vscode';
import * as fs from 'fs';
import * as path from 'path';

/**
 * Manages file context for chat (@-mentions)
 */
export class FileContextManager {
    private taggedFiles = new Set<string>();
    private outputChannel: vscode.OutputChannel;
    
    constructor(
        private context: vscode.ExtensionContext,
        outputChannel: vscode.OutputChannel
    ) {
        this.outputChannel = outputChannel;
        this.loadTaggedFiles();
    }

    /**
     * Tag a file for chat context
     */
    async tagFile(uri: vscode.Uri): Promise<void> {
        const relativePath = vscode.workspace.asRelativePath(uri);
        this.taggedFiles.add(relativePath);
        await this.saveTaggedFiles();
        
        this.outputChannel.appendLine(`[FileContext] Tagged: ${relativePath}`);
        vscode.window.showInformationMessage(`Tagged: ${relativePath}`);
    }

    /**
     * Untag a file
     */
    async untagFile(path: string): Promise<void> {
        this.taggedFiles.delete(path);
        await this.saveTaggedFiles();
        
        this.outputChannel.appendLine(`[FileContext] Untagged: ${path}`);
    }

    /**
     * Get all tagged files
     */
    getTaggedFiles(): string[] {
        return Array.from(this.taggedFiles);
    }

    /**
     * Clear all tagged files
     */
    async clearAll(): Promise<void> {
        this.taggedFiles.clear();
        await this.saveTaggedFiles();
        
        this.outputChannel.appendLine('[FileContext] Cleared all tags');
        vscode.window.showInformationMessage('Cleared all tagged files');
    }

    /**
     * Get file contents for tagged files
     */
    async getTaggedFileContents(): Promise<TaggedFileContent[]> {
        const contents: TaggedFileContent[] = [];
        
        for (const relativePath of this.taggedFiles) {
            const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
            if (!workspaceFolder) continue;
            
            const fullPath = path.join(workspaceFolder.uri.fsPath, relativePath);
            
            try {
                const content = await fs.promises.readFile(fullPath, 'utf-8');
                contents.push({
                    path: relativePath,
                    content: content
                });
            } catch (error) {
                this.outputChannel.appendLine(`[FileContext] Error reading ${relativePath}: ${error}`);
            }
        }
        
        return contents;
    }

    /**
     * Format tagged files as @[file] references
     */
    formatAsReferences(): string {
        return Array.from(this.taggedFiles)
            .map(path => `@[${path}]`)
            .join(' ');
    }

    /**
     * Parse @[file] references from text
     */
    parseReferences(text: string): string[] {
        const regex = /@\[([^\]]+)\]/g;
        const matches: string[] = [];
        let match;
        
        while ((match = regex.exec(text)) !== null) {
            matches.push(match[1]);
        }
        
        return matches;
    }

    /**
     * Show quick pick of tagged files
     */
    async showTaggedFiles(): Promise<void> {
        const files = this.getTaggedFiles();
        
        if (files.length === 0) {
            vscode.window.showInformationMessage('No files are tagged');
            return;
        }

        const items = files.map(file => ({
            label: file,
            description: 'Tagged file'
        }));

        const selected = await vscode.window.showQuickPick(items, {
            placeHolder: 'Select a tagged file to open',
            canPickMany: false
        });

        if (selected) {
            const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
            if (workspaceFolder) {
                const uri = vscode.Uri.joinPath(workspaceFolder.uri, selected.label);
                await vscode.window.showTextDocument(uri);
            }
        }
    }

    /**
     * Show file picker to tag files
     */
    async showFilePicker(): Promise<void> {
        const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
        if (!workspaceFolder) {
            vscode.window.showErrorMessage('No workspace folder open');
            return;
        }

        // Find files in workspace
        const files = await vscode.workspace.findFiles(
            '**/*',
            '{**/node_modules/**,**/.git/**,**/dist/**,**/build/**,**/out/**,**/target/**}',
            1000
        );

        const items = files.map(uri => {
            const relativePath = vscode.workspace.asRelativePath(uri);
            return {
                label: path.basename(relativePath),
                description: path.dirname(relativePath),
                uri: uri,
                picked: this.taggedFiles.has(relativePath)
            };
        });

        const selected = await vscode.window.showQuickPick(items, {
            placeHolder: 'Select files to tag for chat context',
            canPickMany: true
        });

        if (selected) {
            for (const item of selected) {
                await this.tagFile(item.uri);
            }
        }
    }

    /**
     * Load tagged files from workspace state
     */
    private loadTaggedFiles(): void {
        const saved = this.context.workspaceState.get<string[]>('taggedFiles', []);
        this.taggedFiles = new Set(saved);
        this.outputChannel.appendLine(`[FileContext] Loaded ${saved.length} tagged files`);
    }

    /**
     * Save tagged files to workspace state
     */
    private async saveTaggedFiles(): Promise<void> {
        await this.context.workspaceState.update('taggedFiles', Array.from(this.taggedFiles));
    }

    /**
     * Dispose of resources
     */
    dispose(): void {
        this.saveTaggedFiles();
    }
}

export interface TaggedFileContent {
    path: string;
    content: string;
}
