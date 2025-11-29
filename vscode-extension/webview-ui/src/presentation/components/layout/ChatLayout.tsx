import React from "react";

interface ChatLayoutProps {
  header: React.ReactNode;
  messages: React.ReactNode;
  input: React.ReactNode;
  isEmpty?: boolean;
  welcome?: React.ReactNode;
}

/// ChatLayout provides the main layout structure for the chat interface
export const ChatLayout: React.FC<ChatLayoutProps> = ({
  header,
  messages,
  input,
  isEmpty = false,
  welcome,
}) => {
  return (
    <div className="flex flex-col h-screen w-full bg-background">
      {/* Header */}
      <div className="shrink-0">
        {header}
      </div>

      {/* Messages or Welcome Screen */}
      <div className="flex-1 min-h-0 overflow-hidden">
        {isEmpty && welcome ? (
          <div className="h-full flex items-center justify-center">
            {welcome}
          </div>
        ) : (
          messages
        )}
      </div>

      {/* Input */}
      <div className="shrink-0">
        {input}
      </div>
    </div>
  );
};
