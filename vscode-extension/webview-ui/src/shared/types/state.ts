import { Option } from "effect";
import { Conversation } from "@domain/models";

/// AppState represents the complete application state
export interface AppState {
  readonly conversations: ReadonlyArray<Conversation>;
  readonly activeConversationId: Option.Option<string>;
  readonly streamingState: StreamingState;
  readonly ui: UIState;
}

/// StreamingState represents the state of streaming responses
export interface StreamingState {
  readonly isStreaming: boolean;
  readonly currentDelta: string;
}

/// UIState represents the UI-specific state
export interface UIState {
  readonly sidebarOpen: boolean;
  readonly settingsOpen: boolean;
  readonly theme: "light" | "dark";
}
