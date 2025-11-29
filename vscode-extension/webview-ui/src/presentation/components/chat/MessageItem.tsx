import React from "react";
import { Card, CardContent } from "@/components/ui/card";
import { Avatar, AvatarFallback } from "@/components/ui/avatar";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import { User, Bot, AlertCircle } from "lucide-react";
import { Message, ToolExecution } from "@domain/models";

interface MessageItemProps {
  message: Message | ToolExecution | {
    role?: 'user' | 'assistant';
    content?: string;
    timestamp: number;
    type?: 'tool';
    toolName?: string;
    args?: Record<string, any>;
    status?: 'running' | 'completed' | 'failed';
  };
}

/// MessageItem displays a single message with role-based styling using shadcn components
export const MessageItem: React.FC<MessageItemProps> = ({ message }) => {
  // Handle different message types
  const role = 'role' in message ? (message.role || 'assistant') : 'assistant';
  const isUser = role === "user";
  const content = 'content' in message ? message.content : '';
  
  return (
    <div className={cn(
      "flex gap-3 p-3",
      isUser ? "flex-row-reverse" : "flex-row"
    )}>
      {/* Avatar */}
      <Avatar className="h-8 w-8 shrink-0">
        <AvatarFallback 
          style={{
            backgroundColor: isUser ? 'var(--vscode-button-background)' : 'var(--vscode-button-secondaryBackground)',
            color: isUser ? 'var(--vscode-button-foreground)' : 'var(--vscode-button-secondaryForeground)',
          }}
          className="text-xs"
        >
          {isUser ? <User className="h-4 w-4" /> : <Bot className="h-4 w-4" />}
        </AvatarFallback>
      </Avatar>

      {/* Message Content */}
      <div className={cn("flex flex-col gap-2 flex-1 min-w-0", isUser && "items-end")}>
        {/* Role Badge */}
        <Badge 
          variant={isUser ? "default" : "secondary"} 
          className="w-fit"
        >
          {role === "user" ? "You" : role === "assistant" ? "Assistant" : role}
        </Badge>

        {/* Message Card */}
        <Card 
          className="max-w-3xl"
          style={{
            backgroundColor: isUser 
              ? 'var(--vscode-input-background)' 
              : 'var(--vscode-editor-background)',
            borderColor: 'var(--vscode-panel-border)',
            color: 'var(--vscode-editor-foreground)',
          }}
        >
          <CardContent className="p-4">
            <div 
              className="whitespace-pre-wrap text-sm"
              style={{ color: 'var(--vscode-editor-foreground)' }}
            >
              {content || ''}
            </div>
            
            {/* Status indicators */}
            {'status' in message && message.status === "running" && (
              <div 
                className="flex items-center gap-2 mt-3 text-xs"
                style={{ color: 'var(--vscode-descriptionForeground)' }}
              >
                <div className="animate-pulse">●</div>
                <span>Processing...</span>
              </div>
            )}
            {'status' in message && message.status === "failed" && (
              <div 
                className="flex items-center gap-2 mt-3 text-xs"
                style={{ color: 'var(--vscode-errorForeground)' }}
              >
                <AlertCircle className="h-3 w-3" />
                <span>Failed</span>
              </div>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
};
