import { useForgeStore } from "@/stores/ForgeStore";
import { useProjectStore } from "@/stores/ProjectStore";
import { useDirectoryStore } from "@/stores/DirectoryStore";
import ProjectSelectionView from "@/components/ProjectSelectionView";
import ConversationHeader from "@/components/ConversationHeader";
import ModeSwitcher from "@/components/ModeSwitcher";
import DocumentView from "@/components/DocumentView";
import ToolConsoleView from "@/components/ToolConsoleView";
import MessageInput from "@/components/MessageInput";
import StatusBar from "@/components/StatusBar";
import DirectoryView from "@/components/DirectoryView";
import FileViewer from "@/components/FileViewerModal";
import { TooltipProvider } from "@/components/ui/tooltip";
import { ThemeProvider, useTheme } from "next-themes";
import { PanelLeft } from "lucide-react";
import { useEffect, useState } from "react";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable";
import { Button } from "@/components/ui/button";
import { ClerkProvider, SignedIn, SignedOut } from "@clerk/clerk-react";
import { dark } from "@clerk/themes";
import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";
import { Toaster } from 'sonner';

import { LoginPage } from "./components/Login";
import { SignUpPage } from "./components/SignUp";
import { InvitationPage } from "./components/Invitation";
import { LoadingScreen } from "./components/LoadingScreen";
import { InvitedOnly } from "./components/InvitedOnly";

// Component for the chat interface
const ChatInterface: React.FC = () => {
  useEffect(() => {
    const forgeStore = useForgeStore.getState();
    if (!forgeStore.listenersInitialized) {
      forgeStore.setupListeners();
    }
  }, []);

  // Initialize sizes from localStorage or use defaults
  const [directorySize, setDirectorySize] = useState(() => {
    return Number(localStorage.getItem("directorySize")) || 20;
  });
  const [toolConsoleSize, setToolConsoleSize] = useState(() => {
    return Number(localStorage.getItem("toolConsoleSize")) || 25;
  });

  const { isVisible, toggleVisible } = useDirectoryStore();

  // Persist sizes to localStorage
  useEffect(() => {
    localStorage.setItem("directorySize", directorySize.toString());
  }, [directorySize]);

  useEffect(() => {
    localStorage.setItem("toolConsoleSize", toolConsoleSize.toString());
  }, [toolConsoleSize]);

  return (
    <div className="flex flex-col h-screen w-full overflow-hidden bg-background text-foreground antialiased">
      <div className="sticky top-0 z-10">
        <ConversationHeader />
      </div>
          <div className="flex flex-col h-full">
            <div className="sticky top-0 z-10 bg-background/95 backdrop-blur-sm border-b border-border/50 flex items-center">
              <ModeSwitcher />
            </div>
            <div className="flex-1 overflow-hidden relative">
              <DocumentView />
            </div>
            <MessageInput />
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

// Wrap the Clerk provider with theme awareness
const ThemedClerkProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const { theme, resolvedTheme, forcedTheme } = useTheme();
  const currentTheme = theme || resolvedTheme || forcedTheme;

  return (
    <ClerkProvider
      afterSignOutUrl="/sign-in"
      signUpUrl="/sign-up"
      signInUrl="/sign-in"
      signInForceRedirectUrl="/"
      signUpForceRedirectUrl="/"
      appearance={{
        baseTheme: currentTheme === "dark" ? dark : undefined,
      }}
      publishableKey={import.meta.env.VITE_CLERK_PUBLISHABLE_KEY}>
      {children}
    </ClerkProvider>
  );
};

function App() {
  return (
    <ThemeProvider attribute="class" defaultTheme="system" enableSystem={true} storageKey="ui-theme">
      <ThemedClerkProvider>
        <TooltipProvider>
          <BrowserRouter>
            <SignedOut>
              <Routes>
                <Route path="/sign-in" element={<LoginPage />} />
                <Route path="/sign-up" element={<SignUpPage />} />
                <Route path="/*" element={<Navigate to="/sign-in" replace />} />
              </Routes>
            </SignedOut>
            <SignedIn>
              <Routes>
                <Route path="/*" element={<InvitedOnly><FileViewer /><AppContent /></InvitedOnly>} />
                <Route path="/invitation" element={<InvitationPage />} />
              </Routes>
            </SignedIn>
          </BrowserRouter>
        </TooltipProvider>
        <Toaster richColors position="top-right" />
      </ThemedClerkProvider>
    </ThemeProvider>
  );
}

export default App;
