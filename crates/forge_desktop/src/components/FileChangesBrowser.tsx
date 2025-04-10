import React, { useState } from "react";
import {
  ChevronDown,
  ChevronRight,
  Plus,
  Minus,
  FileIcon,
  FolderIcon,
} from "lucide-react";
import { useChangesStore } from "@/stores/ChangesStore";
import {
  FileChange,
  FileChangeType,
  groupFileChangesByDirectory,
} from "@/utils/diffUtils";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/utils/utils";
import { Badge } from "@/components/ui/badge";

interface FileChangesBrowserProps {
  onSelectFile?: (file: FileChange) => void;
  selectedPath?: string;
}

const FileChangeItem = ({
  change,
  selected,
  onClick,
}: {
  change: FileChange;
  selected: boolean;
  onClick: () => void;
}) => {
  const getIcon = (type: FileChangeType) => {
    switch (type) {
      case "added":
        return <Plus className="h-4 w-4 text-green-500" />;
      case "deleted":
        return <Minus className="h-4 w-4 text-red-500" />;
      case "renamed":
        return <FileIcon className="h-4 w-4 text-blue-500" />;
      default:
        return <FileIcon className="h-4 w-4 text-yellow-500" />;
    }
  };

  const getFileName = (path: string) => {
    const parts = path.split("/");
    return parts[parts.length - 1];
  };

  return (
    <div
      className={cn(
        "flex items-center py-1 px-2 text-sm cursor-pointer rounded-md hover:bg-muted",
        selected && "bg-muted",
      )}
      onClick={onClick}
    >
      <div className="mr-2">{getIcon(change.type)}</div>
      <span className="flex-grow truncate">{getFileName(change.path)}</span>
      <div className="flex items-center gap-1 text-xs">
        {change.additions > 0 && (
          <span className="text-green-500">+{change.additions}</span>
        )}
        {change.deletions > 0 && (
          <span className="text-red-500">-{change.deletions}</span>
        )}
      </div>
    </div>
  );
};

const DirectoryItem = ({
  name,
  files,
  selectedPath,
  onSelectFile,
}: {
  name: string;
  files: FileChange[];
  selectedPath?: string;
  onSelectFile: (file: FileChange) => void;
}) => {
  const [expanded, setExpanded] = useState(true);

  const toggleExpanded = () => {
    setExpanded(!expanded);
  };

  const getDirectoryName = () => {
    if (name === "/") return "/";
    const parts = name.split("/");
    return parts[parts.length - 1];
  };

  return (
    <div className="directory-item">
      <div
        className="flex items-center py-1 px-2 text-sm cursor-pointer hover:bg-muted rounded-md"
        onClick={toggleExpanded}
      >
        <span className="mr-1">
          {expanded ? (
            <ChevronDown className="h-4 w-4 text-muted-foreground" />
          ) : (
            <ChevronRight className="h-4 w-4 text-muted-foreground" />
          )}
        </span>
        <FolderIcon className="h-4 w-4 mr-2 text-muted-foreground" />
        <span>{getDirectoryName()}</span>
        <Badge variant="outline" className="ml-2 text-xs">
          {files.length}
        </Badge>
      </div>

      {expanded && (
        <div className="pl-6 directory-contents">
          {files.map((file) => (
            <FileChangeItem
              key={file.path}
              change={file}
              selected={selectedPath === file.path}
              onClick={() => onSelectFile(file)}
            />
          ))}
        </div>
      )}
    </div>
  );
};

export const FileChangesBrowser: React.FC<FileChangesBrowserProps> = ({
  onSelectFile,
  selectedPath,
}) => {
  const { changes } = useChangesStore();

  const groupedChanges = groupFileChangesByDirectory(changes);

  // Sort directories by name
  const sortedDirectories = Object.keys(groupedChanges).sort();

  return (
    <div className="changes-browser h-full">
      <ScrollArea className="h-full">
        <div className="p-2">
          {changes.length === 0 ? (
            <div className="text-sm text-muted-foreground p-2 text-center">
              No changes detected
            </div>
          ) : (
            sortedDirectories.map((directory) => (
              <DirectoryItem
                key={directory}
                name={directory}
                files={groupedChanges[directory]}
                selectedPath={selectedPath}
                onSelectFile={onSelectFile || (() => {})}
              />
            ))
          )}
        </div>
      </ScrollArea>
    </div>
  );
};
