import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";

// State related to file tagging
export interface TaggedFile {
  path: string;
}

// Event type constants, matching those in Rust commands.rs
const EVENT_TITLE = "title";
// Constants for tool names that should not be displayed in UI
const TOOL_EVENT_DISPATCH = "tool_forge_event_dispatch";

// Type definitions
export interface Message {
  id: string;
  content: string;
  sender: "user" | "system";
  timestamp: Date;
  isShowUserMessage?: boolean;
}

export interface ToolCall {
  id: string;
  name: string;
  content: string;
  isError: boolean;
  result?: string;
}

export interface ModeInfo {
  mode: string;
  description: string;
}

export interface ConversationInfo {
  id: string;
  title: string | null;
}

// State interface definition
interface ForgeState {
  // State
  messages: Message[];
  toolCalls: ToolCall[];
  isLoading: boolean;
  error: string | null;
  conversationId: string | null;
  conversationTitle: string | null;
  mode: ModeInfo;
  isFirstMessage: boolean;
  debugMode: boolean;
  listenersInitialized: boolean;
  taggedFiles: string[]; // New state for file tagging

  // File tagging actions
  addTaggedFile: (filePath: string) => void;
  removeTaggedFile: (index: number) => void;
  setTaggedFiles: (files: string[]) => void; // New action to set all tagged files
  clearTaggedFiles: () => void;

  // Actions
  setMessages: (messages: Message[]) => void;
  addMessage: (message: Message) => void;
  addToolCall: (toolCall: ToolCall) => void;
  updateToolCall: (id: string, updates: Partial<ToolCall>) => void;
  removeToolCall: (id: string) => void;
  setLoading: (isLoading: boolean) => void;
  setError: (error: string | null) => void;
  setConversationId: (id: string | null) => void;
  setConversationTitle: (title: string | null) => void;
  setMode: (mode: ModeInfo) => void;
  setIsFirstMessage: (isFirst: boolean) => void;
  resetState: () => void;

  // Event handling
  setupListeners: () => Promise<() => void>;

  // API operations
  sendMessage: (content: string) => Promise<void>;
  cancelStream: () => Promise<void>;
  changeMode: (mode: "Act" | "Plan" | "Help") => Promise<void>;
  newConversation: () => Promise<void>;
  exportConversation: (options?: {
    path?: string;
    title?: string;
  }) => Promise<string>;
}

// Create the Zustand store with immer middleware for immutable updates
export const useForgeStore = create<ForgeState>()(
  immer((set, get) => ({
    // Initial state
    messages: [],
    toolCalls: [],
    isLoading: false,
    error: null,
    conversationId: null,
    conversationTitle: null,
    mode: {
      mode: "Act",
      description: "mode - executes commands and makes file changes",
    },
    isFirstMessage: true,
    debugMode: false,
    listenersInitialized: false, // Initial state for file tagging
    taggedFiles: [],

    // File tagging actions
    addTaggedFile: (filePath) =>
      set((state) => {
        // Check if the file is already tagged
        if (!state.taggedFiles.includes(filePath)) {
          state.taggedFiles.push(filePath);
        }
      }),

    removeTaggedFile: (index) =>
      set((state) => {
        state.taggedFiles = state.taggedFiles.filter((_, i) => i !== index);
      }),

    setTaggedFiles: (files) =>
      set((state) => {
        state.taggedFiles = files;
      }),

    clearTaggedFiles: () =>
      set((state) => {
        state.taggedFiles = [];
      }),

    // Event handling
    setupListeners: async () => {
      // Only set up listeners once
      if (get().listenersInitialized) {
        return () => {}; // Return no-op cleanup if already initialized
      }

      const unlistenFunctions: UnlistenFn[] = [];

      // Listen for agent messages
      const messageListener = await listen<any>("agent-message", (event) => {
        try {
          const { debugMode } = get();
          const agentMessage = event.payload;

          if (agentMessage?.agent && agentMessage?.message) {
            const message = agentMessage.message;

            // Process tool call starts
            if (message.toolCallStart) {
              const toolCall = message.toolCallStart;

              // Skip event dispatch tool from UI
              if (toolCall.name === TOOL_EVENT_DISPATCH) {
                return;
              }

              // Create a tool call record
              const newToolCall = {
                id: toolCall.call_id || `tool-${Date.now()}`,
                name: toolCall.name,
                content: JSON.stringify(toolCall.arguments || {}, null, 2),
                isError: false,
              };

              // Add to tool calls
              set((state) => {
                state.toolCalls.push(newToolCall);
              });
            }
            // Process tool call ends
            else if (message.toolCallEnd) {
              const toolEnd = message.toolCallEnd;

              // Skip event dispatch tool from UI
              if (toolEnd.name === TOOL_EVENT_DISPATCH) {
                // Remove from tool calls
                set((state) => {
                  state.toolCalls = state.toolCalls.filter(
                    (tool) => tool.id !== toolEnd.call_id,
                  );
                });
                return;
              }

              // Update the existing tool call
              set((state) => {
                state.toolCalls = state.toolCalls.map((tool) =>
                  tool.id === toolEnd.call_id
                    ? {
                        ...tool,
                        isError: toolEnd.is_error || false,
                        result: toolEnd.content,
                      }
                    : tool,
                );
              });
            }
            // Process text messages
            else if (message.text !== undefined) {
              // Only process non-empty text messages
              if (message.text && message.text) {
                const content = message.text;

                // Simply add the text message directly
                set((state) => {
                  state.messages.push({
                    id: `system-${Date.now()}`,
                    content,
                    sender: "system",
                    timestamp: new Date(),
                  });
                });

                if (debugMode) console.log("New message added");
              }
            }
            // Process event messages
            else if (message.event) {
              // Handle title event
              if (message.event.name === EVENT_TITLE) {
                // Update conversation title
                set((state) => {
                  state.conversationTitle = message.event.value.toString();
                });
              }
            }
          }
        } catch (err) {
          set((state) => {
            state.error =
              err instanceof Error ? err.message : "Error processing message";
          });
        }
      });

      // Listen for agent errors
      const errorListener = await listen<string>("agent-error", (event) => {
        const errorMessage = event.payload;
        set((state) => {
          state.error = errorMessage;
          state.isLoading = false;
        });
      });

      // Listen for stream completion
      const completeListener = await listen("agent-stream-complete", () => {
        set((state) => {
          state.isLoading = false;
        });
      });

      unlistenFunctions.push(messageListener);
      unlistenFunctions.push(errorListener);
      unlistenFunctions.push(completeListener);

      // Mark listeners as initialized
      set((state) => {
        state.listenersInitialized = true;
      });

      // Return cleanup function
      return () => {
        unlistenFunctions.forEach((fn) => fn());
        set((state) => {
          state.listenersInitialized = false;
        });
      };
    },

    // State update actions
    setMessages: (messages) =>
      set((state) => {
        state.messages = messages;
      }),

    addMessage: (message) =>
      set((state) => {
        state.messages.push(message);
      }),

    addToolCall: (toolCall) =>
      set((state) => {
        state.toolCalls.push(toolCall);
      }),

    updateToolCall: (id, updates) =>
      set((state) => {
        state.toolCalls = state.toolCalls.map((tool) =>
          tool.id === id ? { ...tool, ...updates } : tool,
        );
      }),

    removeToolCall: (id) =>
      set((state) => {
        state.toolCalls = state.toolCalls.filter((tool) => tool.id !== id);
      }),

    setLoading: (isLoading) =>
      set((state) => {
        state.isLoading = isLoading;
      }),

    setError: (error) =>
      set((state) => {
        state.error = error;
      }),

    setConversationId: (id) =>
      set((state) => {
        state.conversationId = id;
      }),

    setConversationTitle: (title) =>
      set((state) => {
        state.conversationTitle = title;
      }),

    setMode: (mode) =>
      set((state) => {
        state.mode = mode;
      }),

    setIsFirstMessage: (isFirst) =>
      set((state) => {
        state.isFirstMessage = isFirst;
      }),

    resetState: () =>
      set((state) => {
        state.messages = [];
        state.toolCalls = [];
        state.error = null;
        state.conversationTitle = null;
        state.isFirstMessage = true;
      }),

    // API operations
    sendMessage: async (content) => {
      try {
        if (!content.trim()) return;

        const { conversationId, isFirstMessage, debugMode } = get();

        // If no conversation exists, create one first
        if (!conversationId) {
          await get().newConversation();
        }

        set((state) => {
          state.isLoading = true;
          state.error = null;
        });

        // Add user message to the state
        set((state) => {
          state.messages.push({
            id: `user-${Date.now()}`,
            content,
            sender: "user",
            timestamp: new Date(),
          });
        });

        if (debugMode)
          console.log("Sending message:", content, "is_first:", isFirstMessage);

        // Send the message with file tags in their correct positions
        await invoke("send_message", {
          options: {
            content, // Use the content as-is with file tags in place
            is_first: isFirstMessage,
          },
        });

        // Clear tagged files after successfully sending the message
        get().clearTaggedFiles();

        // Update first message flag
        if (isFirstMessage) {
          set((state) => {
            state.isFirstMessage = false;
          });
        }
      } catch (err) {
        set((state) => {
          state.error = err instanceof Error ? err.message : String(err);
          state.isLoading = false;
        });
        if (get().debugMode) console.error("Error sending message:", err);
      }
    },

    cancelStream: async () => {
      try {
        // Call the backend to cancel the stream
        await invoke("cancel_stream");

        // The isLoading state will be updated via the agent-stream-complete event
        if (get().debugMode) console.log("Stream cancellation requested");

        // Add a system message indicating cancellation
        set((state) => {
          state.messages.push({
            id: `system-cancel-${Date.now()}`,
            content: "*Conversation cancelled by user*",
            sender: "system",
            timestamp: new Date(),
          });
        });
      } catch (err) {
        set((state) => {
          state.error = err instanceof Error ? err.message : String(err);
        });
        if (get().debugMode) console.error("Error canceling stream:", err);
      }
    },

    changeMode: async (newMode) => {
      try {
        set((state) => {
          state.isLoading = true;
        });

        if (get().debugMode) console.log("Changing mode to:", newMode);

        // Call the backend to change mode
        const updatedMode = await invoke<ModeInfo>("change_mode", {
          mode: newMode,
        });

        set((state) => {
          state.mode = updatedMode;
          state.isLoading = false;
        });
      } catch (err) {
        set((state) => {
          state.error = err instanceof Error ? err.message : String(err);
          state.isLoading = false;
        });
        if (get().debugMode) console.error("Error changing mode:", err);
      }
    },

    newConversation: async () => {
      try {
        set((state) => {
          state.isLoading = true;
        });

        get().resetState();

        // Load the workflow and initialize a new conversation
        await invoke("load_workflow", { path: null });
        const id = await invoke<string>("init_conversation");

        set((state) => {
          state.conversationId = id;
        });

        // Get the current mode
        const currentMode = await invoke<ModeInfo>("get_mode");

        set((state) => {
          state.mode = currentMode;
          state.isLoading = false;
        });

        // Get conversation info including title
        try {
          const info = await invoke<ConversationInfo>("get_conversation_info");
          if (info && info.title) {
            set((state) => {
              state.conversationTitle = info.title;
            });
            if (get().debugMode)
              console.log("Loaded conversation title:", info.title);
          }
        } catch (err) {
          // Non-critical error, just log it in debug mode
          if (get().debugMode)
            console.error("Error getting conversation info:", err);
        }

        if (get().debugMode)
          console.log("New conversation initialized with ID:", id);
      } catch (err) {
        set((state) => {
          state.error = err instanceof Error ? err.message : String(err);
          state.isLoading = false;
        });
        if (get().debugMode)
          console.error("Error initializing conversation:", err);
      }
    },

    exportConversation: async (options) => {
      try {
        const { conversationId, debugMode } = get();

        if (!conversationId) {
          throw new Error("No active conversation to export");
        }

        if (debugMode)
          console.log("Exporting conversation:", conversationId, options);

        const path = await invoke<string>("export_conversation", { options });
        return path;
      } catch (err) {
        set((state) => {
          state.error = err instanceof Error ? err.message : String(err);
        });
        if (get().debugMode)
          console.error("Error exporting conversation:", err);
        throw err;
      }
    },
  })),
);

// Initialize listeners when this module is first imported
// This ensures we're ready to receive events as soon as possible
useForgeStore.getState().setupListeners();
