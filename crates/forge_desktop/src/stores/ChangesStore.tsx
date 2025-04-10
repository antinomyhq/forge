import { create } from "zustand";
import { persist } from "zustand/middleware";
import {
  DiffJsonData,
  FileChange,
  FileChangeType,
  tryParseDiffJson,
  calculateDiffStats,
} from "@/utils/diffUtils";

interface ChangesState {
  changes: FileChange[];
  addChange: (change: FileChange) => void;
  updateChange: (path: string, partial: Partial<FileChange>) => void;
  removeChange: (path: string) => void;
  clearChanges: () => void;
  getChangeByPath: (path: string) => FileChange | undefined;
  addChangeFromDiff: (diffData: DiffJsonData) => void;
  addChangeFromToolResult: (toolName: string, result: string) => boolean;
}

export const useChangesStore = create<ChangesState>()(
  persist(
    (set, get) => ({
      changes: [],

      addChange: (change: FileChange) =>
        set((state) => {
          // Check if we already have this file
          const existingIndex = state.changes.findIndex(
            (c) => c.path === change.path,
          );

          if (existingIndex >= 0) {
            // Update existing change
            const updatedChanges = [...state.changes];
            updatedChanges[existingIndex] = {
              ...updatedChanges[existingIndex],
              ...change,
              lastModified: new Date(),
            };
            return { changes: updatedChanges };
          }

          // Add new change
          return {
            changes: [
              ...state.changes,
              { ...change, lastModified: new Date() },
            ],
          };
        }),

      updateChange: (path: string, partial: Partial<FileChange>) =>
        set((state) => {
          const existingIndex = state.changes.findIndex((c) => c.path === path);

          if (existingIndex < 0) {
            return state;
          }

          const updatedChanges = [...state.changes];
          updatedChanges[existingIndex] = {
            ...updatedChanges[existingIndex],
            ...partial,
            lastModified: new Date(),
          };

          return { changes: updatedChanges };
        }),

      removeChange: (path: string) =>
        set((state) => ({
          changes: state.changes.filter((c) => c.path !== path),
        })),

      clearChanges: () => set({ changes: [] }),

      getChangeByPath: (path: string) => {
        return get().changes.find((c) => c.path === path);
      },

      addChangeFromDiff: (diffData: DiffJsonData) => {
        const changeType: FileChangeType = diffData.op_name.includes("create")
          ? "added"
          : diffData.op_name.includes("remove")
            ? "deleted"
            : "modified";

        const { additions, deletions } = calculateDiffStats(diffData);

        const change: FileChange = {
          path: diffData.path,
          type: changeType,
          additions,
          deletions,
          lastModified: new Date(),
          diffData,
        };

        get().addChange(change);
      },

      addChangeFromToolResult: (_toolName: string, result: string): boolean => {
        // Check if the result is a JSON diff
        const diffData = tryParseDiffJson(result);

        if (diffData && diffData.has_changes) {
          get().addChangeFromDiff(diffData);
          return true;
        }

        // For non-JSON diff formats, we could try to parse them in the future
        return false;
      },
    }),
    {
      name: "forge-file-changes",
    },
  ),
);
