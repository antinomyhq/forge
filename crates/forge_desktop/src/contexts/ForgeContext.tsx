// This context provides the Forge API state and functionality to React components

// Event type constants, matching those in Rust commands.rs
const EVENT_TITLE = 'title';
// Constants for tool names that should not be displayed in UI
const TOOL_EVENT_DISPATCH = 'tool_forge_event_dispatch';
const TOOL_SHOW_USER = 'tool_forge_display_show_user';


import React, { createContext, useContext, useState, useEffect, useCallback, ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

// Type definitions for our context
interface Message {
  id: string;
  content: string;
  sender: 'user' | 'system';
  timestamp: Date;
  isShowUserMessage?: boolean; // Flag to identify messages from show_user tool
}

interface ToolCall {
  id: string;
  name: string;
  content: string;
  isError: boolean;
  result?: string;
}

interface ForgeContextType {
  messages: Message[];
  setMessages: React.Dispatch<React.SetStateAction<Message[]>>;
  toolCalls: ToolCall[];
  isLoading: boolean;
  error: string | null;
  conversationId: string | null;
  conversationTitle: string | null;
  mode: { mode: string; description: string };
  sendMessage: (content: string) => Promise<void>;
  changeMode: (mode: 'Act' | 'Plan' | 'Help') => Promise<void>;
  newConversation: () => Promise<void>;
  exportConversation: (options?: { path?: string; title?: string }) => Promise<string>;
}

// Create the context with default values
const ForgeContext = createContext<ForgeContextType>({
  messages: [],
  setMessages: () => {},
  toolCalls: [],
  isLoading: false,
  error: null,
  conversationId: null,
  conversationTitle: null,
  mode: { mode: 'Act', description: 'mode - executes commands and makes file changes' },
  sendMessage: async () => {},
  changeMode: async () => {},
  newConversation: async () => {},
  exportConversation: async () => '',
});

// Context provider component
export const ForgeProvider: React.FC<{ children: ReactNode }> = ({ children }) => {
  const [messages, setMessages] = useState<Message[]>([]);
  const [toolCalls, setToolCalls] = useState<ToolCall[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [conversationId, setConversationId] = useState<string | null>(null);
  const [conversationTitle, setConversationTitle] = useState<string | null>(null);
  const [mode, setMode] = useState<{ mode: string; description: string }>({
    mode: 'Act',
    description: 'mode - executes commands and makes file changes',
  });
  const [isFirstMessage, setIsFirstMessage] = useState(true);
  const [debugMode] = useState<boolean>(false); // Set to true only when debugging is needed
  const [currentSystemMessageId, setCurrentSystemMessageId] = useState<string | null>(null); // Track current system message for aggregation

  // Set up event listeners for agent messages
  useEffect(() => {
    const unlistenFns: (() => void)[] = [];
    
    async function setupListeners() {
      // Listen for agent messages
      const unlisten1 = await listen<any>('agent-message', (event) => {
        try {
          const agentMessage = event.payload;
          
          if (agentMessage?.agent && agentMessage?.message) {
            const message = agentMessage.message;
            
            // Process text messages
            if (message.text !== undefined) {
              // Only process non-empty text messages
              if (message.text && message.text.trim()) {
                const content = message.text.trim();
                
                // Reset the current system message ID when a tool call happens or on first message
                if (toolCalls.length > 0 && currentSystemMessageId === null) {
                  // Create a new system message after tool calls
                  const newId = `system-${Date.now()}`;
                  setCurrentSystemMessageId(newId);
                  setMessages(prev => [...prev, {
                    id: newId,
                    content,
                    sender: 'system',
                    timestamp: new Date(),
                    isShowUserMessage: false,
                  }]);
                  
                  if (debugMode) console.log('New message created after tool call');
                } else if (currentSystemMessageId) {
                  // Append to the existing message (aggregation)
                  setMessages(prev => {
                    return prev.map(msg => {
                      if (msg.id === currentSystemMessageId) {
                        // Append the new content to the existing message
                        return {
                          ...msg,
                          content: msg.content + (msg.content.endsWith('\n') ? '' : ' ') + content
                        };
                      }
                      return msg;
                    });
                  });
                  
                  if (debugMode) console.log('Aggregated content to existing message');
                } else {
                  // Create a new system message (first message or after user message)
                  const newId = `system-${Date.now()}`;
                  setCurrentSystemMessageId(newId);
                  setMessages(prev => [...prev, {
                    id: newId,
                    content,
                    sender: 'system',
                    timestamp: new Date(),
                    isShowUserMessage: false,
                  }]);
                  
                  if (debugMode) console.log('New message created');
                }
              }
            }
            
            // Process tool call starts
            else if (message.toolCallStart) {
              const toolCall = message.toolCallStart;
              
              // Create a tool call record
              const newToolCall = {
                id: toolCall.call_id || `tool-${Date.now()}`,
                name: toolCall.name,
                content: JSON.stringify(toolCall.arguments || {}, null, 2),
                isError: false,
              };
              
              // Special handling for show_user tool
              if (toolCall.name === TOOL_SHOW_USER) {
                if (toolCall.arguments && toolCall.arguments.content) {
                  const content = toolCall.arguments.content;
                  
                  // Add directly to messages
                  setMessages(prev => {
                    const trimmedContent = content.trim();
                    // Check if this message is a duplicate (has exactly the same content as the last message)
                    if (prev.length > 0) {
                      const lastMessage = prev[prev.length - 1];
                      if (lastMessage.content === trimmedContent && lastMessage.sender === 'system') {
                        // Skip duplicate messages
                        if (debugMode) console.log('Skipping duplicate show_user message');
                        return prev;
                      }
                    }
                    
                    // Reset the current system message ID since we're creating a show_user message
                    setCurrentSystemMessageId(null);
                    
                    return [...prev, {
                      id: `system-show-user-${Date.now()}`,
                      content: trimmedContent,
                      sender: 'system' as 'user' | 'system',
                      timestamp: new Date(),
                      isShowUserMessage: true, // Mark this as a show_user tool message
                    }];
                  });
                }
                
                // Don't add show_user tool to tool calls
                if (debugMode) console.log('Skipping show_user tool from UI display');
                return;
              }
              
              // Skip event dispatch tool from UI
              if (toolCall.name === TOOL_EVENT_DISPATCH) {
                // Don't add to tool calls
                if (debugMode) console.log('Skipping event dispatch tool from UI display');
                return;
              }
              
              // Add to tool calls
              setToolCalls(prev => {
                // Check if we already have this tool call with the same ID
                if (prev.some(tool => tool.id === newToolCall.id)) {
                  // Skip duplicate tool call
                  if (debugMode) console.log('Skipping duplicate tool call:', newToolCall.id);
                  return prev;
                }
                return [...prev, newToolCall];
              });
            }
            
            // Process tool call ends
            else if (message.toolCallEnd) {
              const toolEnd = message.toolCallEnd;
              
              // Skip hidden tool results from UI
              if (toolEnd.name === TOOL_EVENT_DISPATCH || toolEnd.name === TOOL_SHOW_USER) {
                // Remove the tool call from the list instead of updating it
                setToolCalls(prev => prev.filter(tool => tool.id !== toolEnd.call_id));
                if (debugMode) console.log(`Removing ${toolEnd.name} tool result from UI display`);
                return;
              }
              
              // Update the existing tool call
              setToolCalls(prev => prev.map(tool => 
                tool.id === toolEnd.call_id ? 
                  { ...tool, isError: toolEnd.is_error || false, result: toolEnd.content } :
                  tool
              ));
            }
            
            // Process usage info
            else if (message.usage) {
              // Store usage info if needed
            }
            
            // Process event messages
            else if (message.event) {
              // Handle title event
              if (message.event.name === EVENT_TITLE) {
                // Update conversation title
                setConversationTitle(message.event.value.toString());
                
                if (debugMode) console.log('Title updated:', message.event.value);
              }
            }
          }
        } catch (err) {
          setError(err instanceof Error ? err.message : 'Error processing message');
        }
      });
      
      // Listen for agent errors
      const unlisten2 = await listen('agent-error', (event) => {
        const errorMessage = event.payload as string;
        setError(errorMessage);
        setIsLoading(false);
      });
      
      // Listen for stream completion
      const unlisten3 = await listen('agent-stream-complete', () => {
        setIsLoading(false);
      });
      
      unlistenFns.push(unlisten1, unlisten2, unlisten3);
    }
    
    setupListeners();
    
    // Cleanup listeners on unmount
    return () => {
      unlistenFns.forEach(fn => fn());
    };
  }, [toolCalls, currentSystemMessageId, debugMode]);

  // Initialize a new conversation
  const newConversation = useCallback(async () => {
    try {
      setIsLoading(true);
      setMessages([]);
      setToolCalls([]);
      setError(null);
      setConversationTitle(null);
      setIsFirstMessage(true);
      setCurrentSystemMessageId(null); // Reset the current system message ID
      
      // Load the workflow and initialize a new conversation
      await invoke('load_workflow', { path: null });
      const id = await invoke<string>('init_conversation');
      setConversationId(id);
      
      // Get the current mode
      const currentMode = await invoke<{ mode: string; description: string }>('get_mode');
      setMode(currentMode);
      
      setIsLoading(false);      
      // Get conversation info including title
      try {
        const info = await invoke<{ id: string; title: string | null }>('get_conversation_info');
        if (info && info.title) {
          setConversationTitle(info.title);
          if (debugMode) console.log('Loaded conversation title:', info.title);
        }
      } catch (err) {
        // Non-critical error, just log it in debug mode
        if (debugMode) console.error('Error getting conversation info:', err);
      }
      
      if (debugMode) console.log('New conversation initialized with ID:', id);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setIsLoading(false);
      if (debugMode) console.error('Error initializing conversation:', err);
    }
  }, [debugMode]);

  // Send a message to the conversation
  const sendMessage = useCallback(async (content: string) => {
    try {
      if (!content.trim()) return;
      
      // If no conversation exists, create one first
      if (!conversationId) {
        await newConversation();
      }
      
      setIsLoading(true);
      setError(null);
      
      // Add user message to the state
      setMessages(prev => [...prev, {
        id: `user-${Date.now()}`,
        content,
        sender: 'user',
        timestamp: new Date(),
      }]);
      
      // Reset the current system message ID when user sends a message
      setCurrentSystemMessageId(null);
      
      if (debugMode) console.log('Sending message:', content, 'is_first:', isFirstMessage);
      
      // Send the message to the backend
      await invoke('send_message', {
        options: {
          content,
          is_first: isFirstMessage,
        }
      });
      
      // Update first message flag
      if (isFirstMessage) {
        setIsFirstMessage(false);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setIsLoading(false);
      if (debugMode) console.error('Error sending message:', err);
    }
  }, [conversationId, isFirstMessage, newConversation, debugMode]);

  // Change the current mode
  const changeMode = useCallback(async (newMode: 'Act' | 'Plan' | 'Help') => {
    try {
      setIsLoading(true);
      
      if (debugMode) console.log('Changing mode to:', newMode);
      
      // Call the backend to change mode
      const updatedMode = await invoke<{ mode: string; description: string }>('change_mode', { mode: newMode });
      setMode(updatedMode);
      
      setIsLoading(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setIsLoading(false);
      if (debugMode) console.error('Error changing mode:', err);
    }
  }, [debugMode]);

  // Export the conversation
  const exportConversation = useCallback(async (options?: { path?: string; title?: string }) => {
    try {
      if (!conversationId) {
        throw new Error("No active conversation to export");
      }
      
      if (debugMode) console.log('Exporting conversation:', conversationId, options);
      
      const path = await invoke<string>('export_conversation', { options });
      return path;
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      if (debugMode) console.error('Error exporting conversation:', err);
      throw err;
    }
  }, [conversationId, debugMode]);

  // Initialize a conversation on first load
  useEffect(() => {
    newConversation();
  }, [newConversation]);

  // The context value
  const contextValue: ForgeContextType = {
    messages,
    setMessages,
    toolCalls,
    isLoading,
    error,
    conversationId,
    conversationTitle,
    mode,
    sendMessage,
    changeMode,
    newConversation,
    exportConversation,
  };

  return (
    <ForgeContext.Provider value={contextValue}>
      {children}
    </ForgeContext.Provider>
  );
};

// Hook for using the Forge context
export const useForge = () => useContext(ForgeContext);