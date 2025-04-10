import React from "react";
import {
  ChevronDown,
  ChevronRight,
  File as FileIcon,
  Folder as FolderIcon,
  FileText,
  Image as ImageIcon,
  FileCode,
  Package,
} from "lucide-react";
import { FileSystemEntry, useDirectoryStore } from "@/stores/DirectoryStore";
import { useFileViewerStore } from "@/stores/FileViewerStore";
import { cn } from "@/utils/utils";

interface TreeItemProps {
  item: FileSystemEntry;
  level: number;
}

// Helper function to determine which icon to use based on file extension
const getFileIcon = (filename: string) => {
  const extension = filename.split(".").pop()?.toLowerCase();

  // Common file types
  if (extension === undefined)
    return <FileIcon className="h-4 w-4 text-gray-500" />;

  if (["jpg", "jpeg", "png", "gif", "svg", "webp"].includes(extension)) {
    return <ImageIcon className="h-4 w-4 text-purple-500" />;
  }

  if (
    [
      "js",
      "jsx",
      "ts",
      "tsx",
      "py",
      "rb",
      "java",
      "c",
      "cpp",
      "rs",
      "go",
      "php",
    ].includes(extension)
  ) {
    return <FileCode className="h-4 w-4 text-yellow-500" />;
  }

  if (
    ["json", "yaml", "yml", "toml", "xml", "conf", "ini"].includes(extension)
  ) {
    return <Package className="h-4 w-4 text-green-500" />;
  }

  // Default file icon
  return <FileText className="h-4 w-4 text-gray-500" />;
};

const TreeItem: React.FC<TreeItemProps> = ({ item, level }) => {
  const { expandedPaths, toggleExpandPath } = useDirectoryStore();
  const { openFile } = useFileViewerStore();

  const isExpanded = item.is_directory && expandedPaths.has(item.path);
  const hasChildren = item.children && item.children.length > 0;

  // Calculate indentation padding
  const paddingLeft = level * 16; // 16px per level

  // Handle file click - open in viewer for files, toggle expansion for directories
  const handleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (item.is_directory) {
      toggleExpandPath(item.path);
    } else {
      // Open file in the FileViewer modal
      openFile(item.path);
    }
  };

  // Drag start handler for file items
  const handleDragStart = (e: React.DragEvent) => {
    console.log("drag start");
    // Only set drag data for non-directory items
    if (!item.is_directory) {
      e.dataTransfer.setData("text/plain", item.path);
      e.dataTransfer.effectAllowed = "copy";
    }
  };

  return (
    <div>
      {/* Item row */}
      <div
        className={cn(
          "flex items-center py-1 px-2 hover:bg-accent/50 rounded-md cursor-pointer",
          "transition-colors duration-100 text-sm",
          !item.is_directory && "cursor-grab",
        )}
        onClick={handleClick}
        style={{ paddingLeft: `${paddingLeft}px` }}
        draggable={!item.is_directory}
        onDragStart={handleDragStart}
      >
        {/* Expand/collapse indicator */}
        <div className="w-5 flex items-center justify-center mr-1">
          {item.is_directory && hasChildren ? (
            isExpanded ? (
              <ChevronDown className="h-4 w-4 text-muted-foreground" />
            ) : (
              <ChevronRight className="h-4 w-4 text-muted-foreground" />
            )
          ) : (
            <span className="w-4" /> // Empty placeholder for alignment
          )}
        </div>

        {/* File/folder icon */}
        <div className="flex items-center mr-2">
          {item.is_directory ? (
            <FolderIcon className="h-4 w-4 text-blue-500" />
          ) : (
            getFileIcon(item.name)
          )}
        </div>

        {/* File/folder name with truncation */}
        <div className="truncate">{item.name}</div>
      </div>

      {/* Children */}
      {isExpanded && item.children && (
        <div>
          {item.children.map((child) => (
            <TreeItem key={child.path} item={child} level={level + 1} />
          ))}
        </div>
      )}
    </div>
  );
};

export default TreeItem;
