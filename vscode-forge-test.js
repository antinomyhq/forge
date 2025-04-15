#!/usr/bin/env node

/**
 * VS Code Extension Test Simulator for Forge Server
 * 
 * This script simulates how a VS Code extension would interact with
 * the Forge Server using stdin/stdout for communication. It provides
 * a simple CLI interface to test server functionality.
 * 
 * Usage:
 *   node vscode-forge-test.js
 */

const { spawn } = require('child_process');
const readline = require('readline');
const crypto = require('crypto');

// Global variables
let serverProcess = null;
let serverReady = false;
let requestId = 1;
let conversationId = null;
let responseCallbacks = new Map();
let dataBuffer = '';
let activeStreams = new Map(); // Track active streams

// Create readline interface for user input
const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
    prompt: '> '
});

/**
 * Start the forge server process
 */
function startServer() {
    console.log('Starting forge_server...');
    
    serverProcess = spawn('cargo', ['run', '--bin', 'forge-server'], {
        stdio: ['pipe', 'pipe', 'pipe'], // stdin, stdout, stderr
        cwd: process.cwd(),
        shell: true
    });
    
    // Handle server output (stdout)
    serverProcess.stdout.on('data', (data) => {
        const output = data.toString();
        console.log(`[RAW] <<< ${output.trim()}`);
        parseResponses(output);
    });
    
    // Handle server logs (stderr)
    serverProcess.stderr.on('data', (data) => {
        const output = data.toString().trim();
        
        if (output) {
            console.log(`\n[Server Log] ${output}`);
        }
        
        // Check for server ready signal
        if ((output.includes('JSON-RPC server ready to accept connections') || 
             output.includes('Server thread started')) && 
            !serverReady) {
            serverReady = true;
            console.log('\n✅ Forge server is ready\n');
            showHelp();
            rl.prompt();
        }
    });
    
    // Handle server exit
    serverProcess.on('exit', (code) => {
        console.log(`\nForge server exited with code ${code}`);
        rl.close();
        process.exit(0);
    });
    
    // Handle errors
    serverProcess.on('error', (err) => {
        console.error(`\nFailed to start forge_server: ${err.message}`);
        rl.close();
        process.exit(1);
    });
}

/**
 * Parse JSON-RPC responses from the server
 */
function parseResponses(data) {
    try {
        // Split by newlines in case multiple responses were sent
        const lines = data.toString().trim().split('\n');
        
        for (const line of lines) {
            if (!line.trim()) continue;
            
            console.log(`\n[Processing line]: ${line}`);
            
            // Try to parse the line as JSON
            try {
                const parsed = JSON.parse(line);
                console.log('\n[Parsed JSON]:', JSON.stringify(parsed, null, 2));
                handleResponse(parsed);
            } catch (e) {
                console.log(`\n[Parse error]: ${e.message}, adding to buffer`);
                
                // Add to buffer and try to parse
                dataBuffer += line;
                try {
                    const parsed = JSON.parse(dataBuffer);
                    console.log('\n[Buffer parsed]:', JSON.stringify(parsed, null, 2));
                    dataBuffer = '';
                    handleResponse(parsed);
                } catch (e) {
                    // Still can't parse, keep in buffer for next chunk
                    console.log(`\n[Buffer parse error]: ${e.message}`);
                }
            }
        }
    } catch (e) {
        console.error(`Error processing server response: ${e.message}`);
    }
}

/**
 * Handle a parsed JSON-RPC response
 */
function handleResponse(response) {
    console.log('\n📥 Received response:', JSON.stringify(response, null, 2));
    
    // Check if this is a stream notification
    if (response.method && response.method === 'chat.update') {
        handleStreamNotification(response);
        rl.prompt();
        return;
    } else if (response.method === 'chat.complete') {
        if (response.params && response.params.length >= 2) {
            const streamId = response.params[0];
            console.log(`\n✅ Stream ${streamId} complete`);
            
            if (activeStreams.has(streamId)) {
                const streamInfo = activeStreams.get(streamId);
                streamInfo.complete = true;
                
                // Print the complete message
                if (streamInfo.messages.length > 0) {
                    const fullMessage = streamInfo.messages.join('');
                    console.log('\n📩 Complete AI response:');
                    console.log('-------------------------------------');
                    console.log(fullMessage);
                    console.log('-------------------------------------');
                }
            }
        }
        rl.prompt();
        return;
    }
    
    // This is a regular response to a request
    const callback = responseCallbacks.get(response.id);
    if (callback) {
        responseCallbacks.delete(response.id);
        
        // If this is a chat response, store the stream ID
        if (response.result && typeof response.result === 'string' && 
            response.result.match(/[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}/)) {
            activeStreams.set(response.result, { messages: [], complete: false });
            console.log(`\n💬 Started new stream: ${response.result}`);
        }
        
        if (response.error) {
            callback.reject(response.error);
        } else {
            callback.resolve(response.result);
        }
    }
    
    rl.prompt();
}

/**
 * Handle a stream notification
 */
function handleStreamNotification(notification) {
    if (!notification || !notification.params) {
        console.log(`\n⚠️ Invalid stream notification:`, JSON.stringify(notification, null, 2));
        return;
    }
    
    // Extract stream ID and message object from params
    let streamId, messageObj;
    
    // Debug the notification structure
    console.log('\n🔍 Notification structure:', JSON.stringify(notification, null, 2));
    
    if (Array.isArray(notification.params)) {
        // Format: [stream_id, messageObj]
        streamId = notification.params[0];
        messageObj = notification.params[1];
        console.log(`\n📨 Stream ${streamId} message:`, JSON.stringify(messageObj, null, 2));
    } else {
        console.log(`\n⚠️ Unexpected params format:`, JSON.stringify(notification.params, null, 2));
        return;
    }
    
    if (!streamId || !messageObj) {
        console.log('\n⚠️ Missing stream ID or message object');
        return;
    }
    
    // Extract content from the message object
    let content = null;
    console.log(`\n🔎 Examining message object type:`, typeof messageObj, messageObj);
    
    if (typeof messageObj === 'object') {
        // First check for the expected format from test messages
        if (messageObj.type === 'text' && messageObj.content) {
            content = messageObj.content;
            console.log(`\n✅ Found text message with content: "${content}"`);
        }
        // Check for debug messages
        else if (messageObj.type === 'debug') {
            content = `[DEBUG] ${messageObj.content}`;
            console.log(`\n🐛 Debug message: ${content}`);
        }
        // Check for other formats
        else if (messageObj.content && typeof messageObj.content === 'string') {
            content = messageObj.content;
            console.log(`\n📄 Found content field: "${content}"`);
        }
        // Try to find content in message field (nested)
        else if (messageObj.message && messageObj.message.text) {
            content = messageObj.message.text;
            console.log(`\n📑 Found nested text: "${content}"`);
        }
        // As a fallback, stringify the whole object
        else {
            console.log('\n⚠️ Could not find content in message object, using fallback');
            content = JSON.stringify(messageObj);
        }
    } else if (typeof messageObj === 'string') {
        // Direct string content
        content = messageObj;
        console.log(`\n📝 Direct string content: "${content}"`);
    }
    
    // Store the message content if we have a valid stream
    if (streamId && content) {
        if (!activeStreams.has(streamId)) {
            activeStreams.set(streamId, { messages: [], complete: false });
            console.log(`\n📝 Created new stream record for ${streamId}`);
        }
        
        const streamInfo = activeStreams.get(streamId);
        streamInfo.messages.push(content);
        
        // Display the accumulated message
        console.log('\nCurrent accumulated message:');
        console.log('-------------------------------------');
        console.log(streamInfo.messages.join(''));
        console.log('-------------------------------------');
    } else {
        console.log(`\n⚠️ Could not add message to stream - missing ID (${streamId}) or content (${content})`);
    }
}

/**
 * Send a JSON-RPC request to the server
 */
function sendRequest(method, params = null) {
    return new Promise((resolve, reject) => {
        const id = requestId++;
        
        const request = {
            jsonrpc: '2.0',
            id,
            method,
            params
        };
        
        console.log('\n📤 Sending request:');
        console.log(JSON.stringify(request, null, 2));
        
        // Store the callback for this request ID
        responseCallbacks.set(id, { resolve, reject });
        
        // Write the request to the server's stdin
        const requestStr = JSON.stringify(request) + '\n';
        console.log(`[RAW] >>> ${requestStr.trim()}`);
        serverProcess.stdin.write(requestStr);
    });
}

/**
 * Initialize a new conversation
 */
async function initConversation() {
    try {
        conversationId = await sendRequest('init');
        console.log(`\n✅ Conversation initialized with ID: ${conversationId}\n`);
    } catch (error) {
        console.error(`\n❌ Failed to initialize conversation: ${error.message}\n`);
    }
}

/**
 * Send a chat message
 */
async function sendChatMessage(message) {
    if (!conversationId) {
        console.log('\n❌ Please initialize a conversation first using "init"\n');
        return;
    }
    
    try {
        const chatRequest = {
            conversation_id: conversationId,
            event: {
                id: crypto.randomUUID(),
                name: 'user_task_init',
                value: message,
                timestamp: new Date().toISOString()
            }
        };
        
        console.log('\n📤 Sending chat request with conversation ID:', conversationId);
        const streamId = await sendRequest('chat', chatRequest);
        console.log(`\n✅ Message sent, stream ID: ${streamId}\n`);
        
        // Manually create an initial debug message to test stream ID tracking
        const debugTestMessage = {
            jsonrpc: '2.0',
            method: 'chat.update',
            params: [
                streamId,
                {
                    type: 'debug',
                    content: '[Client Debug] Testing stream connectivity'
                }
            ]
        };
        
        console.log('\n🧪 Injecting test message for stream ID:', streamId);
        handleResponse(debugTestMessage);
        
        // Wait a bit and send a text message to simulate actual server response
        setTimeout(() => {
            const testTextMessage = {
                jsonrpc: '2.0',
                method: 'chat.update',
                params: [
                    streamId,
                    {
                        type: 'text',
                        content: 'This is a client-generated test message for debugging.'
                    }
                ]
            };
            console.log('\n🧪 Injecting text message for stream ID:', streamId);
            handleResponse(testTextMessage);
        }, 500);
        
    } catch (error) {
        console.error(`\n❌ Failed to send message: ${error.message}\n`);
    }
}

/**
 * Handle user commands
 */
function processCommand(input) {
    const trimmedInput = input.trim();
    
    // Empty input - just prompt again
    if (!trimmedInput) {
        rl.prompt();
        return;
    }
    
    // Split input into command and arguments
    const [command, ...args] = trimmedInput.split(' ');
    
    switch (command) {
        case 'help':
            showHelp();
            break;
        case 'init':
            initConversation();
            break;
        case 'send':
            if (args.length === 0) {
                console.log('\n❌ Please provide a message to send\n');
                rl.prompt();
            } else {
                sendChatMessage(args.join(' '));
            }
            break;
        case 'test':
            simulateChatForTesting();
            break;
        case 'streams':
            showActiveStreams();
            break;
        case 'exit':
        case 'quit':
            console.log('\nShutting down server...');
            serverProcess.kill('SIGINT');
            break;
        default:
            console.log(`\n❌ Unknown command: ${command}`);
            console.log('Type "help" to see available commands\n');
            rl.prompt();
    }
}

/**
 * Display help information
 */
function showHelp() {
    console.log('\n--- VS Code Extension Test Simulator for Forge Server ---');
    console.log('Available commands:');
    console.log('  help        - Show this help message');
    console.log('  init        - Initialize a new conversation');
    console.log('  send <msg>  - Send a message to the server');
    console.log('  test        - Run a test simulation of streaming messages');
    console.log('  streams     - Show active streams');
    console.log('  quit/exit   - Shut down the server and exit');
    console.log('--------------------------------------------------\n');
    rl.prompt();
}

/**
 * Simulate a chat session for testing
 */
function simulateChatForTesting() {
    // Generate a test stream ID
    const testStreamId = crypto.randomUUID();
    activeStreams.set(testStreamId, { messages: [], complete: false });
    
    console.log(`\n💬 TEST: Started new stream: ${testStreamId}`);
    
    // Simulate a few messages
    setTimeout(() => {
        const testMessage1 = {
            jsonrpc: '2.0',
            method: 'chat.update',
            params: [
                testStreamId,
                {
                    type: 'text',
                    content: 'Hello! This is a '
                }
            ]
        };
        handleResponse(testMessage1);
    }, 500);
    
    setTimeout(() => {
        const testMessage2 = {
            jsonrpc: '2.0',
            method: 'chat.update',
            params: [
                testStreamId,
                {
                    type: 'text',
                    content: 'test message simulating '
                }
            ]
        };
        handleResponse(testMessage2);
    }, 1000);
    
    setTimeout(() => {
        const testMessage3 = {
            jsonrpc: '2.0',
            method: 'chat.update',
            params: [
                testStreamId,
                {
                    type: 'text',
                    content: 'a streaming response.'
                }
            ]
        };
        handleResponse(testMessage3);
    }, 1500);
    
    // Complete the stream
    setTimeout(() => {
        const completeMessage = {
            jsonrpc: '2.0',
            method: 'chat.complete',
            params: [
                testStreamId,
                true
            ]
        };
        handleResponse(completeMessage);
    }, 2000);
}

/**
 * Show active streams and their status
 */
function showActiveStreams() {
    if (activeStreams.size === 0) {
        console.log('\n💾 No active streams');
        rl.prompt();
        return;
    }
    
    console.log('\n📡 Active streams:');
    console.log('--------------------------------------------------');
    
    activeStreams.forEach((info, streamId) => {
        const status = info.complete ? 'Complete' : 'In progress';
        const messageCount = info.messages.length;
        console.log(`Stream ID: ${streamId}`);
        console.log(`Status: ${status}`);
        console.log(`Message parts: ${messageCount}`);
        
        if (messageCount > 0) {
            console.log('\nMessages:');
            console.log('--------------------------------------------------');
            console.log(info.messages.join(''));
            console.log('--------------------------------------------------');
        }
        
        console.log(); // Empty line between streams
    });
    
    rl.prompt();
}

// Start the server and set up the readline interface
startServer();

rl.on('line', (input) => {
    processCommand(input);
});

rl.on('close', () => {
    if (serverProcess) {
        serverProcess.kill('SIGINT');
    }
    console.log('\nGoodbye!');
    process.exit(0);
});

// Handle Ctrl+C to gracefully shutdown
process.on('SIGINT', () => {
    console.log('\nShutting down server...');
    if (serverProcess) {
        serverProcess.kill('SIGINT');
    } else {
        process.exit(0);
    }
});

console.log('Waiting for server to start...');