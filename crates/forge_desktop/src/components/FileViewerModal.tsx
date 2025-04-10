import React, { useState, useEffect, useRef } from "react";
import { useTheme } from "next-themes";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Clipboard, X, Check, Download } from "lucide-react";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useFileViewerStore } from "@/stores/FileViewerStore";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import {
  oneDark,
  oneLight,
} from "react-syntax-highlighter/dist/esm/styles/prism";
import { Loader2 } from "lucide-react";

interface FileViewerModalProps {
  // The component doesn't need props as it uses the global store
}

const FileViewerModal: React.FC<FileViewerModalProps> = () => {
  const { theme } = useTheme();
  const [copied, setCopied] = useState(false);
  const contentRef = useRef<HTMLDivElement>(null);

  const {
    isOpen,
    fileName,
    content,
    language,
    isLoading,
    error,
    selectedLine,
    closeViewer,
  } = useFileViewerStore();

  useEffect(() => {
    if (copied) {
      const timer = setTimeout(() => {
        setCopied(false);
      }, 2000);

      return () => clearTimeout(timer);
    }
  }, [copied]);

  // Add a new useEffect to manually set data attributes on line numbers after render
  useEffect(() => {
    // After the component renders, manually add data attributes to line numbers
    if (contentRef.current && content) {
      setTimeout(() => {
        const lineNumberElements =
          contentRef.current?.querySelectorAll(".linenumber");
        lineNumberElements?.forEach((el, index) => {
          el.setAttribute("data-line-number", String(index + 1));
        });
      }, 50); // Small delay to ensure the syntax highlighter has rendered
    }
  }, [content]); // Re-run when content changes

  // Scroll to selected line if specified
  useEffect(() => {
    if (content && selectedLine && contentRef.current) {
      setTimeout(() => {
        // Find the line element by its line number
        const lineElement = contentRef.current?.querySelector(
          `[data-line-number="${selectedLine}"]`,
        );
        if (lineElement) {
          // Scroll the line into view with some offset
          lineElement.scrollIntoView({ behavior: "smooth", block: "center" });

          // Briefly highlight the line
          lineElement.classList.add("bg-yellow-100", "dark:bg-yellow-900");
          setTimeout(() => {
            lineElement.classList.remove("bg-yellow-100", "dark:bg-yellow-900");
            lineElement.classList.add(
              "bg-yellow-50",
              "dark:bg-yellow-900/30",
              "transition-colors",
              "duration-1000",
            );
          }, 1000);
        }
      }, 100); // Small delay to ensure content is rendered
    }
  }, [content, selectedLine]);

  const handleCopy = () => {
    if (content) {
      navigator.clipboard.writeText(content);
      setCopied(true);
    }
  };

  const handleDownload = () => {
    if (content && fileName) {
      const blob = new Blob([content], { type: "text/plain" });
      const link = document.createElement("a");
      link.href = URL.createObjectURL(blob);
      link.download = fileName;
      document.body.appendChild(link);
      link.click();
      document.body.removeChild(link);
    }
  };

  if (!isOpen) {
    return null;
  }

  // Choose syntax highlighting theme based on app theme
  const syntaxTheme = theme === "dark" ? oneDark : oneLight;

  const renderContent = () => {
    if (isLoading) {
      return (
        <div className="flex items-center justify-center h-full">
          <Loader2 className="h-8 w-8 animate-spin text-primary" />
        </div>
      );
    }

    if (error) {
      return (
        <div className="flex flex-col items-center justify-center h-full text-destructive p-4">
          <div className="text-center">
            <p className="font-medium">Error loading file</p>
            <p className="mt-2">{error}</p>
          </div>
        </div>
      );
    }

    if (!content) {
      return (
        <div className="flex items-center justify-center h-full text-muted-foreground">
          <p>No content to display</p>
        </div>
      );
    }

    return (
      <div className="relative h-full" ref={contentRef}>
        <SyntaxHighlighter
          language={language || "text"}
          style={syntaxTheme}
          showLineNumbers={true}
          customStyle={{
            margin: 0,
            padding: "1rem",
            height: "100%",
            fontSize: "0.9rem",
            borderRadius: "0",
            overflow: "visible",
          }}
          lineNumberStyle={{
            color: theme === "dark" ? "#6b7280" : "#9ca3af",
            fontSize: "0.8rem",
            marginRight: "1rem",
            // Note: The attributes property was removed as it's not supported in the type
          }}
          wrapLines={true}
          lineProps={(lineNumber: number) => {
            const style: React.HTMLAttributes<HTMLElement> = {
              style: { display: "block" },
              className:
                selectedLine === lineNumber
                  ? "bg-yellow-100 dark:bg-yellow-900/70 -mx-4 px-4"
                  : undefined,
            };
            return style;
          }}
        >
          {content}
        </SyntaxHighlighter>
      </div>
    );
  };

  return (
    <Dialog open={isOpen} onOpenChange={closeViewer}>
      <DialogContent className="max-w-[90vw] w-[90vw] max-h-[90vh] h-[90vh] flex flex-col">
        <DialogHeader className="flex w-full items-center justify-between">
          <div className="flex items-center space-x-2">
            <DialogTitle className="text-lg font-medium">
              {fileName || "File Viewer"}
            </DialogTitle>
            {language && (
              <div className="px-2 py-1 rounded-md bg-muted text-xs font-medium">
                {language}
              </div>
            )}
            {selectedLine && (
              <div className="px-2 py-1 rounded-md bg-yellow-100 dark:bg-yellow-900/50 text-xs font-medium text-yellow-800 dark:text-yellow-200">
                Line {selectedLine}
              </div>
            )}
          </div>
          <div className="flex items-center space-x-2">
            <Button
              variant="outline"
              size="icon"
              onClick={handleCopy}
              disabled={!content || isLoading}
              title="Copy content"
            >
              {copied ? (
                <Check className="h-4 w-4 text-green-600" />
              ) : (
                <Clipboard className="h-4 w-4" />
              )}
            </Button>

            <Button
              variant="outline"
              size="icon"
              onClick={handleDownload}
              disabled={!content || isLoading}
              title="Download file"
            >
              <Download className="h-4 w-4" />
            </Button>

            <Button variant="ghost" size="icon" onClick={closeViewer}>
              <X className="h-4 w-4" />
            </Button>
          </div>
        </DialogHeader>

        <div className="flex-grow overflow-hidden border rounded-md">
          <ScrollArea className="h-full">{renderContent()}</ScrollArea>
        </div>
      </DialogContent>
    </Dialog>
  );
};

export default FileViewerModal;
