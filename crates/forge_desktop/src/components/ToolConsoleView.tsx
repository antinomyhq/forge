import React, { useState, useEffect, useRef } from "react";
import { useForgeStore } from "@/stores/ForgeStore";
import { formatDistanceToNow } from "date-fns";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  CheckCircle,
  XCircle,
  ChevronDown,
  ChevronUp,
  ClipboardCopy,
  Clock,
  Terminal,
  Maximize2,
  Minimize2
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

// Helper to format tool name for display
const formatToolName = (name: string): string => {
  return name
    .replace("tool_forge_", "")
    .split("_")
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");
};

// Helper to extract file path from content if available
const extractFilePath = (content: string): string | null => {
  try {
    // Parse JSON content
    const data = JSON.parse(content);

    // Look for common path fields
    if (data.path) return data.path;
    if (data.file_path) return data.file_path;
    if (data.filepath) return data.filepath;

    // Check if there are patches with paths
    if (data.patches && data.patches.length > 0 && data.patches[0].path) {
      return data.patches[0].path;
    }

    return null;
  } catch (e) {
    return null;
  }
};

interface EnhancedToolCall {
  id: string;
  name: string;
  displayName: string;
  content: string;
  isError: boolean;
  result?: string;
  timestamp: Date;
  filePath?: string | null;
}

const ToolCallItem: React.FC<{ toolCall: EnhancedToolCall }> = ({
  toolCall,
}) => {
  const [expanded, setExpanded] = useState(false);

  // Format timestamp as relative time (e.g., "2 minutes ago")
  const relativeTime = formatDistanceToNow(toolCall.timestamp, {
    addSuffix: true,
  });

  // Copy content to clipboard
  const handleCopy = (text: string | undefined) => {
    if (text) {
      navigator.clipboard.writeText(text);
    }
  };

  return (
    <div
      className={`tool-call-item mb-2 rounded-md border ${
        toolCall.isError
          ? "border-red-200 dark:border-red-900 bg-red-50/30 dark:bg-red-950/10"
          : "border-green-200 dark:border-green-900 bg-green-50/30 dark:bg-green-950/10"
      }`}
    >
      <div
        className="flex items-center justify-between p-2 cursor-pointer"
        onClick={() => setExpanded(!expanded)}
      >
        <div className="flex items-center space-x-2">
          {toolCall.isError ? (
            <XCircle className="h-4 w-4 text-red-500" />
          ) : (
            <CheckCircle className="h-4 w-4 text-green-500" />
          )}

          <span className="text-sm font-medium">
            {formatToolName(toolCall.name)}
          </span>

          {toolCall.filePath && (
            <span className="text-xs text-muted-foreground truncate max-w-[200px]">
              {typeof toolCall.filePath === "string"
                ? toolCall.filePath.split("/").pop()
                : ""}
            </span>
          )}
        </div>

        <div className="flex items-center space-x-2">
          <Tooltip>
            <TooltipTrigger asChild>
              <div className="flex items-center text-xs text-muted-foreground">
                <Clock className="h-3 w-3 mr-1" />
                <span>{relativeTime}</span>
              </div>
            </TooltipTrigger>
            <TooltipContent>
              {toolCall.timestamp.toLocaleTimeString()}
            </TooltipContent>
          </Tooltip>

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
      </div>

      {expanded && (
        <div className="p-2 border-t border-inherit">
          <div className="relative">
            <div className="text-xs font-medium mb-1">Arguments:</div>
            <pre className="bg-muted rounded-md p-2 text-xs font-mono overflow-x-auto whitespace-pre">
              {toolCall.content}
            </pre>
            <Button
              variant="outline"
              size="icon"
              className="absolute top-1 right-1 h-5 w-5 opacity-80 hover:opacity-100"
              onClick={() => handleCopy(toolCall.content)}
            >
              <ClipboardCopy className="h-3 w-3" />
            </Button>
          </div>

          {toolCall.result && (
            <div className="mt-2">
              <div className="text-xs font-medium mb-1">Result:</div>
              <div className="relative">
                <pre className="bg-muted rounded-md p-2 text-xs font-mono overflow-x-auto whitespace-pre">
                  {toolCall.result}
                </pre>
                <Button
                  variant="outline"
                  size="icon"
                  className="absolute top-1 right-1 h-5 w-5 opacity-80 hover:opacity-100"
                  onClick={() => handleCopy(toolCall.result)}
                >
                  <ClipboardCopy className="h-3 w-3" />
                </Button>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
};

const ToolConsoleView: React.FC = () => {
  const { toolCalls } = useForgeStore();
  const toolConsoleRef = useRef<HTMLDivElement>(null);
  const [isOpen, setIsOpen] = useState<boolean>(() => {
    return localStorage.getItem("toolConsoleOpen") === "true";
  });
  const [height, setHeight] = useState<number>(() => {
    return Number(localStorage.getItem("toolConsoleHeight")) || 250;
  });
  const [isDragging, setIsDragging] = useState(false);
  const dragStartY = useRef<number | null>(null);
  const startHeight = useRef<number>(height);

  // Enhanced tool calls with timestamps and extracted file paths
  const enhancedToolCalls: EnhancedToolCall[] = toolCalls.map((tool) => {
    // Extract timestamp safely from the ID or use current time
    const idParts = tool.id.split("-");
    const timestampString = idParts.length > 1 ? idParts[1] : null;
    const timestamp = timestampString ? parseInt(timestampString) : Date.now();

    return {
      ...tool,
      displayName: formatToolName(tool.name),
      timestamp: new Date(timestamp),
      filePath: extractFilePath(tool.content),
    };
  });

  // Sort tool calls by timestamp (newest first)
  const sortedToolCalls = [...enhancedToolCalls].sort(
    (a, b) => b.timestamp.getTime() - a.timestamp.getTime(),
  );

  // Group tool calls by type
  const toolsByType = sortedToolCalls.reduce(
    (acc, tool) => {
      const type = tool.isError ? "errors" : "success";
      if (!acc[type]) acc[type] = [];
      acc[type].push(tool);
      return acc;
    },
    {} as Record<string, EnhancedToolCall[]>,
  );

  // Save settings to localStorage
  useEffect(() => {
    localStorage.setItem("toolConsoleOpen", isOpen ? "true" : "false");
  }, [isOpen]);

  useEffect(() => {
    localStorage.setItem("toolConsoleHeight", height.toString());
  }, [height]);

  // Scroll to bottom on new tool calls
  useEffect(() => {
    if (toolConsoleRef.current && isOpen) {
      const viewport = toolConsoleRef.current.querySelector(
        "[data-radix-scroll-area-viewport]",
      );
      if (viewport) {
        viewport.scrollTop = viewport.scrollHeight;
      }
    }
  }, [toolCalls.length, isOpen]);

  // Auto-open console on error
  useEffect(() => {
    if (toolsByType.errors && toolsByType.errors.length > 0) {
      const lastErrorTime = toolsByType.errors[0].timestamp.getTime();
      const lastCheckTime = parseInt(localStorage.getItem("lastErrorCheckTime") || "0");
      
      if (lastErrorTime > lastCheckTime) {
        setIsOpen(true);
        localStorage.setItem("lastErrorCheckTime", Date.now().toString());
      }
    }
  }, [toolsByType.errors]);

  // Handle mouse events for resizing
  const handleMouseDown = (e: React.MouseEvent) => {
    e.preventDefault();
    dragStartY.current = e.clientY;
    startHeight.current = height;
    setIsDragging(true);
  };

  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (isDragging && dragStartY.current !== null) {
        const deltaY = dragStartY.current - e.clientY;
        const newHeight = Math.max(150, Math.min(500, startHeight.current + deltaY));
        setHeight(newHeight);
      }
    };

    const handleMouseUp = () => {
      setIsDragging(false);
      dragStartY.current = null;
    };

    if (isDragging) {
      window.addEventListener('mousemove', handleMouseMove);
      window.addEventListener('mouseup', handleMouseUp);
    }

    return () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
    };
  }, [isDragging]);

  return (
    <div className="z-10 w-full">

      {/* Resize handle - only visible when console is open */}
      {isOpen && (
        <div 
          className={`w-full h-1 bg-border hover:bg-primary cursor-ns-resize ${isDragging ? 'bg-primary' : ''}`}
          onMouseDown={handleMouseDown}
        />
      )}
      
      <div 
        className="w-full h-full bg-background border-t border-border shadow-md overflow-hidden"
        style={{ 
          height: isOpen ? `${height}px` : '40px'
        }}
      >
        {/* Header bar - always visible */}
        <div 
          className="flex items-center justify-between px-4 py-2 border-b cursor-pointer"
          onClick={() => setIsOpen(!isOpen)}
        >
          <div className="flex items-center">
            <Terminal className="h-4 w-4 mr-2" />
            <h3 className="text-sm font-medium">Tool Console</h3>
            
            <div className="flex space-x-2 ml-4">
              {toolsByType.errors && toolsByType.errors.length > 0 && (
                <Badge variant="destructive" className="h-5">
                  {toolsByType.errors.length} Error
                  {toolsByType.errors.length > 1 ? "s" : ""}
                </Badge>
              )}
              {toolsByType.success && toolsByType.success.length > 0 && (
                <Badge
                  variant="outline"
                  className="h-5 bg-green-100 dark:bg-green-900/20"
                >
                  {toolsByType.success.length} Success
                </Badge>
              )}
            </div>
          </div>
          
          <Button
            variant="ghost"
            size="sm"
            className="h-6"
            onClick={(e) => {
              e.stopPropagation();
              setIsOpen(!isOpen);
            }}
          >
            {isOpen ? <Minimize2 className="h-4 w-4" /> : <Maximize2 className="h-4 w-4" />}
          </Button>
        </div>

        {/* Content area - only visible when open */}
        {isOpen && (
          <ScrollArea 
            className="flex-1 p-2" 
            style={{ height: `calc(${height}px - 41px)` }}
            ref={toolConsoleRef}
          >
            {sortedToolCalls.length > 0 ? (
              sortedToolCalls.map((tool) => (
                <ToolCallItem key={tool.id} toolCall={tool} />
              ))
            ) : (
              <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
                No tool calls yet
              </div>
            )}
          </ScrollArea>
        )}
      </div>
    </div>
  );
};

export default ToolConsoleView;