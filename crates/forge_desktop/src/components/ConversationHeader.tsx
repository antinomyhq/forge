import React from "react";
import { useForgeStore } from "@/stores/ForgeStore";
import ProjectSwitcher from "@/components/ProjectSwitcher";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
} from "@/components/ui/card";
import { PlusCircle, Download, Settings2 } from "lucide-react";

const ConversationHeader: React.FC = () => {
  const { conversationTitle, newConversation, exportConversation } =
    useForgeStore();

  const handleNewConversation = () => {
    newConversation();
  };

  const handleExportConversation = async () => {
    try {
      const path = await exportConversation();
      alert(`Conversation exported to: ${path}`);
    } catch (error) {
      console.error("Export failed:", error);
    }
  };

  return (
    <Card className="rounded-none border-b border-border/50 shadow-sm">
      <CardHeader className="py-3 px-6">
        <div className="flex justify-between items-center">
          <div>
            <CardTitle className="text-2xl font-bold tracking-tight">
              {conversationTitle || "New Conversation"}
            </CardTitle>
            <CardDescription className="text-sm">Forge Desktop</CardDescription>
          </div>

          <div className="flex gap-2 items-center">
            <ProjectSwitcher />

            <Button
              variant="outline"
              onClick={handleNewConversation}
              size="sm"
              className="gap-1.5"
            >
              <PlusCircle className="h-4 w-4" />
              New
            </Button>
            <Button
              variant="outline"
              onClick={handleExportConversation}
              size="sm"
              className="gap-1.5"
            >
              <Download className="h-4 w-4" />
              Export
            </Button>
            <Button variant="ghost" size="icon" className="h-8 w-8">
              <Settings2 className="h-4 w-4" />
            </Button>
          </div>
        </div>
      </CardHeader>
    </Card>
  );
};

export default ConversationHeader;
