import React from "react";
import { Button } from "@/components/ui/button";
import { ClipboardCopy } from "lucide-react";
import { cn } from "@/utils/utils";

interface CodeBlockProps {
  code: string;
  language?: string;
  fileName?: string | null;
  showLineNumbers?: boolean;
  className?: string;
}

export const CodeBlock: React.FC<CodeBlockProps> = ({
  code,
  language = "text",
  fileName = null,
  showLineNumbers = true,
  className,
}) => {
  const handleCopyCode = () => {
    navigator.clipboard.writeText(code);
  };

  // Function to format the display language
  const formatLanguage = (lang: string): string => {
    const languageMap: Record<string, string> = {
      js: "JavaScript",
      ts: "TypeScript",
      tsx: "TypeScript (JSX)",
      jsx: "JavaScript (JSX)",
      py: "Python",
      rb: "Ruby",
      rs: "Rust",
      go: "Go",
      java: "Java",
      cpp: "C++",
      cs: "C#",
      php: "PHP",
      html: "HTML",
      css: "CSS",
      json: "JSON",
      yaml: "YAML",
      md: "Markdown",
      sql: "SQL",
      sh: "Shell",
      bash: "Bash",
    };

    return languageMap[lang] || lang.charAt(0).toUpperCase() + lang.slice(1);
  };

  // Get line numbers for the code
  const lineNumbers = showLineNumbers
    ? code
        .split("\n")
        .map((_, idx) => idx + 1)
        .join("\n")
    : null;

  return (
    <div
      className={cn(
        "code-block overflow-hidden rounded-md border border-border",
        className,
      )}
    >
      {/* Header with file name/language and copy button */}
      <div className="flex items-center justify-between px-4 py-2 bg-muted/50 border-b border-border">
        <div className="flex items-center gap-2">
          {fileName && <span className="text-xs font-medium">{fileName}</span>}
          {language && language !== "text" && !fileName && (
            <span className="text-xs font-medium">
              {formatLanguage(language)}
            </span>
          )}
        </div>
        <Button
          variant="ghost"
          size="icon"
          className="h-6 w-6"
          onClick={handleCopyCode}
          title="Copy code"
        >
          <ClipboardCopy className="h-3.5 w-3.5" />
        </Button>
      </div>

      {/* Code content with optional line numbers */}
      <div className="relative">
        <pre className="flex text-xs overflow-x-auto">
          {showLineNumbers && (
            <div className="line-numbers select-none px-3 py-3 text-right text-muted-foreground bg-muted/30 border-r border-border">
              {lineNumbers}
            </div>
          )}
          <code className={`language-${language} p-3 w-full`}>{code}</code>
        </pre>
      </div>
    </div>
  );
};
