import React, { useState, useMemo } from "react";
import ReactDiffViewer, { DiffMethod } from "react-diff-viewer-continued";
import { FileText, Maximize2, Minimize2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useTheme } from "next-themes";
import { DiffJsonData } from "@/utils/diffUtils";
import { Badge } from "@/components/ui/badge";

interface SideBySideDiffProps {
  diffData: DiffJsonData;
  showFilePath?: boolean;
  compact?: boolean;
  onExpandClick?: () => void;
}

export const SideBySideDiff: React.FC<SideBySideDiffProps> = ({
  diffData,
  showFilePath = true,
  compact = false,
  onExpandClick,
}) => {
  const { theme } = useTheme();
  const [viewType, setViewType] = useState<"unified" | "split">("split");

  // If there are no changes, don't display anything
  if (!diffData.has_changes) {
    return null;
  }

  // Extract old and new content from the diff data
  const { oldContent, newContent } = useMemo(() => {
    // If there are no changes, just use the content as is
    if (!diffData.has_changes) {
      return {
        oldContent: "",
        newContent: "",
      };
    }

    // Reconstruct old and new content from hunks
    const oldLines: string[] = [];
    const newLines: string[] = [];

    for (const hunk of diffData.hunks) {
      for (const change of hunk.changes) {
        if (change.tag === "delete" || change.tag === "equal") {
          if (change.old_index !== null) {
            while (oldLines.length < change.old_index) {
              oldLines.push("");
            }
            oldLines[change.old_index] = change.content;
          }
        }

        if (change.tag === "insert" || change.tag === "equal") {
          if (change.new_index !== null) {
            while (newLines.length < change.new_index) {
              newLines.push("");
            }
            newLines[change.new_index] = change.content;
          }
        }
      }
    }

    return {
      oldContent: oldLines.join(""),
      newContent: newLines.join(""),
    };
  }, [diffData]);

  const isDarkTheme = theme === "dark";

  return (
    <div className="diff-viewer-container">
      {showFilePath && (
        <div className="diff-header flex items-center justify-between p-2 bg-muted rounded-t-md border border-border">
          <div className="flex items-center gap-2">
            <FileText size={16} />
            <span className="text-sm font-medium">{diffData.path}</span>
            <Badge variant="outline" className="text-xs">
              {diffData.file_type}
            </Badge>
          </div>
          <div className="flex items-center gap-2">
            <Button
              variant="ghost"
              size="sm"
              className="h-7 text-xs"
              onClick={() =>
                setViewType(viewType === "unified" ? "split" : "unified")
              }
            >
              {viewType === "unified" ? "Side by Side" : "Unified"}
            </Button>
            {onExpandClick && (
              <Button
                variant="ghost"
                size="icon"
                className="h-7 w-7"
                onClick={onExpandClick}
              >
                {compact ? <Maximize2 size={14} /> : <Minimize2 size={14} />}
              </Button>
            )}
          </div>
        </div>
      )}

      <div
        className={`diff-content ${compact ? "max-h-[400px] overflow-auto" : ""} border border-t-0 border-border rounded-b-md`}
      >
        <ReactDiffViewer
          oldValue={oldContent}
          newValue={newContent}
          splitView={viewType === "split"}
          disableWordDiff={compact}
          useDarkTheme={isDarkTheme}
          compareMethod={DiffMethod.WORDS}
          codeFoldMessageRenderer={() => (
            <span className="text-xs">Expand Code</span>
          )}
          styles={{
            contentText: {
              fontSize: "14px",
              lineHeight: "1.5",
              fontFamily: '"JetBrains Mono", monospace',
            },
          }}
        />
      </div>
    </div>
  );
};
