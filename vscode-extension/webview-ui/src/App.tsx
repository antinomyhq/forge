import { useEffect } from 'react';
import { Effect, Runtime, Stream, Fiber } from 'effect';
import { JsonRpcService } from '@/infrastructure/rpc/JsonRpcService';
import { useChatState } from './presentation/hooks/useChatState';
import { useChatStateUpdater } from './presentation/hooks/useChatStateUpdater';
import { useChatActions } from './presentation/hooks/useChatActions';
import { useRuntime } from './presentation/hooks/useRuntime';
import { ChatLayout } from './presentation/components/layout/ChatLayout';
import { ChatHeader } from './presentation/components/header/ChatHeader';
import { WelcomeScreen } from './presentation/components/layout/WelcomeScreen';
import { MessageList } from './presentation/components/chat/MessageList';
import { InputBox } from './presentation/components/chat/InputBox';
import { StreamingIndicator } from './presentation/components/chat/StreamingIndicator';
import { EffectErrorBoundary } from './presentation/components/error/EffectErrorBoundary';
import { initVSCodeTheme, watchVSCodeTheme } from './lib/vscode-theme';
import './index.css';

function App() {
  console.log('[App] ===== FORGE WEBVIEW LOADED (v2 - JSON-RPC ONLY) =====');
  const runtime = useRuntime();
  const chatState = useChatState();
  const { updateFromMessage } = useChatStateUpdater();
  const { sendMessage, changeModel, changeAgent, initialize, cancelMessage } = useChatActions();

  // Initialize VSCode theme
  useEffect(() => {
    console.log('[App] Initializing VSCode theme...');
    initVSCodeTheme();
    watchVSCodeTheme();
  }, []);

  // Initialize Effect-TS and listen for messages using Effect streams
  useEffect(() => {
    console.log('[App] Initializing with Effect-TS streams...');

    // Subscribe to message stream from JsonRpcService FIRST, then initialize
    const program = Effect.gen(function* () {
      const rpc = yield* JsonRpcService;
      
      console.log('[App] Subscribing to JsonRpcService.notifications stream');
      
      // Subscribe to all notifications and update state
      yield* Effect.forkDaemon(
        Stream.runForEach(rpc.notifications, (message) =>
          Effect.sync(() => {
            console.log('[App] Received message via stream:', message);
            updateFromMessage(message);
          })
        )
      );
      
      // NOW send ready + request models/agents (after listener is set up)
      console.log('[App] JsonRpcService ready, sending initialization messages...');
      yield* Effect.sync(() => initialize());
    });

    // Use runFork for async operations, returns RuntimeFiber
    const runningFiber = Runtime.runFork(runtime)(program);
    console.log('[App] Message stream subscription active');

    return () => {
      console.log('[App] Cleaning up message stream subscription');
      // Interrupt the running fiber asynchronously (fire and forget)
      Runtime.runPromise(runtime)(Fiber.interrupt(runningFiber)).catch(console.error);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [runtime]); // Only depend on runtime - initialize and updateFromMessage are stable

  // Handle quick action from welcome screen
  const handleQuickAction = (action: string) => {
    sendMessage(action);
  };

  // Handle new conversation
  const handleNewConversation = () => {
    // TODO: Implement new conversation logic
    console.log('[App] New conversation requested');
  };

  const isEmpty = chatState.messages.length === 0 && !chatState.isStreaming && !chatState.isLoading;

  return (
    <EffectErrorBoundary
      fallback={(error, reset) => (
        <div style={{ padding: '20px', color: 'var(--vscode-errorForeground)' }}>
          <h3>Application Error</h3>
          <p>{error.message}</p>
          <button onClick={reset}>Try Again</button>
          <button onClick={() => window.location.reload()}>Reload</button>
        </div>
      )}
    >
      <ChatLayout
      header={
        <ChatHeader
          agentName={chatState.agentName}
          tokenCount={chatState.tokenCount}
          cost={chatState.cost}
          onNewConversation={handleNewConversation}
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
    </EffectErrorBoundary>
  );
}

export default App;