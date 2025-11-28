import { EventEmitter } from 'events';
import { JsonRpcRequest, JsonRpcResponse, JsonRpcNotification } from '../generated';

/**
 * JSON-RPC 2.0 client for stdio communication
 * Handles request/response and notification streaming
 */
export class JsonRpcClient extends EventEmitter {
    private stdin: NodeJS.WritableStream;
    private stdout: NodeJS.ReadableStream;
    private nextId = 1;
    private pendingRequests = new Map<number, PendingRequest>();
    private buffer = '';
    private requestTimeout = 30000; // 30 seconds

    constructor(stdin: NodeJS.WritableStream, stdout: NodeJS.ReadableStream) {
        super();
        this.stdin = stdin;
        this.stdout = stdout;
        this.setupStdoutHandler();
    }

    /**
     * Send a JSON-RPC request and wait for response
     */
    async request<T = unknown>(method: string, params?: unknown): Promise<T> {
        const id = this.nextId++;
        const request: Partial<JsonRpcRequest> = {
            jsonrpc: '2.0',
            id,
            method,
        };
        
        // Only include params if defined
        if (params !== undefined) {
            request.params = params;
        }

        // Create promise for response
        const promise = new Promise<T>((resolve, reject) => {
            const timeout = setTimeout(() => {
                this.pendingRequests.delete(id);
                reject(new Error(`Request timeout: ${method}`));
            }, this.requestTimeout);

            this.pendingRequests.set(id, {
                resolve: resolve as (value: unknown) => void,
                reject,
                timeout
            });
        });

        // Send request
        this.send(request as JsonRpcRequest);

        return promise;
    }

    /**
     * Send a JSON-RPC notification (no response expected)
     */
    notify(method: string, params: unknown): void {
        const notification: JsonRpcNotification = {
            jsonrpc: '2.0',
            method,
            params,
        };

        this.send(notification);
    }

    /**
     * Send a message to server
     */
    private send(message: JsonRpcRequest | JsonRpcNotification): void {
        const json = JSON.stringify(message);
        this.stdin.write(json + '\n');
    }

    /**
     * Set up stdout handler to process responses and notifications
     */
    private setupStdoutHandler(): void {
        this.stdout.on('data', (data: Buffer) => {
            this.handleData(data.toString());
        });
    }

    /**
     * Handle incoming data from stdout
     * Processes line-delimited JSON-RPC messages
     */
    private handleData(data: string): void {
        this.buffer += data;

        // Process complete lines
        let newlineIndex: number;
        while ((newlineIndex = this.buffer.indexOf('\n')) !== -1) {
            const line = this.buffer.substring(0, newlineIndex).trim();
            this.buffer = this.buffer.substring(newlineIndex + 1);

            if (line.length === 0) {
                continue;
            }

            try {
                const message = JSON.parse(line);
                this.handleMessage(message);
            } catch (error) {
                this.emit('error', new Error(`Failed to parse JSON: ${line}`));
            }
        }
    }

    /**
     * Handle a parsed JSON-RPC message
     */
    private handleMessage(message: unknown): void {
        // Type guard for message structure
        if (!message || typeof message !== 'object') {
            return;
        }

        const msg = message as Record<string, unknown>;

        // Check if it's a response (has 'id' field)
        if ('id' in msg && typeof msg.id === 'number') {
            this.handleResponse(msg as unknown as JsonRpcResponse);
        }
        // Check if it's a notification (has 'method' but no 'id')
        else if ('method' in msg && typeof msg.method === 'string' && !('id' in msg)) {
            this.handleNotification(msg as unknown as JsonRpcNotification);
        }
    }

    /**
     * Handle JSON-RPC response
     */
    private handleResponse(response: JsonRpcResponse): void {
        const responseId = response.id as number;
        const pending = this.pendingRequests.get(responseId);

        if (!pending) {
            this.emit('warning', `Received response for unknown request ID: ${responseId}`);
            return;
        }

        // Clear timeout and remove from pending
        clearTimeout(pending.timeout);
        this.pendingRequests.delete(responseId);

        // Resolve or reject based on response
        if (response.error) {
            pending.reject(new Error(response.error.message));
        } else {
            pending.resolve(response.result);
        }
    }

    /**
     * Handle JSON-RPC notification
     */
    private handleNotification(notification: JsonRpcNotification): void {
        // Emit notification event with method and params
        this.emit('notification', notification.method, notification.params);
    }

    /**
     * Dispose of resources
     */
    dispose(): void {
        // Reject all pending requests
        for (const [, pending] of this.pendingRequests) {
            clearTimeout(pending.timeout);
            pending.reject(new Error('Client disposed'));
        }
        this.pendingRequests.clear();
        this.removeAllListeners();
    }
}

interface PendingRequest {
    resolve: (value: unknown) => void;
    reject: (error: Error) => void;
    timeout: NodeJS.Timeout;
}
