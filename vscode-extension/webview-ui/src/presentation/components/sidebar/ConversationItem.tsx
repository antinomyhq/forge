import React from "react";
import { Conversation } from "@domain/models";

interface ConversationItemProps {
  conversation: Conversation;
  isActive: boolean;
  onSelect: () => void;
}

/// ConversationItem displays a single conversation in the list
export const ConversationItem: React.FC<ConversationItemProps> = ({
  conversation,
  isActive,
  onSelect,
}) => {
  const title =
    conversation.messages.length > 0
      ? conversation.messages[0]!.content.slice(0, 50) + "..."
      : "New Conversation";

  return (
    <button
      onClick={onSelect}
      className={`w-full text-left px-4 py-3 border-b border-gray-800 hover:bg-gray-800 transition-colors ${
        isActive ? "bg-gray-800" : ""
      }`}
    >
      <div className="text-sm text-white truncate">{title}</div>
      <div className="text-xs text-gray-400 mt-1">
        {conversation.messages.length} messages
      </div>
    </button>
  );
};
