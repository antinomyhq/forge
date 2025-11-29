import React from "react";
import { Card, CardContent } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { Badge } from "@/components/ui/badge";
import { Avatar, AvatarFallback } from "@/components/ui/avatar";
import { Bot, Loader2 } from "lucide-react";
import { MarkdownRenderer } from "./MarkdownRenderer";

interface StreamingIndicatorProps {
  delta: string;
}

/// StreamingIndicator displays the current streaming content with loading animation
export const StreamingIndicator: React.FC<StreamingIndicatorProps> = ({ delta }) => {
  return (
    <div className="flex gap-3 p-3">
      {/* Avatar */}
      <Avatar className="h-8 w-8 shrink-0">
        <AvatarFallback style={{ 
          backgroundColor: 'var(--vscode-badge-background)', 
          color: 'var(--vscode-badge-foreground)' 
        }}>
          <Bot className="h-4 w-4" />
        </AvatarFallback>
      </Avatar>

      {/* Streaming Content */}
      <div className="flex flex-col gap-2 flex-1 min-w-0">
        {/* Badge with animation */}
        <Badge variant="secondary" className="w-fit animate-pulse">
          <Loader2 className="h-3 w-3 mr-1 animate-spin" />
          Assistant is typing...
        </Badge>

        {/* Content Card */}
        <Card 
          style={{ 
            backgroundColor: 'var(--vscode-editor-background)', 
            borderColor: 'var(--vscode-panel-border)' 
          }}
        >
          <CardContent className="p-4">
            {delta ? (
              <MarkdownRenderer 
                content={delta} 
                className="text-sm"
              />
            ) : (
              <div className="space-y-2">
                <Skeleton className="h-4 w-full" />
                <Skeleton className="h-4 w-3/4" />
                <Skeleton className="h-4 w-5/6" />
              </div>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
};
