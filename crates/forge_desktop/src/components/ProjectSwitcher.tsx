import React, { useState } from "react";
import { useProjectStore, ProjectInfo } from "@/stores/ProjectStore";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  FolderOpen,
  FolderPlus,
  ChevronDown,
  ArrowLeft,
  CheckCircle2,
} from "lucide-react";

// Component for a new project dialog
const NewProjectDialog: React.FC<{
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCreateProject: (path: string, name?: string) => void;
}> = ({ open, onOpenChange, onCreateProject }) => {
  const { openDirectoryDialog } = useProjectStore();
  const [projectPath, setProjectPath] = useState<string>("");
  const [projectName, setProjectName] = useState<string>("");

  const handleSelectLocation = async () => {
    const path = await openDirectoryDialog();
    if (path) {
      setProjectPath(path);

      // Extract folder name as default project name
      const parts = path.split(/[/\\]/);
      const folderName = parts[parts.length - 1];
      if (folderName && !projectName) {
        setProjectName(folderName);
      }
    }
  };

  const handleCreateProject = () => {
    if (projectPath) {
      onCreateProject(projectPath, projectName || undefined);
      onOpenChange(false);

      // Reset form
      setProjectPath("");
      setProjectName("");
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Create New Project</DialogTitle>
          <DialogDescription>
            Create a new project directory or select an existing one.
          </DialogDescription>
        </DialogHeader>

        <div className="grid gap-4 py-4">
          <div className="flex flex-col gap-2">
            <label htmlFor="project-path" className="text-sm font-medium">
              Project Location
            </label>
            <div className="flex gap-2">
              <Input
                id="project-path"
                value={projectPath}
                onChange={(e) => setProjectPath(e.target.value)}
                placeholder="/path/to/project"
                className="flex-1"
              />
              <Button
                variant="outline"
                size="icon"
                onClick={handleSelectLocation}
                type="button"
              >
                <FolderOpen className="h-4 w-4" />
              </Button>
            </div>
          </div>

          <div className="flex flex-col gap-2">
            <label htmlFor="project-name" className="text-sm font-medium">
              Project Name (Optional)
            </label>
            <Input
              id="project-name"
              value={projectName}
              onChange={(e) => setProjectName(e.target.value)}
              placeholder="My Project"
            />
          </div>
        </div>

        <DialogFooter>
          <Button
            type="submit"
            disabled={!projectPath}
            onClick={handleCreateProject}
          >
            Create Project
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};

// Main project switcher component
const ProjectSwitcher: React.FC = () => {
  const {
    currentProject,
    recentProjects,
    switchProject,
    closeProject,
    createNewProject,
    isLoading,
  } = useProjectStore();
  const [isNewProjectDialogOpen, setIsNewProjectDialogOpen] = useState(false);

  const handleSwitchProject = async (project: ProjectInfo) => {
    if (isLoading) return;

    // Don't do anything if this is already the current project
    if (currentProject?.path === project.path) return;

    try {
      await switchProject(project);
    } catch (err) {
      console.error("Failed to switch project:", err);
    }
  };

  const handleCreateProject = async (path: string, name?: string) => {
    if (isLoading) return;
    try {
      await createNewProject(path, name);
    } catch (err) {
      console.error("Failed to create project:", err);
    }
  };

  const handleCloseProject = async () => {
    if (isLoading) return;
    try {
      await closeProject();
    } catch (err) {
      console.error("Failed to close project:", err);
    }
  };

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="outline"
            size="sm"
            className="gap-2 h-8 px-2 max-w-[220px]"
            disabled={isLoading}
          >
            <div className="truncate">
              {currentProject?.name || "Select Project"}
            </div>
            <ChevronDown className="h-4 w-4 ml-auto" />
          </Button>
        </DropdownMenuTrigger>

        <DropdownMenuContent align="end" className="w-[300px]">
          <DropdownMenuLabel>Switch Project</DropdownMenuLabel>

          {recentProjects.map((project) => (
            <DropdownMenuItem
              key={project.path}
              onClick={() => handleSwitchProject(project)}
              disabled={isLoading}
              className="flex items-center justify-between py-2"
            >
              <div className="flex flex-col">
                <span className="font-medium">{project.name}</span>
                <span className="text-xs text-muted-foreground truncate max-w-[220px]">
                  {project.path}
                </span>
              </div>

              {currentProject?.path === project.path && (
                <CheckCircle2 className="h-4 w-4 text-primary ml-2" />
              )}
            </DropdownMenuItem>
          ))}

          <DropdownMenuSeparator />

          <DropdownMenuItem
            onClick={() => setIsNewProjectDialogOpen(true)}
            disabled={isLoading}
          >
            <FolderPlus className="h-4 w-4 mr-2" />
            <span>Add New Project...</span>
          </DropdownMenuItem>

          <DropdownMenuItem onClick={handleCloseProject} disabled={isLoading}>
            <ArrowLeft className="h-4 w-4 mr-2" />
            <span>Back to Project Selection</span>
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      <NewProjectDialog
        open={isNewProjectDialogOpen}
        onOpenChange={setIsNewProjectDialogOpen}
        onCreateProject={handleCreateProject}
      />
    </>
  );
};

export default ProjectSwitcher;
