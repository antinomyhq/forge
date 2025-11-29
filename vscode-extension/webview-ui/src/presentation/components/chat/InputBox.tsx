import React from "react";
import { Textarea } from "@/components/ui/textarea";
import { Button } from "@/components/ui/button";
import { Send, Loader2, X } from "lucide-react";
import { cn } from "@/lib/utils";
import { ModelPicker } from "@/presentation/components/header/ModelPicker";
import { AgentPicker } from "@/presentation/components/header/AgentPicker";
import { AIModel, Agent } from "@domain/models";

interface InputBoxProps {
  onSend: (message: string) => void;
  onCancel?: () => void;
  disabled?: boolean;
  isStreaming?: boolean;
  models: ReadonlyArray<AIModel>;
  agents: ReadonlyArray<Agent>;
  selectedModelId: string;
  selectedModelName: string;
  selectedAgentId: string;
  selectedAgentName: string;
  onModelChange: (modelId: string) => void;
  onAgentChange: (agentId: string) => void;
}

/// InputBox provides a text input for sending messages using shadcn components
export const InputBox: React.FC<InputBoxProps> = ({ 
  onSend, 
  onCancel,
  disabled = false,
  isStreaming = false,
  models,
  agents,
  selectedModelId,
  selectedModelName,
  selectedAgentId,
  selectedAgentName,
  onModelChange,
  onAgentChange,
}) => {
  const [message, setMessage] = React.useState("");
  const textareaRef = React.useRef<HTMLTextAreaElement>(null);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (message.trim() && !disabled) {
      onSend(message);
      setMessage("");
      // Reset textarea height
      if (textareaRef.current) {
        textareaRef.current.style.height = "auto";
      }
    }
  };

  const handleCancel = () => {
    if (onCancel) {
      onCancel();
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    // Submit on Enter (without Shift)
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSubmit(e);
    }
  };

  const hasContent = message.trim().length > 0;

  return (
    <div 
      className="border-t"
      style={{ 
        backgroundColor: 'var(--vscode-editor-background)',
        borderColor: 'var(--vscode-panel-border)',
      }}
    >
      <form onSubmit={handleSubmit} className="p-4">
        <div className="flex flex-col gap-3">
          {/* Textarea with embedded button */}
          <div className="relative">
            <Textarea
              ref={textareaRef}
              value={message}
              onChange={(e) => setMessage(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Ask Forge to do anything..."
              disabled={disabled}
              className={cn(
                "min-h-[80px] max-h-[200px] resize-none pr-12",
                disabled && "opacity-50 cursor-not-allowed"
              )}
              rows={3}
            />
            
            {/* Send/Cancel Button - Positioned inside textarea */}
            <div className="absolute bottom-2 right-2">
              {isStreaming && onCancel ? (
                <Button
                  type="button"
                  onClick={handleCancel}
                  variant="destructive"
                  size="icon"
                  className="h-9 w-9 rounded-full shadow-sm"
                >
                  <X className="h-4 w-4" />
                  <span className="sr-only">Cancel</span>
                </Button>
              ) : (
                <Button
                  type="submit"
                  disabled={disabled || !hasContent}
                  size="icon"
                  className={cn(
                    "h-9 w-9 rounded-full shadow-sm transition-all",
                    hasContent && !disabled 
                      ? "opacity-100" 
                      : "opacity-50"
                  )}
                >
                  {disabled ? (
                    <Loader2 className="h-4 w-4 animate-spin" />
                  ) : (
                    <Send className="h-4 w-4" />
                  )}
                  <span className="sr-only">Send message</span>
                </Button>
              )}
            </div>
          </div>

          {/* Bottom row: Agent and Model Pickers */}
          <div className="flex items-center gap-2">
            {/* Agent Picker */}
            <AgentPicker
              agents={agents}
              selectedAgentId={selectedAgentId}
              selectedAgentName={selectedAgentName}
              onAgentChange={onAgentChange}
              disabled={isStreaming}
              compact={true}
            />
            
            {/* Separator */}
            <div className="h-4 w-px bg-border" />
            
            {/* Model Picker */}
            <ModelPicker
              models={models}
              selectedModelId={selectedModelId}
              selectedModelName={selectedModelName}
              onModelChange={onModelChange}
              disabled={isStreaming}
              compact={true}
            />
          </div>
        </div>
      </form>
    </div>
  );
};
