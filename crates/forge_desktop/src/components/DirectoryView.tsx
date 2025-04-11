import React, { useEffect, useState } from "react";
import {
  FolderOpen,
  AlertCircle,
  Loader2,
  Files,
  GitBranch,
} from "lucide-react";
import { useDirectoryStore } from "@/stores/DirectoryStore";
import { useProjectStore } from "@/stores/ProjectStore";
import { useChangesStore } from "@/stores/ChangesStore";
import TreeItem from "./TreeItem";
import { FileChangesBrowser } from "./FileChangesBrowser";
import { ScrollArea } from "./ui/scroll-area";
import { Button } from "./ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "./ui/tabs";
import { DiffModalView } from "./DiffModalView";
import { Badge } from "./ui/badge";

// DirectoryView is now used within FloatingDirectoryView
const DirectoryView: React.FC = () => {
  const { directoryTree, isLoading, error, loadDirectoryStructure } =
    useDirectoryStore();
  const { currentProject } = useProjectStore();
  const { changes } = useChangesStore();

  const [selectedTab, setSelectedTab] = useState<string>("files");
  const [selectedChange, setSelectedChange] = useState<string | null>(null);
  const [showDiffModal, setShowDiffModal] = useState(false);

  const selectedDiffData = useChangesStore((state) => {
    if (!selectedChange) return null;
    const change = state.getChangeByPath(selectedChange);
    return change?.diffData || null;
  });

  // Load directory structure when the component mounts or when project changes
  useEffect(() => {
    if (currentProject && currentProject.path) {
      loadDirectoryStructure(currentProject.path);
    }
  }, [currentProject?.path, loadDirectoryStructure]);

  // Switch to changes tab when changes are detected
  useEffect(() => {
    if (changes.length > 0) {
      setSelectedTab("changes");
    }
  }, [changes.length === 1]); // Only switch automatically on first change

  // Auto-switching logic could be further refined based on UX requirements

  // Render empty state if no project is selected
  if (!currentProject) {
    return (
      <div className="flex flex-col h-full">
        <div className="flex-1 flex items-center justify-center text-muted-foreground text-sm p-4">
          <div className="text-center">
            <FolderOpen className="h-8 w-8 mx-auto mb-2 opacity-50" />
            <p>No project selected</p>
          </div>
        </div>
      </div>
    );
  }

  // Render loading state
  if (isLoading && selectedTab === "files") {
    return (
      <div className="flex flex-col h-full">
        <div className="flex-1 flex items-center justify-center text-muted-foreground text-sm p-4">
          <div className="text-center">
            <Loader2 className="h-8 w-8 mx-auto mb-2 animate-spin" />
            <p>Loading directory structure...</p>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full overflow-hidden">
      <Tabs
        value={selectedTab}
        onValueChange={setSelectedTab}
        className="flex-1 flex flex-col"
      >
        <TabsList className="w-full rounded-none border-b grid grid-cols-2">
          <TabsTrigger value="files" className="flex items-center gap-1.5">
            <Files className="h-4 w-4" />
            <span>Files</span>
          </TabsTrigger>
          <TabsTrigger value="changes" className="flex items-center gap-1.5">
            <GitBranch className="h-4 w-4" />
            <span>Changes</span>
            {changes.length > 0 && (
              <Badge variant="secondary" className="ml-1 px-1.5 h-5">
                {changes.length}
              </Badge>
            )}
          </TabsTrigger>
        </TabsList>

        <TabsContent value="files" className="flex-1 p-0 m-0">
          {error ? (
            <div className="flex-1 flex items-center justify-center text-destructive text-sm p-4">
              <div className="text-center">
                <AlertCircle className="h-8 w-8 mx-auto mb-2" />
                <p>Error loading files:</p>
                <p className="mt-2 max-w-xs break-words">{error}</p>
              </div>
            </div>
          ) : !directoryTree ? (
            <div className="flex-1 flex items-center justify-center text-muted-foreground text-sm p-4">
              <div className="text-center">
                <FolderOpen className="h-8 w-8 mx-auto mb-2 opacity-50" />
                <p>No files found</p>
              </div>
            </div>
          ) : (
            <ScrollArea className="h-full">
              <div className="p-2">
                <TreeItem item={directoryTree} level={0} />
              </div>
            </ScrollArea>
          )}
        </TabsContent>

        <TabsContent value="changes" className="flex-1 p-0 m-0">
          {changes.length === 0 ? (
            <div className="flex-1 flex items-center justify-center text-muted-foreground text-sm p-4">
              <div className="text-center">
                <GitBranch className="h-8 w-8 mx-auto mb-2 opacity-50" />
                <p>No changes detected</p>
              </div>
            </div>
          ) : (
            <FileChangesBrowser
              onSelectFile={(change) => {
                setSelectedChange(change.path);
                if (change.diffData && change.diffData.has_changes) {
                  setShowDiffModal(true);
                }
              }}
              selectedPath={selectedChange || undefined}
            />
          )}
        </TabsContent>
      </Tabs>

      {/* Diff Modal */}
      {selectedDiffData && (
        <DiffModalView
          open={showDiffModal}
          onOpenChange={setShowDiffModal}
          diffData={selectedDiffData}
        />
      )}
    </div>
  );
};

export default DirectoryView;