import React from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { SideBySideDiff } from "@/components/document/SideBySideDiff";
import { DiffJsonData } from "@/utils/diffUtils";
import { X } from "lucide-react";
import { Button } from "@/components/ui/button";

interface DiffModalViewProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  diffData: DiffJsonData | null;
}

export const DiffModalView: React.FC<DiffModalViewProps> = ({
  open,
  onOpenChange,
  diffData,
}) => {
  if (!diffData || !diffData.has_changes) {
    return null;
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-[90vw] w-[90vw] max-h-[90vh] h-[90vh] flex flex-col">
        <DialogHeader className="flex w-full items-center justify-between">
          <DialogTitle className="text-lg font-medium">
            {diffData.path}
          </DialogTitle>
          <Button
            variant="ghost"
            size="icon"
            onClick={() => onOpenChange(false)}
            className="h-8 w-8"
          >
            <X className="h-4 w-4" />
          </Button>
        </DialogHeader>

        <div className="flex-grow overflow-hidden">
          <SideBySideDiff diffData={diffData} showFilePath={false} />
        </div>
      </DialogContent>
    </Dialog>
  );
};
