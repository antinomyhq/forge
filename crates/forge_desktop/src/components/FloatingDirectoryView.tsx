import React, { useState, useEffect } from "react";
import { X, Minus, Maximize2, GripHorizontal, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import DirectoryView from "@/components/DirectoryView";
import { useDirectoryStore } from "@/stores/DirectoryStore";
import { useProjectStore } from "@/stores/ProjectStore";
import { ScrollArea } from "@/components/ui/scroll-area";

// Default dimensions
const DEFAULT_WIDTH = 320; // Default width in pixels
const MIN_WIDTH = 200; // Minimum width in pixels
const MAX_WIDTH = 600; // Maximum width in pixels

const FloatingDirectoryView: React.FC = () => {
  const { loadDirectoryStructure } = useDirectoryStore();
  const { currentProject } = useProjectStore();
  const [minimized, setMinimized] = useState(false);
  const [visible, setVisible] = useState(true);
  const [width, setWidth] = useState(DEFAULT_WIDTH);
  const [isResizing, setIsResizing] = useState(false);
  const [initialMousePosX, setInitialMousePosX] = useState(0);
  const [initialWidth, setInitialWidth] = useState(DEFAULT_WIDTH);

  // Load saved width and visibility on component mount
  useEffect(() => {
    // Load visibility
    const savedVisibility = localStorage.getItem("directoryViewVisible");
    if (savedVisibility !== null) {
      setVisible(savedVisibility === "true");
    }
    
    // Load width
    const savedWidth = localStorage.getItem("directoryViewWidth");
    if (savedWidth !== null) {
      setWidth(Number(savedWidth));
    }
  }, []);

  // Handle mouse down for resizing
  const handleMouseDown = (e: React.MouseEvent<HTMLDivElement>) => {
    setIsResizing(true);
    setInitialMousePosX(e.clientX);
    setInitialWidth(width);
    e.preventDefault();
  };

  // Handle mouse move during resize
  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (!isResizing) return;
      
      // Calculate new width (since it's right-aligned, we need to subtract)
      let newWidth = initialWidth + (initialMousePosX - e.clientX);
      
      // Apply constraints
      newWidth = Math.max(MIN_WIDTH, Math.min(newWidth, MAX_WIDTH));
      
      // Update width
      setWidth(newWidth);
    };

    const handleMouseUp = () => {
      if (isResizing) {
        setIsResizing(false);
        // Save the new width to localStorage
        localStorage.setItem("directoryViewWidth", width.toString());
      }
    };

    if (isResizing) {
      document.addEventListener('mousemove', handleMouseMove);
      document.addEventListener('mouseup', handleMouseUp);
    }

    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, [isResizing, initialMousePosX, initialWidth, width]);

  // Toggle visibility
  const toggleVisibility = () => {
    const newState = !visible;
    setVisible(newState);
    localStorage.setItem("directoryViewVisible", newState.toString());
  };
  
  // Return button only if not visible
  if (!visible) {
    return (
      <div className="fixed top-[120px] right-0 z-50">
        <Button
          variant="ghost"
          size="sm"
          className="rounded-l-md rounded-r-none border border-border border-r-0 bg-background"
          onClick={toggleVisibility}
        >
          <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M18 20V6a2 2 0 0 0-2-2H8a2 2 0 0 0-2 2v14" />
            <path d="M2 20h20" />
            <path d="M14 12v.01" />
          </svg>
        </Button>
      </div>
    );
  }

  // Function to handle refresh button click
  const handleRefresh = () => {
    if (currentProject && currentProject.path) {
      loadDirectoryStructure(currentProject.path);
    }
  };

  return (
    <div 
      className={`fixed top-[120px] right-0 bg-background border border-border rounded-l-md shadow-md z-50 transition-all duration-200 ease-in-out ${
        minimized ? "h-12 overflow-hidden" : "h-[70vh]"
      }`}
      style={{ 
        maxHeight: minimized ? "48px" : "70vh",
        width: minimized ? "40px" : `${width}px`,
        borderRight: 0
      }}
    >
      {/* Resize handle on the left edge */}
      {!minimized && (
        <div
          className="absolute top-0 left-0 bottom-0 w-1.5 cursor-ew-resize hover:bg-primary/20 z-10 flex items-center justify-center"
          onMouseDown={handleMouseDown}
        >
          <div className="h-10 w-1 flex items-center justify-center opacity-0 hover:opacity-70">
            <GripHorizontal className="rotate-90 h-3.5 w-3.5 text-primary" />
          </div>
        </div>
      )}
      
      {/* Header with controls */}
      <div className="flex items-center justify-between p-3 border-b border-border">
        {!minimized && (
          <div className="font-medium text-sm truncate max-w-[calc(100%-80px)]">Project Explorer</div>
        )}
        <div className={`flex items-center space-x-1 ${minimized ? 'ml-0' : 'ml-auto'}`}>
          {!minimized && (
            <Button
              variant="ghost"
              size="icon"
              className="h-6 w-6 flex-shrink-0"
              onClick={handleRefresh}
              title="Refresh files"  
            >
              <RefreshCw className="h-3 w-3" />
            </Button>
          )}
          <Button
            variant="ghost"
            size="icon"
            className="h-6 w-6 flex-shrink-0"
            onClick={() => setMinimized(!minimized)}
          >
            {minimized ? <Maximize2 className="h-3 w-3" /> : <Minus className="h-3 w-3" />}
          </Button>
          <Button
            variant="ghost" 
            size="icon"
            className="h-6 w-6 flex-shrink-0"
            onClick={toggleVisibility}
          >
            <X className="h-3 w-3" />
          </Button>
        </div>
      </div>
      
      {/* Directory content with ScrollArea */}
      {!minimized && (
        <ScrollArea className="h-[calc(100%-48px)]">
          <div className="p-2 overflow-hidden">
            <DirectoryView />
          </div>
        </ScrollArea>
      )}
    </div>
  );
};

export default FloatingDirectoryView;