import { useForgeStore } from "@/stores/ForgeStore";
import { useProjectStore } from "@/stores/ProjectStore";
import ProjectSelectionView from "@/components/ProjectSelectionView";
import ConversationHeader from "@/components/ConversationHeader";
import ModeSwitcher from "@/components/ModeSwitcher";
import DocumentView from "@/components/DocumentView";
import ToolConsoleView from "@/components/ToolConsoleView";
import MessageInput from "@/components/MessageInput";
import StatusBar from "@/components/StatusBar";
import FloatingDirectoryView from "@/components/FloatingDirectoryView";
import FileViewer from "@/components/FileViewerModal";
import { TooltipProvider } from "@/components/ui/tooltip";
import { ThemeProvider } from "next-themes";
import { Button } from "@/components/ui/button";
import { PanelLeft, Loader2 } from "lucide-react";
import { useEffect } from "react";

// Component for the loading screen
const LoadingScreen: React.FC = () => (
  <div className="h-screen w-full flex flex-col items-center justify-center">
    <Loader2 className="h-12 w-12 animate-spin text-primary mb-4" />
    <p className="text-muted-foreground">Loading...</p>
  </div>
);

// Component for the chat interface
const ChatInterface: React.FC = () => {
  useEffect(() => {
    const forgeStore = useForgeStore.getState();
    if (!forgeStore.listenersInitialized) {
      forgeStore.setupListeners();
    }
  }, []);

  return (
    <div className="flex flex-col h-screen w-full overflow-hidden bg-background text-foreground antialiased">
      <div className="sticky top-0 z-10">
        <ConversationHeader />
      </div>

      <div className="flex-1 relative">
        {/* Main Content */}
        <div className="flex flex-col h-full relative">
          <div className="sticky top-0 z-10 bg-background/95 backdrop-blur-sm border-b border-border/50 flex items-center">
            <Button
              variant="ghost"
              size="icon"
              className="p-2 mr-1 hover:bg-accent/50 rounded-md"
              onClick={() => {
                // Toggle directory view visibility
                const currentVisibility = localStorage.getItem("directoryViewVisible");
                const newVisibility = currentVisibility === "false" ? "true" : "false";
                localStorage.setItem("directoryViewVisible", newVisibility);
                window.location.reload();
              }}
              title="Show directory panel"
            >
              <PanelLeft className="h-5 w-5 text-muted-foreground" />
            </Button>
            <ModeSwitcher />
          </div>
          <div className="flex-1 overflow-hidden relative">
            <DocumentView />
          </div>
          <MessageInput />
        </div>

        {/* Fixed Directory View */}
        <FloatingDirectoryView />
      </div>

      {/* Tool Console above Status Bar */}
      <div className="relative">
        <ToolConsoleView />
      </div>
      
      <StatusBar />
    </div>
  );
};

// Main app wrapper with conditional rendering
const AppContent: React.FC = () => {
  const { currentProject, isLoading } = useProjectStore();

  if (isLoading) {
    return <LoadingScreen />;
  }

  if (!currentProject) {
    return <ProjectSelectionView />;
  }

  return <ChatInterface />;
};

function App() {
  return (
    <ThemeProvider attribute="class">
      <TooltipProvider>
        <AppContent />
        <FileViewer />
      </TooltipProvider>
    </ThemeProvider>
  );
}

export default App;