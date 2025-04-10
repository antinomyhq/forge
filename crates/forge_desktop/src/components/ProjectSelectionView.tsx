import React, { useState } from "react";
import { useProjectStore, ProjectInfo } from "@/stores/ProjectStore";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardFooter,
  CardHeader,
  CardTitle,
  CardDescription,
} from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { FolderOpen, FolderPlus, Clock } from "lucide-react";
import { format, parseISO } from "date-fns";
import StatusBar from "./StatusBar";

const ProjectCard: React.FC<{
  project: ProjectInfo;
  onSelect: () => void;
}> = ({ project, onSelect }) => {
  // Format the last accessed time
  const formatDate = (dateString: string) => {
    try {
      return format(parseISO(dateString), "MMM d, yyyy h:mm a");
    } catch (e) {
      return "Unknown date";
    }
  };

  return (
    <Card
      className="cursor-pointer hover:bg-accent/50 transition-colors"
      onClick={onSelect}
    >
      <CardHeader className="pb-2">
        <CardTitle className="text-lg">{project.name}</CardTitle>
        <CardDescription className="truncate">{project.path}</CardDescription>
      </CardHeader>
      <CardFooter className="pt-0 text-xs text-muted-foreground flex justify-start gap-2 items-center">
        <Clock className="h-3 w-3" />
        <span>Last opened: {formatDate(project.lastAccessed)}</span>
      </CardFooter>
    </Card>
  );
};

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

const ProjectSelectionView: React.FC = () => {
  const { recentProjects, selectProject, createNewProject, isLoading, error } =
    useProjectStore();
  const [isNewProjectDialogOpen, setIsNewProjectDialogOpen] = useState(false);

  const handleSelectProject = async (project: ProjectInfo) => {
    if (isLoading) return;
    try {
      await selectProject(project.path);
    } catch (err) {
      console.error("Failed to select project:", err);
    }
  };

  const handleOpenProject = async () => {
    if (isLoading) return;
    try {
      await selectProject();
    } catch (err) {
      console.error("Failed to open project:", err);
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

  return (
    <div className="flex flex-col h-screen w-full bg-background">
      <div className="p-6 flex-1 overflow-auto">
        <header className="text-center mb-8">
          <h1 className="text-3xl font-bold tracking-tight mb-2">
            Forge Desktop
          </h1>
          <p className="text-muted-foreground">Select a project to begin</p>
        </header>

        <main className="flex-1 max-w-3xl mx-auto w-full">
          <div className="flex justify-between items-center mb-4">
            <h2 className="text-xl font-semibold">Recent Projects</h2>
          </div>

          <div className="bg-card rounded-lg p-4 mb-8 shadow-sm">
            {recentProjects.length > 0 ? (
              <ScrollArea className="h-[300px] pr-4">
                <div className="grid gap-4">
                  {recentProjects.map((project) => (
                    <ProjectCard
                      key={project.path}
                      project={project}
                      onSelect={() => handleSelectProject(project)}
                    />
                  ))}
                </div>
              </ScrollArea>
            ) : (
              <div className="text-center py-8 text-muted-foreground">
                <FolderOpen className="h-12 w-12 mx-auto mb-4 opacity-50" />
                <p>No recent projects</p>
              </div>
            )}
          </div>

          <div className="flex flex-col gap-4 mb-8">
            <Button
              size="lg"
              className="w-full gap-2"
              onClick={handleOpenProject}
              disabled={isLoading}
            >
              <FolderOpen className="h-5 w-5" />
              Open Repository
            </Button>

            <Button
              size="lg"
              className="w-full gap-2"
              variant="outline"
              onClick={() => setIsNewProjectDialogOpen(true)}
              disabled={isLoading}
            >
              <FolderPlus className="h-5 w-5" />
              Create New Repository
            </Button>
          </div>

          {error && (
            <Card className="bg-destructive/10 border-destructive/30 mb-8">
              <CardContent className="pt-4">
                <p className="text-destructive">Error: {error}</p>
              </CardContent>
            </Card>
          )}

          <Separator className="my-8" />

          <footer className="text-center text-sm text-muted-foreground">
            <p className="mb-2">Forge Desktop &copy; 2025</p>
          </footer>
        </main>
      </div>

      <StatusBar />

      <NewProjectDialog
        open={isNewProjectDialogOpen}
        onOpenChange={setIsNewProjectDialogOpen}
        onCreateProject={handleCreateProject}
      />
    </div>
  );
};

export default ProjectSelectionView;
