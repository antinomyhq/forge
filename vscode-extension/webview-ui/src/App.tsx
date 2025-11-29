import { useEffect } from 'react';
import { useChatState, useChatStateUpdater } from './presentation/hooks/useChatStateSimple';
import { useChatActions } from './presentation/hooks/useChatActionsSimple';
import { ChatLayout } from './presentation/components/layout/ChatLayout';
import { ChatHeader } from './presentation/components/header/ChatHeader';
import { WelcomeScreen } from './presentation/components/layout/WelcomeScreen';
import { MessageList } from './presentation/components/chat/MessageList';
import { InputBox } from './presentation/components/chat/InputBox';
import { StreamingIndicator } from './presentation/components/chat/StreamingIndicator';
import { initVSCodeTheme, watchVSCodeTheme } from './lib/vscode-theme';
import './index.css';

function App() {
  const chatState = useChatState();
  const { updateFromMessage } = useChatStateUpdater();
  const { sendMessage, changeModel, changeAgent, initialize, cancelMessage } = useChatActions();

  // Initialize VSCode theme
  useEffect(() => {
    console.log('[App] Initializing VSCode theme...');
    initVSCodeTheme();
    watchVSCodeTheme();
  }, []);

  // Initialize Effect-TS and listen for messages
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

  // Handle quick action from welcome screen
  const handleQuickAction = (action: string) => {
    sendMessage(action);
  };

  const isEmpty = chatState.messages.length === 0 && !chatState.isStreaming && !chatState.isLoading;

  return (
    <ChatLayout
      header={
        <ChatHeader
          agentName={chatState.agentName}
          tokenCount={chatState.tokenCount}
          cost={chatState.cost}
        />
      }
      messages={
        <>
          <MessageList messages={chatState.messages} />
          {chatState.isStreaming && (
            <StreamingIndicator delta={chatState.streamingContent} />
          )}
        </>
      }
      input={
        <InputBox
          onSend={sendMessage}
          onCancel={cancelMessage}
          disabled={chatState.isLoading}
          isStreaming={chatState.isStreaming}
          models={chatState.models}
          agents={chatState.agents}
          selectedModelId={chatState.modelId}
          selectedModelName={chatState.modelName}
          selectedAgentId={chatState.agentId}
          selectedAgentName={chatState.agentName}
          onModelChange={changeModel}
          onAgentChange={changeAgent}
        />
      }
      isEmpty={isEmpty}
      welcome={
        <WelcomeScreen onQuickAction={handleQuickAction} />
      }
    />
  );
}

export default App;