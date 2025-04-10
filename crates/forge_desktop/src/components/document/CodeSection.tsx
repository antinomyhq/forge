import React from "react";
import { CodeBlock } from "./CodeBlock";
import { extractCodeBlocks, extractFilePath } from "@/utils/utils";
import { FilePathText } from "@/utils/filePathUtils";
import ReactMarkdown from "react-markdown";
import FileTag from "../FileTag";
import { useProjectStore } from "@/stores/ProjectStore";

interface CodeSectionProps {
  content: string;
}

// Regex to match file tags (e.g., @[/path/to/file.txt])
const FILE_TAG_REGEX = /@\[(.*?)\]/g;

// Function to render content with both file tags and clickable file paths
const renderContentWithFileTagsAndPaths = (text: string): React.ReactNode => {
  const { currentProject } = useProjectStore.getState();
  const baseDir = currentProject?.path || "";

  // Use the same regex for file tags
  const fileTagRegex = FILE_TAG_REGEX;
  const parts: React.ReactNode[] = [];
  let lastIndex = 0;
  let match;

  // Need to reset the lastIndex property of the regex
  fileTagRegex.lastIndex = 0;

  // Find all file tags in the content
  while ((match = fileTagRegex.exec(text)) !== null) {
    // Add any text before this tag as ReactMarkdown with clickable file paths
    if (match.index > lastIndex) {
      const textBefore = text.substring(lastIndex, match.index);
      if (textBefore.trim()) {
        parts.push(
          <div
            key={`text-${lastIndex}`}
            className="prose prose-sm dark:prose-invert max-w-none"
          >
            <ReactMarkdown
              components={{
                // Custom renderer for paragraphs to inject file path detection
                p: ({ children }) => {
                  if (typeof children === "string") {
                    return (
                      <FilePathText
                        text={children as string}
                        options={{ baseDir }}
                      />
                    );
                  }
                  return <p>{children}</p>;
                },
              }}
            >
              {textBefore}
            </ReactMarkdown>
          </div>,
        );
      }
    }

    // Add the file tag as a FileTag component
    parts.push(
      <span key={`tag-${match.index}`} className="inline-block mx-1 my-1">
        <FileTag
          filePath={match[1]}
          onRemove={() => {}}
          inline={true}
          readOnly={true}
          copyFormat="tag"
        />
      </span>,
    );

    lastIndex = match.index + match[0].length;
  }

  // Add any remaining text after the last tag
  if (lastIndex < text.length) {
    const textAfter = text.substring(lastIndex);
    if (textAfter.trim()) {
      parts.push(
        <div
          key="text-end"
          className="prose prose-sm dark:prose-invert max-w-none"
        >
          <ReactMarkdown
            components={{
              // Custom renderer for paragraphs to inject file path detection
              p: ({ children }) => {
                if (typeof children === "string") {
                  return (
                    <FilePathText
                      text={children as string}
                      options={{ baseDir }}
                    />
                  );
                }
                return <p>{children}</p>;
              },
            }}
          >
            {textAfter}
          </ReactMarkdown>
        </div>,
      );
    }
  }

  // If no file tags were found, render the whole text as markdown with clickable file paths
  if (parts.length === 0) {
    return (
      <div className="prose prose-sm dark:prose-invert max-w-none">
        <ReactMarkdown
          components={{
            // Custom renderer for paragraphs to inject file path detection
            p: ({ children }) => {
              if (typeof children === "string") {
                return (
                  <FilePathText
                    text={children as string}
                    options={{ baseDir }}
                  />
                );
              }
              return <p>{children}</p>;
            },
          }}
        >
          {text}
        </ReactMarkdown>
      </div>
    );
  }

  return <>{parts}</>;
};

export const CodeSection: React.FC<CodeSectionProps> = ({ content }) => {
  // Extract code blocks from the content
  const codeBlocks = extractCodeBlocks(content);

  if (codeBlocks.length === 0) {
    // If no code blocks found, render content as markdown with file tag handling and clickable file paths
    return (
      <div className="prose prose-sm dark:prose-invert max-w-none">
        {renderContentWithFileTagsAndPaths(content)}
      </div>
    );
  }

  // Function to render content with code blocks highlighted
  const renderContentWithCodeBlocks = () => {
    // Replace code blocks with placeholders for later replacement
    let processedContent = content;
    const placeholders: { placeholder: string; component: React.ReactNode }[] =
      [];

    codeBlocks.forEach((block, index) => {
      const blockPattern = new RegExp(
        "```" +
          block.language +
          "\\n" +
          block.code.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") +
          "```",
        "g",
      );
      const placeholder = `__CODE_BLOCK_${index}__`;
      processedContent = processedContent.replace(blockPattern, placeholder);

      // Extract file path from any header above the code block
      const contentBeforeBlock = content.split("```" + block.language)[index];
      let filePath: string | null = null;

      if (contentBeforeBlock) {
        // Look for file path in the last 3 lines before the code block
        const lines = contentBeforeBlock.split("\n");
        const lastLines = lines.slice(-3).join("\n");
        filePath = extractFilePath(lastLines);
      }

      placeholders.push({
        placeholder,
        component: (
          <div key={index} className="my-4">
            <CodeBlock
              code={block.code}
              language={block.language}
              fileName={filePath}
            />
          </div>
        ),
      });
    });

    // Split content by placeholders and reconstruct with actual components
    const parts = processedContent.split(/(\_\_CODE\_BLOCK\_\d+\_\_)/g);
    return parts.map((part, i) => {
      // Check if this part is a placeholder
      const placeholderMatch = part.match(/\_\_CODE\_BLOCK\_(\d+)\_\_/);
      if (placeholderMatch) {
        const index = parseInt(placeholderMatch[1], 10);
        return placeholders[index].component;
      }
      // Otherwise, render as markdown with file tag handling and clickable file paths
      return part ? (
        <div key={i} className="prose prose-sm dark:prose-invert max-w-none">
          {renderContentWithFileTagsAndPaths(part)}
        </div>
      ) : null;
    });
  };

  return <>{renderContentWithCodeBlocks()}</>;
};
