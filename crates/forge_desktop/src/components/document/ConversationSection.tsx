import React, { useRef, useMemo } from "react";
import { SpecialContent } from "./SpecialContent";
import { CodeSection } from "./CodeSection";
import { Separator } from "@/components/ui/separator";
import FileTag from "../FileTag";

interface ConversationSectionProps {
  userMessage: {
    id: string;
    content: string;
    timestamp: Date;
  } | null;
  responseMessages: {
    id: string;
    content: string;
    timestamp: Date;
    isShowUserMessage?: boolean;
  }[];
  toolCalls?: {
    id: string;
    name: string;
    content: string;
    isError: boolean;
    result?: string;
  }[];
  isLatest: boolean;
}

const ConversationSection: React.FC<ConversationSectionProps> = ({
  userMessage,
  responseMessages,
  isLatest,
}) => {
  const sectionRef = useRef<HTMLDivElement>(null);

  // Process special tags in response content
  const processedContent = useMemo(() => {
    // Check if we have any show_user messages
    const hasShowUserMessages = responseMessages.some(
      (msg) => msg.isShowUserMessage,
    );

    // If we have show_user messages, only use those and filter out regular system messages
    const messagesToUse = hasShowUserMessages
      ? responseMessages.filter((msg) => msg.isShowUserMessage)
      : responseMessages;

    // Combine all relevant messages
    const combinedContent = messagesToUse.map((msg) => msg.content).join("");

    return combinedContent;
  }, [responseMessages]);

  // Extract and process special tags
  const { mainContent, specialBlocks } = useMemo(() => {
    const blocks: { type: string; content: string }[] = [];
    let content = processedContent;

    // Extract special tag content
    const tagPatterns = [
      { tag: "analysis", pattern: /<analysis>([\s\S]*?)<\/analysis>/g },
      { tag: "thinking", pattern: /<thinking>([\s\S]*?)<\/thinking>/g },
      {
        tag: "action_plan",
        pattern: /<action_plan>([\s\S]*?)<\/action_plan>/g,
      },
      { tag: "execution", pattern: /<execution>([\s\S]*?)<\/execution>/g },
      {
        tag: "verification",
        pattern: /<verification>([\s\S]*?)<\/verification>/g,
      },
    ];

    // Extract and remove special blocks
    tagPatterns.forEach(({ tag, pattern }) => {
      content = content.replace(pattern, (_, p1) => {
        blocks.push({ type: tag, content: p1 });
        return ""; // Remove the tag content from the main text
      });
    });

    // Clean up extra newlines
    content = content.replace(/\n{3,}/g, "\n\n");

    return {
      mainContent: content,
      specialBlocks: blocks,
    };
  }, [processedContent]);

  // Function to render user content with file tags
  const renderUserContent = (content: string): React.ReactNode => {
    // Regex to find file path tags
    const fileTagRegex = /@\[(.*?)\]/g;
    const parts: React.ReactNode[] = [];
    let lastIndex = 0;
    let match;

    // Reset regex index
    fileTagRegex.lastIndex = 0;

    // Find all file tags
    while ((match = fileTagRegex.exec(content)) !== null) {
      // Add text before the file tag
      if (match.index > lastIndex) {
        parts.push(
          <span key={`text-${lastIndex}`}>
            {content.substring(lastIndex, match.index)}
          </span>,
        );
      }

      // Add the file tag component
      parts.push(
        <FileTag
          key={`tag-${match.index}`}
          filePath={match[1]}
          onRemove={() => {}}
          readOnly={true}
          inline={true}
          copyFormat="tag"
        />,
      );

      lastIndex = match.index + match[0].length;
    }

    // Add any remaining text
    if (lastIndex < content.length) {
      parts.push(<span key={`text-end`}>{content.substring(lastIndex)}</span>);
    }

    return parts.length > 0 ? <>{parts}</> : content;
  };
  if (!userMessage && responseMessages.length === 0) {
    return null;
  }

  return (
    <div
      ref={sectionRef}
      className="document-section mb-8 animate-in fade-in duration-300"
    >
      {/* User query section */}
      {userMessage && (
        <div className="user-query mb-4">
          <h3 className="text-base font-semibold text-primary mb-1">
            User Query
          </h3>
          <div className="user-content pl-1 whitespace-pre-wrap">
            {renderUserContent(userMessage.content)}
          </div>
        </div>
      )}

      {/* Response section */}
      {responseMessages.length > 0 && (
        <div className="ai-response mb-2">
          <h3 className="text-base font-semibold text-secondary mb-2">
            Response
          </h3>

          {/* Main response content */}
          <div className="response-content pl-1">
            <CodeSection content={mainContent} />
          </div>

          {/* Special content blocks */}
          {specialBlocks.map((block, idx) => (
            <SpecialContent
              key={`${block.type}-${idx}`}
              type={block.type}
              content={block.content}
            />
          ))}
        </div>
      )}

      {!isLatest && <Separator className="mt-8 mb-4" />}
    </div>
  );
};

export default ConversationSection;
