import React from "react";
import ReactMarkdown from "react-markdown";
import { FilePathText } from "@/utils/filePathUtils";
import { useProjectStore } from "@/stores/ProjectStore";

interface SpecialContentProps {
  type: string;
  content: string;
}

// Map of special content types to their styling
const contentTypeStyles: Record<
  string,
  { title: string; bgColor: string; borderColor: string }
> = {
  analysis: {
    title: "Analysis",
    bgColor: "bg-blue-50 dark:bg-blue-950/30",
    borderColor: "border-blue-200 dark:border-blue-800",
  },
  thinking: {
    title: "Thinking",
    bgColor: "bg-purple-50 dark:bg-purple-950/30",
    borderColor: "border-purple-200 dark:border-purple-800",
  },
  action_plan: {
    title: "Action Plan",
    bgColor: "bg-green-50 dark:bg-green-950/30",
    borderColor: "border-green-200 dark:border-green-800",
  },
  execution: {
    title: "Execution",
    bgColor: "bg-amber-50 dark:bg-amber-950/30",
    borderColor: "border-amber-200 dark:border-amber-800",
  },
  verification: {
    title: "Verification",
    bgColor: "bg-orange-50 dark:bg-orange-950/30",
    borderColor: "border-orange-200 dark:border-orange-800",
  },
};

export const SpecialContent: React.FC<SpecialContentProps> = ({
  type,
  content,
}) => {
  const { currentProject } = useProjectStore();
  const baseDir = currentProject?.path || "";
  const styling = contentTypeStyles[type] || {
    title: type.charAt(0).toUpperCase() + type.slice(1),
    bgColor: "bg-gray-50 dark:bg-gray-900",
    borderColor: "border-gray-200 dark:border-gray-800",
  };

  return (
    <div
      className={`special-content my-4 rounded-md border ${styling.borderColor} ${styling.bgColor}`}
    >
      <div className="special-header px-4 py-1 border-b border-inherit">
        <h4 className="text-sm font-medium">{styling.title}</h4>
      </div>
      <div className="special-body p-4 prose prose-sm dark:prose-invert max-w-none">
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
            // Also make file paths in code blocks and other elements clickable
            code: ({ children }) => {
              if (typeof children === "string") {
                return (
                  <code>
                    <FilePathText
                      text={children as string}
                      options={{ baseDir }}
                    />
                  </code>
                );
              }
              return <code>{children}</code>;
            },
            li: ({ children }) => {
              if (typeof children === "string") {
                return (
                  <li>
                    <FilePathText
                      text={children as string}
                      options={{ baseDir }}
                    />
                  </li>
                );
              }
              return <li>{children}</li>;
            },
          }}
        >
          {content}
        </ReactMarkdown>
      </div>
    </div>
  );
};
