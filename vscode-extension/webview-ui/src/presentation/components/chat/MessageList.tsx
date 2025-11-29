import React from "react";
import { MessageItem } from "./MessageItem";
import { ToolCallCard } from "./ToolCallCard";
import { ScrollArea } from "@/components/ui/scroll-area";

// Simplified message type matching ChatState
export interface SimpleMessage {
  role?: 'user' | 'assistant';
  content?: string;
  timestamp: number;
  type?: 'tool';
  toolName?: string;
  args?: Record<string, any>;
  status?: 'running' | 'completed' | 'failed';
}

interface MessageListProps {
  messages: ReadonlyArray<SimpleMessage>;
}

/// MessageList displays a list of messages in a conversation using shadcn ScrollArea
export const MessageList: React.FC<MessageListProps> = ({ messages }) => {
  const scrollRef = React.useRef<HTMLDivElement>(null);

  React.useEffect(() => {
    scrollRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  return (
    <ScrollArea className="h-full w-full">
      <div className="space-y-2 p-4">
        {messages.map((message, idx) => {
          // Show ToolCallCard for tool messages
          if (message.type === 'tool' && message.toolName) {
            return (
              <ToolCallCard
                key={idx}
                toolCall={{
                  id: `tool-${idx}`,
                  name: message.toolName,
                  status: (message.status as any) || 'pending',
                  ...(message.args ? { arguments: message.args } : {}),
                }}
              />
            );
          }
          
          // Show MessageItem for regular messages
          return <MessageItem key={idx} message={message} />;
        })}
        <div ref={scrollRef} />
      </div>
    </ScrollArea>
  );
};
