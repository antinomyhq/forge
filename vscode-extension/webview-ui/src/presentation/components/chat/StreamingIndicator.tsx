import React from "react";

interface StreamingIndicatorProps {
  delta: string;
}

/// StreamingIndicator displays the current streaming content
export const StreamingIndicator: React.FC<StreamingIndicatorProps> = ({ delta }) => {
  return (
    <div className="px-4 py-2 bg-gray-800 border-t border-gray-700">
      <div className="max-w-3xl">
        <div className="text-xs text-gray-400 mb-1">Assistant is typing...</div>
        <div className="text-sm text-gray-200 whitespace-pre-wrap">{delta}</div>
      </div>
    </div>
  );
};
