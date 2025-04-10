import React from "react";
import { useForgeStore } from "@/stores/ForgeStore";
import { AlertCircle, CheckCircle, Loader2 } from "lucide-react";
import { Card } from "@/components/ui/card";
import ThemeSwitcher from "./ThemeSwitcher";

const StatusBar: React.FC = () => {
  const { conversationId, error, isLoading } = useForgeStore();

  return (
    <Card className="rounded-none shadow-none border-t border-border h-8 px-4 flex items-center justify-between bg-muted/40">
      <div className="flex items-center gap-2 text-xs text-muted-foreground">
        {conversationId ? (
          <>
            <span>Conversation:</span>
            <code className="bg-muted px-1 py-0.5 rounded font-mono text-xs">
              {`${conversationId.substring(0, 8)}...`}
            </code>
          </>
        ) : (
          <span>No active conversation</span>
        )}
      </div>

      <div className="flex items-center gap-4">
        <div className="flex items-center gap-2 text-xs">
          {error ? (
            <div className="flex items-center gap-1 text-destructive">
              <AlertCircle className="h-3 w-3" />
              <span>{error}</span>
            </div>
          ) : isLoading ? (
            <div className="flex items-center gap-1 text-muted-foreground">
              <Loader2 className="h-3 w-3 animate-spin" />
              <span>Processing...</span>
            </div>
          ) : (
            <div className="flex items-center gap-1 text-primary">
              <CheckCircle className="h-3 w-3" />
              <span>Ready</span>
            </div>
          )}
        </div>
        <ThemeSwitcher />
      </div>
    </Card>
  );
};

export default StatusBar;
