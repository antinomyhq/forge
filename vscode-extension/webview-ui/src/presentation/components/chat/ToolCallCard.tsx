import React from "react";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { ChevronRight, CheckCircle2, XCircle, Loader2 } from "lucide-react";

export interface ToolCall {
  id: string;
  name: string;
  status: "running" | "completed" | "failed" | "pending";
  arguments?: Record<string, unknown>;
  result?: string;
  error?: string;
}

interface ToolCallCardProps {
  toolCall: ToolCall;
}

/// ToolCallCard displays a compact log-like line for tool execution
export const ToolCallCard: React.FC<ToolCallCardProps> = ({ toolCall }) => {
  const [isOpen, setIsOpen] = React.useState(false);

  const getStatusIcon = () => {
    switch (toolCall.status) {
      case "running":
        return <Loader2 className="h-3 w-3 animate-spin" style={{ color: 'var(--vscode-charts-blue)' }} />;
      case "completed":
        return <CheckCircle2 className="h-3 w-3" style={{ color: 'var(--vscode-charts-green)' }} />;
      case "failed":
        return <XCircle className="h-3 w-3" style={{ color: 'var(--vscode-errorForeground)' }} />;
      default:
        return <ChevronRight className="h-3 w-3" style={{ color: 'var(--vscode-descriptionForeground)' }} />;
    }
  };

  const getStatusText = () => {
    switch (toolCall.status) {
      case "running":
        return "Running...";
      case "completed":
        return "Completed";
      case "failed":
        return "Failed";
      default:
        return "Pending";
    }
  };

  const hasDetails = toolCall.arguments || toolCall.result || toolCall.error;

  return (
    <Collapsible open={isOpen} onOpenChange={setIsOpen}>
      <div 
        className="flex items-center gap-2 py-1 px-2 text-xs font-mono"
        style={{ 
          color: 'var(--vscode-descriptionForeground)',
          backgroundColor: 'var(--vscode-editor-background)',
        }}
      >
        {/* Status Icon */}
        {getStatusIcon()}
        
        {/* Tool execution text */}
        <span style={{ color: 'var(--vscode-descriptionForeground)' }}>
          Execute <span style={{ color: 'var(--vscode-terminal-ansiBrightBlue)' }}>{toolCall.name}</span>
        </span>
        
        {/* Status */}
        <span style={{ color: 'var(--vscode-descriptionForeground)' }}>
          ({getStatusText()})
        </span>

        {/* Expand button if there are details */}
        {hasDetails && (
          <CollapsibleTrigger className="ml-auto">
            <ChevronRight 
              className={`h-3 w-3 transition-transform ${isOpen ? 'rotate-90' : ''}`}
              style={{ color: 'var(--vscode-descriptionForeground)' }}
            />
          </CollapsibleTrigger>
        )}
      </div>

      {/* Collapsible Details */}
      {hasDetails && (
        <CollapsibleContent>
          <div 
            className="ml-5 pl-3 text-xs space-y-1"
            style={{ 
              borderLeft: '2px solid var(--vscode-panel-border)',
              color: 'var(--vscode-descriptionForeground)'
            }}
          >
            {/* Arguments */}
            {toolCall.arguments && Object.keys(toolCall.arguments).length > 0 && (
              <div className="py-1">
                <div className="font-semibold mb-1">Arguments:</div>
                <pre 
                  className="p-2 rounded overflow-x-auto font-mono"
                  style={{ 
                    backgroundColor: 'var(--vscode-textCodeBlock-background)',
                    color: 'var(--vscode-editor-foreground)'
                  }}
                >
                  {JSON.stringify(toolCall.arguments, null, 2)}
                </pre>
              </div>
            )}

            {/* Result */}
            {toolCall.result && (
              <div className="py-1">
                <div className="font-semibold mb-1">Result:</div>
                <div 
                  className="p-2 rounded whitespace-pre-wrap font-mono"
                  style={{ 
                    backgroundColor: 'var(--vscode-textCodeBlock-background)',
                    color: 'var(--vscode-editor-foreground)'
                  }}
                >
                  {toolCall.result}
                </div>
              </div>
            )}

            {/* Error */}
            {toolCall.error && (
              <div className="py-1">
                <div 
                  className="font-semibold mb-1"
                  style={{ color: 'var(--vscode-errorForeground)' }}
                >
                  Error:
                </div>
                <div 
                  className="p-2 rounded whitespace-pre-wrap font-mono"
                  style={{ 
                    backgroundColor: 'var(--vscode-inputValidation-errorBackground)',
                    color: 'var(--vscode-errorForeground)'
                  }}
                >
                  {toolCall.error}
                </div>
              </div>
            )}
          </div>
        </CollapsibleContent>
      )}
    </Collapsible>
  );
};
