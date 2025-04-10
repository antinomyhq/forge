import React, { useState, useEffect } from "react";
import { Button } from "@/components/ui/button";
import {
  ChevronDown,
  ChevronUp,
  AlertCircle,
  CheckCircle,
  ClipboardCopy,
  Diff,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import {
  tryParseDiffJson,
  isDiffContent,
  DiffJsonData,
} from "@/utils/diffUtils";
import { useChangesStore } from "@/stores/ChangesStore";
import { SideBySideDiff } from "./SideBySideDiff";
import { DiffModalView } from "../DiffModalView";

interface ToolResultDisplayProps {
  toolCall: {
    id: string;
    name: string;
    content: string;
    isError: boolean;
    result?: string;
  };
}

export const ToolResultDisplay: React.FC<ToolResultDisplayProps> = ({
  toolCall,
}) => {
  const [expanded, setExpanded] = useState(false);
  const [isDiff, setIsDiff] = useState(false);
  const [diffData, setDiffData] = useState<DiffJsonData | null>(null);
  const [showDiffModal, setShowDiffModal] = useState(false);

  const addChangeFromToolResult = useChangesStore(
    (state) => state.addChangeFromToolResult,
  );

  // Check if the result is a diff when the component mounts or when the result changes
  useEffect(() => {
    if (toolCall.result) {
      const jsonDiff = tryParseDiffJson(toolCall.result);
      if (jsonDiff) {
        setIsDiff(true);
        setDiffData(jsonDiff);
        addChangeFromToolResult(toolCall.name, toolCall.result);
      } else if (isDiffContent(toolCall.result)) {
        setIsDiff(true);
      } else {
        setIsDiff(false);
        setDiffData(null);
      }
    } else {
      setIsDiff(false);
      setDiffData(null);
    }
  }, [toolCall.result, addChangeFromToolResult, toolCall.name]);

  // Format the tool name for display
  const displayName = toolCall.name
    .replace("tool_forge_", "")
    .split("_")
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");

  // Handle copy to clipboard
  const handleCopy = (text: string) => {
    navigator.clipboard.writeText(text);
  };

  return (
    <div
      className={`tool-result rounded-md border ${
        toolCall.isError
          ? "border-red-200 dark:border-red-900 bg-red-50/50 dark:bg-red-950/20"
          : "border-gray-200 dark:border-gray-800 bg-gray-50/80 dark:bg-gray-900/50"
      }`}
    >
      <div
        className="tool-header flex items-center justify-between px-4 py-2 cursor-pointer border-b border-inherit"
        onClick={() => setExpanded(!expanded)}
      >
        <div className="flex items-center gap-2">
          <span className="text-sm font-medium">Tool Call: {displayName}</span>
          <Badge
            variant={toolCall.isError ? "destructive" : "outline"}
            className="h-5"
          >
            {toolCall.isError ? (
              <>
                <AlertCircle className="mr-1 h-3 w-3" />
                Error
              </>
            ) : (
              <>
                <CheckCircle className="mr-1 h-3 w-3" />
                Success
              </>
            )}
          </Badge>
        </div>

        <Button
          variant="ghost"
          size="icon"
          className="h-6 w-6"
          onClick={(e) => {
            e.stopPropagation();
            setExpanded(!expanded);
          }}
        >
          {expanded ? (
            <ChevronUp className="h-4 w-4" />
          ) : (
            <ChevronDown className="h-4 w-4" />
          )}
        </Button>
      </div>

      {expanded && (
        <div className="tool-content p-3">
          <div className="relative">
            <pre className="bg-muted rounded-md p-3 font-mono text-xs overflow-x-auto whitespace-pre">
              {toolCall.content}
            </pre>
            <Button
              variant="outline"
              size="icon"
              className="absolute top-1 right-1 h-6 w-6 opacity-80 hover:opacity-100"
              onClick={() => handleCopy(toolCall.content)}
            >
              <ClipboardCopy className="h-3 w-3" />
            </Button>
          </div>

          {toolCall.result && (
            <div className="mt-2">
              <div className="flex items-center justify-between mb-1">
                <span className="text-xs font-medium">Result:</span>
                {isDiff && diffData && (
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-6 text-xs flex items-center gap-1"
                    onClick={() => setShowDiffModal(true)}
                  >
                    <Diff className="h-3 w-3" />
                    View Diff
                  </Button>
                )}
              </div>

              {isDiff && diffData && diffData.has_changes ? (
                <div className="mt-2">
                  <SideBySideDiff
                    diffData={diffData}
                    compact={true}
                    onExpandClick={() => setShowDiffModal(true)}
                  />
                </div>
              ) : (
                <div className="relative mt-1">
                  <pre className="bg-muted rounded-md p-3 font-mono text-xs overflow-x-auto whitespace-pre">
                    {toolCall.result}
                  </pre>
                  <Button
                    variant="outline"
                    size="icon"
                    className="absolute top-1 right-1 h-6 w-6 opacity-80 hover:opacity-100"
                    onClick={() => handleCopy(toolCall.result || "")}
                  >
                    <ClipboardCopy className="h-3 w-3" />
                  </Button>
                </div>
              )}

              {/* Diff Modal */}
              {diffData && (
                <DiffModalView
                  open={showDiffModal}
                  onOpenChange={setShowDiffModal}
                  diffData={diffData}
                />
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
};
