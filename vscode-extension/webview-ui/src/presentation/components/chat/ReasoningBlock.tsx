import React from "react";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { ChevronRight, Brain } from "lucide-react";

export interface Reasoning {
  content: string;
}

interface ReasoningBlockProps {
  reasoning: Reasoning;
}

/// ReasoningBlock displays the agent's reasoning in a collapsible card
export const ReasoningBlock: React.FC<ReasoningBlockProps> = ({ reasoning }) => {
  const [isOpen, setIsOpen] = React.useState(false);

  // Show first 100 characters as preview
  const preview = reasoning.content.length > 100 
    ? reasoning.content.slice(0, 100) + "..." 
    : reasoning.content;

  return (
    <Collapsible open={isOpen} onOpenChange={setIsOpen}>
      <div 
        className="flex items-start gap-2 py-2 px-3 my-1 rounded"
        style={{ 
          backgroundColor: 'var(--vscode-editor-inactiveSelectionBackground)',
          borderLeft: '3px solid var(--vscode-charts-purple)',
        }}
      >
        {/* Brain Icon */}
        <Brain 
          className="h-4 w-4 mt-0.5 flex-shrink-0" 
          style={{ color: 'var(--vscode-charts-purple)' }} 
        />
        
        <div className="flex-1 min-w-0">
          {/* Header */}
          <div className="flex items-center gap-2 mb-1">
            <span 
              className="text-xs font-semibold"
              style={{ color: 'var(--vscode-charts-purple)' }}
            >
              Reasoning
            </span>
            <CollapsibleTrigger className="ml-auto">
              <ChevronRight 
                className={`h-3 w-3 transition-transform ${isOpen ? 'rotate-90' : ''}`}
                style={{ color: 'var(--vscode-descriptionForeground)' }}
              />
            </CollapsibleTrigger>
          </div>

          {/* Preview (when collapsed) */}
          {!isOpen && (
            <div 
              className="text-xs line-clamp-2"
              style={{ color: 'var(--vscode-descriptionForeground)' }}
            >
              {preview}
            </div>
          )}
        </div>
      </div>

      {/* Full Content (when expanded) */}
      <CollapsibleContent>
        <div 
          className="ml-9 mr-3 mb-2 p-3 rounded text-sm whitespace-pre-wrap"
          style={{ 
            backgroundColor: 'var(--vscode-textCodeBlock-background)',
            color: 'var(--vscode-editor-foreground)',
            borderLeft: '2px solid var(--vscode-charts-purple)',
          }}
        >
          {reasoning.content}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
};
