import React from "react";
import { Message } from "@domain/models";

interface MessageItemProps {
  message: Message;
}

/// MessageItem displays a single message
export const MessageItem: React.FC<MessageItemProps> = ({ message }) => {
  const isUser = message.role === "user";

  return (
    <div className={`flex ${isUser ? "justify-end" : "justify-start"}`}>
      <div
        className={`max-w-3xl rounded-lg p-4 ${
          isUser ? "bg-blue-600 text-white" : "bg-gray-700 text-gray-100"
        }`}
      >
        <div className="text-sm font-semibold mb-1">
          {message.role === "user" ? "You" : "Assistant"}
        </div>
        <div className="whitespace-pre-wrap">{message.content}</div>
        {message.status === "pending" && (
          <div className="text-xs mt-2 opacity-70">Sending...</div>
        )}
        {message.status === "failed" && (
          <div className="text-xs mt-2 text-red-300">Failed to send</div>
        )}
      </div>
    </div>
  );
};
