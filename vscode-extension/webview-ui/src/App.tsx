import { useState, useRef, useEffect } from 'react';
import { useChatState, useChatStateUpdater } from './presentation/hooks/useChatStateSimple';
import { useChatActions } from './presentation/hooks/useChatActionsSimple';
import './index.css';

function App() {
  const chatState = useChatState();
  const { updateFromMessage } = useChatStateUpdater();
  const { sendMessage, changeModel, initialize, cancelMessage } = useChatActions();
  
  const [showModelPicker, setShowModelPicker] = useState(false);
  const [modelSearch, setModelSearch] = useState('');
  const [input, setInput] = useState('');
  const messagesEndRef = useRef<HTMLDivElement>(null);
  
  // Log button state changes
  useEffect(() => {
    const showCancel = chatState.isStreaming || chatState.isLoading || !!chatState.currentTurnId;
    console.log('[App] Button state:', {
      showCancel,
      isLoading: chatState.isLoading,
      isStreaming: chatState.isStreaming,
      currentTurnId: chatState.currentTurnId,
    });
  }, [chatState.isLoading, chatState.isStreaming, chatState.currentTurnId]);

  // Initialize and listen for messages
  useEffect(() => {
    console.log('[App] Initializing with Effect-TS...');
    
    // Initialize (send ready + request models)
    initialize();

    // Listen for messages from extension
    const handleMessage = (event: MessageEvent) => {
      const message = event.data;
      console.log('[App] Received message:', message);
      
      // Update Effect state
      updateFromMessage(message);
    };

    window.addEventListener('message', handleMessage);
    console.log('[App] Message listener attached');

    return () => {
      window.removeEventListener('message', handleMessage);
      console.log('[App] Message listener detached');
    };
  }, [initialize, updateFromMessage]);

  // Scroll to bottom when messages change
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [chatState.messages, chatState.streamingContent]);

  // Handle send message
  const handleSend = () => {
    if (!input.trim() || chatState.isStreaming) return;
    console.log('[App] Sending message via Effect:', input);
    console.log('[App] Current state before send:', {
      isLoading: chatState.isLoading,
      isStreaming: chatState.isStreaming,
      currentTurnId: chatState.currentTurnId,
    });
    sendMessage(input);
    setInput('');
  };

  // Handle Enter key
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && e.ctrlKey) {
      e.preventDefault();
      handleSend();
    }
  };

  // Handle model selection
  const handleModelSelect = (modelId: string) => {
    console.log('[App] Changing model via Effect:', modelId);
    changeModel(modelId);
    setShowModelPicker(false);
    setModelSearch('');
  };

  // Filter models
  const filteredModels = chatState.models.filter((m: any) => {
    const displayName = m.label || m.name || m.id;
    const searchLower = modelSearch.toLowerCase();
    return displayName.toLowerCase().includes(searchLower) ||
           m.provider?.toLowerCase().includes(searchLower);
  });

  // Render message content
  const renderMessageContent = (msg: any) => {
    // Tool call log message
    if (msg.type === 'tool') {
      return (
        <div className="tool-call-log">
          <div className="tool-call-header">
            <span className="codicon codicon-tools"></span>
            <span className="tool-name">Execute {msg.toolName}</span>
            {msg.status === 'running' && (
              <span className="tool-status running">
                <span className="codicon codicon-loading codicon-modifier-spin"></span>
                Running...
              </span>
            )}
            {msg.status === 'completed' && (
              <span className="tool-status completed">
                <span className="codicon codicon-check"></span>
                Completed
              </span>
            )}
            {msg.status === 'failed' && (
              <span className="tool-status failed">
                <span className="codicon codicon-error"></span>
                Failed
              </span>
            )}
          </div>
          {msg.args && Object.keys(msg.args).length > 0 && (
            <div className="tool-arguments">
              {Object.entries(msg.args).map(([key, value]) => (
                <div key={key} className="tool-arg">
                  <span className="arg-key">{key}:</span>
                  <span className="arg-value">
                    {typeof value === 'string' ? value : JSON.stringify(value)}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>
      );
    }
    
    // Regular message
    return <div className="message-content">{msg.content}</div>;
  };

  return (
    <div className="chat-container">
      {/* Header */}
      <div className="chat-header">
        <div className="header-info">
          <span className="header-item">
            <span className="codicon codicon-person"></span>
            <span>{chatState.agentName}</span>
          </span>
          <span className="header-separator">|</span>
          <span className="header-item">
            <span className="codicon codicon-circuit-board"></span>
            <div className="model-picker">
              <button
                className="model-button"
                onClick={() => setShowModelPicker(!showModelPicker)}
                title="Click to change model"
              >
                <span>{chatState.modelName}</span>
                <span className="codicon codicon-chevron-down"></span>
              </button>
              <div className={`model-dropdown ${showModelPicker ? '' : 'hidden'}`}>
                <input
                  type="text"
                  className="model-search"
                  placeholder="Search models..."
                  value={modelSearch}
                  onChange={(e) => setModelSearch(e.target.value)}
                />
                <div className="model-list">
                  {filteredModels.length > 0 ? (
                    filteredModels.map((model: any) => (
                      <div
                        key={model.id}
                        className="model-item"
                        onClick={() => handleModelSelect(model.id)}
                      >
                        <div className="model-item-label">{model.label || model.name || model.id}</div>
                        {(model.provider || model.contextWindow) && (
                          <div className="model-item-details">
                            {model.provider && <span>{model.provider}</span>}
                            {model.contextWindow && (
                              <span>{model.contextWindow} tokens</span>
                            )}
                          </div>
                        )}
                      </div>
                    ))
                  ) : (
                    <div className="no-models">
                      {chatState.models.length === 0 ? 'Loading models...' : 'No models found'}
                    </div>
                  )}
                </div>
              </div>
            </div>
          </span>
        </div>
        <div className="header-stats">
          <span className="header-item">{chatState.tokenCount}</span>
          <span className="header-separator">|</span>
          <span className="header-item">{chatState.cost}</span>
        </div>
      </div>

      {/* Messages */}
      <div className="messages-container">
        {chatState.messages.length === 0 && !chatState.isStreaming && !chatState.isLoading ? (
          <div className="welcome-screen">
            <h2>Welcome to ForgeCode</h2>
            <p>Start a conversation to get help with your code.</p>
          </div>
        ) : (
          <>
            {chatState.messages.map((msg: any, idx: number) => (
              <div key={idx} className={`message ${msg.role || ''} ${msg.type === 'tool' ? 'tool-message' : ''}`}>
                {renderMessageContent(msg)}
              </div>
            ))}
            
            {/* Loading spinner (shown before stream starts) */}
            {chatState.isLoading && !chatState.isStreaming && (
              <div className="loading-indicator">
                <span className="codicon codicon-loading codicon-modifier-spin"></span>
                <span>Thinking...</span>
              </div>
            )}
            
            {/* Streaming message */}
            {chatState.isStreaming && (
              <div className="message assistant streaming">
                <div className="message-content">
                  {chatState.streamingContent || (
                    <span className="thinking-indicator">
                      <span className="codicon codicon-loading codicon-modifier-spin"></span>
                      <span>Thinking...</span>
                    </span>
                  )}
                </div>
              </div>
            )}
            <div ref={messagesEndRef} />
          </>
        )}
      </div>

      {/* Input */}
      <div className="input-container">
        <div className="input-wrapper">
          <textarea
            className="message-input"
            placeholder="Ask ForgeCode anything..."
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            disabled={chatState.isStreaming}
            rows={1}
          />
          {chatState.isStreaming || chatState.isLoading || chatState.currentTurnId ? (
            <button
              className="cancel-button"
              onClick={cancelMessage}
              title="Cancel current operation"
            >
              <span className="codicon codicon-close"></span>
              <span>Cancel</span>
            </button>
          ) : (
            <button
              className="send-button"
              onClick={handleSend}
              disabled={!input.trim()}
            >
              <span className="codicon codicon-send"></span>
              <span>Send</span>
            </button>
          )}
        </div>
        <div className="input-footer">
          <span>Press Ctrl+Enter to send</span>
          <span>{input.length} characters</span>
        </div>
      </div>
    </div>
  );
}

export default App;
