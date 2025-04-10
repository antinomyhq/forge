// ProjectStore.tsx - Zustand implementation for project management
import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useDirectoryStore } from "./DirectoryStore";

// Type definitions for projects
export interface ProjectInfo {
  path: string;
  name: string;
  lastAccessed: string; // ISO timestamp
}

interface ProjectState {
  // State
  currentProject: ProjectInfo | null;
  recentProjects: ProjectInfo[];
  isLoading: boolean;
  error: string | null;

  // Actions
  setCurrentProject: (project: ProjectInfo | null) => void;
  setRecentProjects: (projects: ProjectInfo[]) => void;
  setLoading: (isLoading: boolean) => void;
  setError: (error: string | null) => void;

  // Operations
  refreshProjects: () => Promise<void>;
  selectProject: (path?: string) => Promise<void>;
  switchProject: (projectInfo: ProjectInfo) => Promise<void>;
  createNewProject: (path: string, name?: string) => Promise<void>;
  closeProject: () => Promise<void>;
  openDirectoryDialog: () => Promise<string | undefined>;
  initialize: () => Promise<void>;
}

// Create the Zustand store with immer middleware for easier state updates
export const useProjectStore = create<ProjectState>()(
  immer((set, get) => ({
    // Initial state
    currentProject: null,
    recentProjects: [],
    isLoading: true,
    error: null,

    // Actions for updating state
    setCurrentProject: (project) =>
      set((state) => {
        state.currentProject = project;
      }),

    setRecentProjects: (projects) =>
      set((state) => {
        state.recentProjects = projects;
      }),

    setLoading: (isLoading) =>
      set((state) => {
        state.isLoading = isLoading;
      }),

    setError: (error) =>
      set((state) => {
        state.error = error;
      }),

    // API operations
    refreshProjects: async () => {
      try {
        set((state) => {
          state.isLoading = true;
          state.error = null;
        });

        // Get current project
        console.log("Getting current project information");
        const project = await invoke<ProjectInfo | null>("get_current_project");
        console.log("Current project from backend:", project);

        // Get recent projects
        console.log("Getting recent projects list");
        const projects = await invoke<ProjectInfo[]>("get_recent_projects");
        console.log("Received recent projects:", projects.length);

        set((state) => {
          state.currentProject = project;
          state.recentProjects = projects;
          state.isLoading = false;
        });
      } catch (err) {
        console.error("Error refreshing projects:", err);
        set((state) => {
          state.error = err instanceof Error ? err.message : String(err);
          state.isLoading = false;
        });
      }
    },

    selectProject: async (path?: string) => {
      try {
        set((state) => {
          state.isLoading = true;
          state.error = null;
        });

        // If no path provided, open directory dialog
        const directoryPath = path || (await get().openDirectoryDialog());

        if (!directoryPath) {
          set((state) => {
            state.isLoading = false;
          });
          console.log("Project selection cancelled by user");
          return; // User cancelled
        }

        console.log("Selecting project at path:", directoryPath);
        // Select the project
        const projectInfo = await invoke<ProjectInfo>("select_project", {
          path: directoryPath,
          name: undefined, // Let backend determine name from path
        });
        console.log("Project selected successfully:", projectInfo);

        set((state) => {
          state.currentProject = projectInfo;
        });
        await get().refreshProjects(); // Refresh recent projects list
      } catch (err) {
        console.error("Error selecting project:", err);
        set((state) => {
          state.error = err instanceof Error ? err.message : String(err);
          state.isLoading = false;
        });
        throw err; // Re-throw for caller to handle
      }
    },

    switchProject: async (projectInfo) => {
      try {
        set((state) => {
          state.isLoading = true;
          state.error = null;
        });
        console.log(
          "Switching to project:",
          projectInfo.name,
          "at path:",
          projectInfo.path,
        );

        // Switch to the project
        const updatedProject = await invoke<ProjectInfo>("switch_project", {
          path: projectInfo.path,
        });
        console.log(
          "Project switched successfully, updated info:",
          updatedProject,
        );

        set((state) => {
          state.currentProject = updatedProject;
        });
        await get().refreshProjects(); // Refresh recent projects list
      } catch (err) {
        console.error("Error switching project:", err);
        set((state) => {
          state.error = err instanceof Error ? err.message : String(err);
          state.isLoading = false;
        });
        throw err;
      }
    },

    createNewProject: async (path, name) => {
      try {
        set((state) => {
          state.isLoading = true;
          state.error = null;
        });

        // Create the project
        const projectInfo = await invoke<ProjectInfo>("create_new_project", {
          path,
          name,
        });

        set((state) => {
          state.currentProject = projectInfo;
        });
        await get().refreshProjects(); // Refresh recent projects list
      } catch (err) {
        set((state) => {
          state.error = err instanceof Error ? err.message : String(err);
          state.isLoading = false;
        });
        throw err;
      }
    },

    closeProject: async () => {
      try {
        set((state) => {
          state.isLoading = true;
          state.error = null;
        });

        // Close the project
        await invoke("close_current_project");

        set((state) => {
          state.currentProject = null;
        });
        await get().refreshProjects(); // Refresh recent projects list
      } catch (err) {
        set((state) => {
          state.error = err instanceof Error ? err.message : String(err);
          state.isLoading = false;
        });
        throw err;
      }
    },

    openDirectoryDialog: async () => {
      try {
        // Open directory selection dialog
        const result = await open({
          directory: true,
          multiple: false,
          title: "Select Project Directory",
        });

        // Return the selected directory or undefined if cancelled
        return typeof result === "string" ? result : undefined;
      } catch (err) {
        set((state) => {
          state.error = err instanceof Error ? err.message : String(err);
        });
        return undefined;
      }
    },

    // Initialize the store state
    initialize: async () => {
      try {
        set((state) => {
          state.isLoading = true;
        });
        console.log("Loading projects state...");
        await invoke("load_projects_state");
        console.log("Refreshing projects...");
        await get().refreshProjects();

        // Get the current project after loading state
        const currentProject = get().currentProject;
        console.log("Current project after refresh:", currentProject);

        // If a project is loaded, ensure its working directory is set
        if (currentProject) {
          console.log("Updating working directory to:", currentProject.path);
          // Update the cwd in the backend
          await invoke("update_cwd", { cwd: currentProject.path });
          console.log("Working directory updated successfully");
        } else {
          console.log("No project currently selected");
        }

        set((state) => {
          state.isLoading = false;
        });
      } catch (err) {
        console.error("Error initializing project store:", err);
        set((state) => {
          state.error = err instanceof Error ? err.message : String(err);
          state.isLoading = false;
        });
      }
    },
  })),
);

// Initialize the store when this module is imported
// This will be executed once when the module is first loaded
useProjectStore.getState().initialize();

// Setup a listener to load directory structure when project changes
const setupDirectoryUpdates = () => {
  // Get the directoryStore
  const directoryStore = useDirectoryStore.getState();

  // Listen for changes to currentProject in ProjectStore
  useProjectStore.subscribe((state) => {
    // If a project is selected, load its directory structure
    if (state.currentProject && state.currentProject.path) {
      console.log(
        "Project changed, loading directory structure:",
        state.currentProject.path,
      );
      directoryStore.loadDirectoryStructure(state.currentProject.path);
    }
  });
};

// Call setup function
setupDirectoryUpdates();
