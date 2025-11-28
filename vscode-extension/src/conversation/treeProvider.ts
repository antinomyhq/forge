import * as vscode from 'vscode';

/**
 * Tree item for conversation
 */
export class ConversationTreeItem extends vscode.TreeItem {
    constructor(
        public readonly id: string,
        public readonly title: string,
        public readonly messageCount: number,
        public readonly timestamp: number,
        public readonly isActive: boolean
    ) {
        super(title, vscode.TreeItemCollapsibleState.None);
        
        this.tooltip = `${title}\nMessages: ${messageCount}\n${new Date(timestamp).toLocaleString()}`;
        this.description = `${messageCount} messages`;
        this.contextValue = 'conversation';
        
        // Set icon
        this.iconPath = new vscode.ThemeIcon(
            isActive ? 'comment-discussion' : 'comment',
            isActive ? new vscode.ThemeColor('charts.green') : undefined
        );
        
        // Make clickable
        this.command = {
            command: 'forgecode.openConversation',
            title: 'Open Conversation',
            arguments: [this.id]
        };
    }
}

/**
 * Tree data provider for conversations
 */
export class ConversationTreeProvider implements vscode.TreeDataProvider<ConversationTreeItem> {
    private _onDidChangeTreeData = new vscode.EventEmitter<ConversationTreeItem | undefined | void>();
    readonly onDidChangeTreeData = this._onDidChangeTreeData.event;
    
    private conversations: ConversationItem[] = [];
    private activeConversationId: string | null = null;

    constructor(_outputChannel: vscode.OutputChannel) {}

    /**
     * Refresh tree view
     */
    refresh(): void {
        this._onDidChangeTreeData.fire();
    }

    /**
     * Get tree item
     */
    getTreeItem(element: ConversationTreeItem): vscode.TreeItem {
        return element;
    }

    /**
     * Get children (conversations)
     */
    getChildren(): Thenable<ConversationTreeItem[]> {
        if (this.conversations.length === 0) {
            return Promise.resolve([]);
        }

        const items = this.conversations.map(conv => 
            new ConversationTreeItem(
                conv.id,
                conv.title,
                conv.messageCount,
                conv.timestamp,
                conv.id === this.activeConversationId
            )
        );

        return Promise.resolve(items);
    }

    /**
     * Set conversations
     */
    setConversations(conversations: ConversationItem[]): void {
        this.conversations = conversations.sort((a, b) => b.timestamp - a.timestamp);
        this.refresh();
    }

    /**
     * Set active conversation
     */
    setActiveConversation(id: string | null): void {
        this.activeConversationId = id;
        this.refresh();
    }

    /**
     * Add conversation
     */
    addConversation(conversation: ConversationItem): void {
        this.conversations.unshift(conversation);
        this.refresh();
    }

    /**
     * Update conversation
     */
    updateConversation(id: string, updates: Partial<ConversationItem>): void {
        const index = this.conversations.findIndex(c => c.id === id);
        if (index !== -1) {
            this.conversations[index] = { ...this.conversations[index], ...updates };
            this.refresh();
        }
    }

    /**
     * Delete conversation
     */
    deleteConversation(id: string): void {
        this.conversations = this.conversations.filter(c => c.id !== id);
        if (this.activeConversationId === id) {
            this.activeConversationId = null;
        }
        this.refresh();
    }
}

export interface ConversationItem {
    id: string;
    title: string;
    messageCount: number;
    timestamp: number;
}
