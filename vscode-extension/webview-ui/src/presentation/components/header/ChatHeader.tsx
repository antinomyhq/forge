import React from "react";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { User, Coins, Hash } from "lucide-react";

interface ChatHeaderProps {
  agentName: string;
  tokenCount: string;
  cost: string;
}

/// ChatHeader displays agent info and usage statistics
export const ChatHeader: React.FC<ChatHeaderProps> = ({
  agentName,
  tokenCount,
  cost,
}) => {
  return (
    <div className="border-b bg-background">
      <div className="flex items-center justify-between px-4 py-3 gap-4">
        {/* Left: Agent */}
        <div className="flex items-center gap-3">
          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild>
                <Badge variant="secondary" className="gap-1.5">
                  <User className="h-3.5 w-3.5" />
                  <span className="font-medium">{agentName}</span>
                </Badge>
              </TooltipTrigger>
              <TooltipContent>
                <p>Current Agent</p>
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>
        </div>

        {/* Right: Stats */}
        <div className="flex items-center gap-3 text-sm text-muted-foreground">
          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild>
                <div className="flex items-center gap-1.5">
                  <Hash className="h-4 w-4" />
                  <span className="font-mono">{tokenCount}</span>
                </div>
              </TooltipTrigger>
              <TooltipContent>
                <p>Token Count</p>
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>

          <Separator orientation="vertical" className="h-4" />

          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild>
                <div className="flex items-center gap-1.5">
                  <Coins className="h-4 w-4" />
                  <span className="font-mono">{cost}</span>
                </div>
              </TooltipTrigger>
              <TooltipContent>
                <p>Estimated Cost</p>
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>
        </div>
      </div>
    </div>
  );
};
