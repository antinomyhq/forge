import React from "react";
import { Option } from "effect";
import { Conversation } from "@domain/models";
import { ConversationItem } from "./ConversationItem";

interface ConversationListProps {
  conversations: ReadonlyArray<Conversation>;
  activeId: Option.Option<string>;
  onSelect: (id: string) => void;
}

/// ConversationList displays a list of conversations
export const ConversationList: React.FC<ConversationListProps> = ({
  conversations,
  activeId,
  onSelect,
}) => {
  return (
    <div className="flex-1 overflow-y-auto">
      {conversations.length === 0 ? (
        <div className="p-4 text-center text-gray-500 text-sm">No conversations yet</div>
      ) : (
        conversations.map((conversation) => (
          <ConversationItem
            key={conversation.id.value}
            conversation={conversation}
            isActive={Option.getOrNull(activeId) === conversation.id.value}
            onSelect={() => onSelect(conversation.id.value)}
          />
        ))
      )}
    </div>
  );
};
