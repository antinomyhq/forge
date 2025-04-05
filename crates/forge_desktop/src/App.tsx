import { ForgeProvider } from "@/contexts/ForgeContext";
import ConversationHeader from "@/components/ConversationHeader";
import ModeSwitcher from "@/components/ModeSwitcher";
import ChatView from "@/components/ChatView";
import MessageInput from "@/components/MessageInput";
import StatusBar from "@/components/StatusBar";
import { TooltipProvider } from "@/components/ui/tooltip";

function App() {
  return (
    <ForgeProvider>
      <TooltipProvider>
        <div className="flex flex-col h-screen w-full overflow-hidden bg-background font-sans text-foreground antialiased">
          <div className="sticky top-0 z-10">
            <ConversationHeader />
          </div>
          <div className="flex-1 flex flex-col overflow-hidden border-x border-border/40 mx-auto w-full max-w-6xl shadow-sm">
            <div className="sticky top-0 z-10 bg-background/95 backdrop-blur-sm border-b border-border/50">
              <ModeSwitcher />
            </div>
            <div className="flex-1 overflow-hidden relative">
              <ChatView />
            </div>
            <MessageInput />
          </div>
          <StatusBar />
        </div>
      </TooltipProvider>
    </ForgeProvider>
  );
}

export default App;