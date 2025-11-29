import { Data } from "effect";
import { Conversation, Message } from "@domain/models";

/// StateAction represents all possible state mutations
export type StateAction = Data.TaggedEnum<{
  ConversationAdded: { conversation: Conversation };
  ConversationRemoved: { conversationId: string };
  MessageAdded: { conversationId: string; message: Message };
  ActiveConversationChanged: { id: string };
  StreamDeltaReceived: { delta: string };
  StreamEnded: {};
  SidebarToggled: {};
  SettingsToggled: {};
  ThemeChanged: { theme: "light" | "dark" };
}>;

export const StateAction = Data.taggedEnum<StateAction>();
