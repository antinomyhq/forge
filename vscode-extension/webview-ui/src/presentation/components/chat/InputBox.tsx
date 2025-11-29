import React from "react";
import { Textarea } from "@/components/ui/textarea";
import { Button } from "@/components/ui/button";
import { Send, Loader2, X } from "lucide-react";
import { cn } from "@/lib/utils";

interface InputBoxProps {
  onSend: (message: string) => void;
  onCancel?: () => void;
  disabled?: boolean;
  isStreaming?: boolean;
}

/// InputBox provides a text input for sending messages using shadcn components
export const InputBox: React.FC<InputBoxProps> = ({ 
  onSend, 
  onCancel,
  disabled = false,
  isStreaming = false,
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

  const characterCount = message.length;
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
        <div className="flex flex-col gap-2">
          {/* Character counter (optional) */}
          {characterCount > 0 && (
            <div 
              className="text-xs text-right"
              style={{ color: 'var(--vscode-descriptionForeground)' }}
            >
              {characterCount} character{characterCount !== 1 ? "s" : ""}
            </div>
          )}
          
          <div className="flex gap-2 items-end">
            {/* Textarea */}
            <Textarea
              ref={textareaRef}
              value={message}
              onChange={(e) => setMessage(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Type a message... (Enter to send, Shift+Enter for new line)"
              disabled={disabled}
              className={cn(
                "min-h-[80px] max-h-[200px] resize-none",
                disabled && "opacity-50 cursor-not-allowed"
              )}
              rows={3}
            />
            
            {/* Cancel Button (shown when streaming) */}
            {isStreaming && onCancel && (
              <Button
                type="button"
                onClick={handleCancel}
                variant="destructive"
                size="icon"
                className="h-10 w-10 shrink-0"
              >
                <X className="h-4 w-4" />
                <span className="sr-only">Cancel</span>
              </Button>
            )}
            
            {/* Send Button (shown when not streaming) */}
            {!isStreaming && (
              <Button
                type="submit"
                disabled={disabled || !hasContent}
                size="icon"
                className="h-10 w-10 shrink-0"
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

          {/* Hint text */}
          <div 
            className="text-xs"
            style={{ color: 'var(--vscode-descriptionForeground)' }}
          >
            Press <kbd className="px-1 py-0.5 rounded border" style={{ 
              backgroundColor: 'var(--vscode-input-background)',
              borderColor: 'var(--vscode-input-border)',
            }}>Enter</kbd> to send, 
            <kbd className="px-1 py-0.5 rounded border ml-1" style={{ 
              backgroundColor: 'var(--vscode-input-background)',
              borderColor: 'var(--vscode-input-border)',
            }}>Shift+Enter</kbd> for new line
          </div>
        </div>
      </form>
    </div>
  );
};
