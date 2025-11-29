import React from "react";
import { Message } from "@domain/models";
import { MessageItem } from "./MessageItem";

interface MessageListProps {
  messages: ReadonlyArray<Message>;
}

/// MessageList displays a list of messages in a conversation
export const MessageList: React.FC<MessageListProps> = ({ messages }) => {
  const scrollRef = React.useRef<HTMLDivElement>(null);

  React.useEffect(() => {
    scrollRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  return (
    <div className="flex-1 overflow-y-auto p-4 space-y-4">
      {messages.map((message) => (
        <MessageItem key={message.id.value} message={message} />
      ))}
      <div ref={scrollRef} />
    </div>
  );
};
