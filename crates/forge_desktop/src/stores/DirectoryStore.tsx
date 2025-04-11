// DirectoryStore.tsx - Zustand implementation for directory tree management
import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import { enableMapSet } from "immer";
import { persist } from "zustand/middleware";
import { invoke } from "@tauri-apps/api/core";

// Enable MapSet to support Set objects in Immer
enableMapSet();

// Type definitions for file system entries
export interface FileSystemEntry {
  name: string;
  path: string;
  is_directory: boolean;
  children?: FileSystemEntry[];
}

interface DirectoryState {
  // State
  directoryTree: FileSystemEntry | null;
  isLoading: boolean;
  isVisible: boolean;
  panelWidth: number;
  expandedPaths: Set<string>;
  error: string | null;

  // Actions
  setDirectoryTree: (tree: FileSystemEntry | null) => void;
  setLoading: (isLoading: boolean) => void;
  setVisible: (isVisible: boolean) => void;
  toggleVisible: () => void;
  setPanelWidth: (width: number) => void;
  toggleExpandPath: (path: string) => void;
  setExpandedPath: (path: string, isExpanded: boolean) => void;
  setError: (error: string | null) => void;

  // Operations
  loadDirectoryStructure: (path: string, depth?: number) => Promise<void>;
}

// Create the Zustand store with immer and persist middleware
export const useDirectoryStore = create<DirectoryState>()(
  persist(
    immer((set) => ({
      // Initial state
      directoryTree: null,
      isLoading: false,
      isVisible: true, // Visible by default
      panelWidth: 25, // Default width in pixels
      expandedPaths: new Set<string>(), // Set of expanded directory paths
      error: null,

      // Actions for updating state
      setDirectoryTree: (tree) =>
        set((state) => {
          state.directoryTree = tree;
        }),

      setLoading: (isLoading) =>
        set((state) => {
          state.isLoading = isLoading;
        }),

      setVisible: (isVisible) =>
        set((state) => {
          state.isVisible = isVisible;
        }),

      toggleVisible: () =>
        set((state) => {
          state.isVisible = !state.isVisible;
        }),

      setPanelWidth: (width) =>
        set((state) => {
          state.panelWidth = width;
        }),

      toggleExpandPath: (path) =>
        set((state) => {
          if (state.expandedPaths.has(path)) {
            state.expandedPaths.delete(path);
          } else {
            state.expandedPaths.add(path);
          }
        }),

      setExpandedPath: (path, isExpanded) =>
        set((state) => {
          if (isExpanded) {
            state.expandedPaths.add(path);
          } else {
            state.expandedPaths.delete(path);
          }
        }),

      setError: (error) =>
        set((state) => {
          state.error = error;
        }),

      // API operations
      loadDirectoryStructure: async (path, depth = 5) => {
        try {
          set((state) => {
            state.isLoading = true;
            state.error = null;
          });

          console.log(
            "Getting directory structure for:",
            path,
            "with depth:",
            depth,
          );
          const tree = await invoke<FileSystemEntry>(
            "get_directory_structure",
            {
              path,
              depth,
            },
          );
          console.log("Received directory structure:", tree);

          // For the root directory, auto-expand it
          set((state) => {
            state.directoryTree = tree;
            state.isLoading = false;
            if (tree) {
              state.expandedPaths.add(tree.path);
            }
          });
        } catch (err) {
          console.error("Error loading directory structure:", err);
          set((state) => {
            state.error = err instanceof Error ? err.message : String(err);
            state.isLoading = false;
          });
        }
      },
    })),
    {
      name: "forge-directory-storage",
      // Only persist these fields
      partialize: (state) => ({
        isVisible: true, // Always persist as true for storage
        panelWidth: state.panelWidth,
        // Convert Set to array for storage and back
        expandedPaths: Array.from(state.expandedPaths),
      }),
      // Migrate storage if needed in the future
      version: 1,
      // Custom merger to handle Set conversion back from storage
      merge: (persistedState, currentState) => {
        const typedPersistedState = persistedState as any;
        return {
          ...currentState,
          ...typedPersistedState,
          // Convert array back to Set after retrieving from storage
          expandedPaths: new Set(typedPersistedState.expandedPaths || []),
        };
      },
    },
  ),
);
