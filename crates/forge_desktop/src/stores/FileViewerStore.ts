import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

export interface FileViewerState {
  // Modal state
  isOpen: boolean;
  filePath: string | null;
  fileName: string | null;
  selectedLine: number | null; // Line to highlight

  // Content state
  content: string | null;
  language: string | null;
  isLoading: boolean;
  error: string | null;

  // File size handling
  maxFileSize: number; // in bytes, 0 means no limit

  // Actions
  openFile: (filePath: string) => Promise<void>;
  closeViewer: () => void;
  setError: (error: string | null) => void;
}

// Helper to extract file extension from path
export const getFileExtension = (path: string): string => {
  const parts = path.split(".");
  return parts.length > 1 ? parts.pop()?.toLowerCase() || "" : "";
};

// Helper to extract file name from path
export const getFileName = (path: string): string => {
  // Handle both forward and backward slashes for cross-platform support
  const normalizedPath = path.replace(/\\/g, "/");
  const parts = normalizedPath.split("/");
  return parts.pop() || path;
};

// Helper to determine language for syntax highlighting based on file extension
export const determineLanguage = (extension: string): string => {
  // Map file extensions to language identifiers used by react-syntax-highlighter
  const extensionMap: Record<string, string> = {
    // Web
    html: "html",
    css: "css",
    js: "javascript",
    jsx: "jsx",
    ts: "typescript",
    tsx: "tsx",

    // Programming
    py: "python",
    rb: "ruby",
    rs: "rust",
    go: "go",
    java: "java",
    cpp: "cpp",
    c: "c",
    cs: "csharp",
    php: "php",
    swift: "swift",
    kt: "kotlin",

    // Data/Config
    json: "json",
    yml: "yaml",
    yaml: "yaml",
    toml: "toml",
    xml: "xml",
    md: "markdown",
    sql: "sql",

    // Shell
    sh: "bash",
    bash: "bash",
    bat: "batch",
    ps1: "powershell",

    // Others
    txt: "text",
    log: "text",
    csv: "text",
    dockerfile: "dockerfile",
  };

  return extensionMap[extension] || "text";
};

export const useFileViewerStore = create<FileViewerState>((set, get) => ({
  // Initial state
  isOpen: false,
  filePath: null,
  fileName: null,
  selectedLine: null,
  content: null,
  language: null,
  isLoading: false,
  error: null,
  maxFileSize: 5 * 1024 * 1024, // 5MB default limit

  // Actions
  openFile: async (filePath) => {
    try {
      // Check if the path contains a line number reference
      const lineMatch = filePath.match(/:([0-9]+)$/);
      let selectedLine = null;
      let cleanPath = filePath;

      // Extract the line number if it exists
      if (lineMatch && lineMatch[1]) {
        selectedLine = parseInt(lineMatch[1], 10);
        cleanPath = filePath.replace(lineMatch[0], "");
      }

      set({
        isOpen: true,
        filePath: cleanPath,
        fileName: getFileName(cleanPath),
        selectedLine,
        isLoading: true,
        error: null,
        content: null,
        language: null,
      });

      // First check if the file exists
      const exists = await invoke("file_exists", { path: cleanPath });

      if (!exists) {
        throw new Error(`File does not exist: ${cleanPath}`);
      }

      // Get file info to determine language
      const fileInfo = await invoke<{ extension: string; size: number }>(
        "get_file_info",
        { path: cleanPath },
      );

      // Check file size against limit
      if (get().maxFileSize > 0 && fileInfo.size > get().maxFileSize) {
        throw new Error(
          `File is too large (${Math.round(fileInfo.size / 1024 / 1024)}MB). Maximum size is ${Math.round(get().maxFileSize / 1024 / 1024)}MB.`,
        );
      }

      const language = determineLanguage(fileInfo.extension);

      // Read file content using our Rust command
      const content = await invoke<string>("read_file_content", {
        path: cleanPath,
      });

      set({
        content,
        language,
        isLoading: false,
      });
    } catch (error) {
      console.error("Failed to load file:", error);
      set({
        error: `Failed to load file: ${error instanceof Error ? error.message : String(error)}`,
        isLoading: false,
      });
    }
  },

  closeViewer: () => {
    set({
      isOpen: false,
      // We don't clear content immediately to avoid UI flickers if the same file is reopened
    });
  },

  setError: (error) => {
    set({ error });
  },
}));
