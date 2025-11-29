import React from "react";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Check, ChevronsUpDown, Cpu } from "lucide-react";
import { cn } from "@/lib/utils";
import { AIModel } from "@domain/models";

interface ModelPickerProps {
  models: ReadonlyArray<AIModel>;
  selectedModelId: string;
  selectedModelName: string;
  onModelChange: (modelId: string) => void;
  disabled?: boolean;
}

/// ModelPicker provides a searchable dropdown for model selection using shadcn Command
export const ModelPicker: React.FC<ModelPickerProps> = ({
  models,
  selectedModelId,
  selectedModelName,
  onModelChange,
  disabled = false,
}) => {
  const [open, setOpen] = React.useState(false);

  const handleSelect = (modelId: string) => {
    onModelChange(modelId);
    setOpen(false);
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          role="combobox"
          aria-expanded={open}
          disabled={disabled}
          className="w-[250px] justify-between"
        >
          <div className="flex items-center gap-2 truncate">
            <Cpu className="h-4 w-4 shrink-0" />
            <span className="truncate">{selectedModelName}</span>
          </div>
          <ChevronsUpDown className="ml-2 h-4 w-4 shrink-0 opacity-50" />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-[300px] p-0" align="start">
        <Command>
          <CommandInput placeholder="Search models..." />
          <CommandList>
            <CommandEmpty>
              {models.length === 0 ? "Loading models..." : "No models found"}
            </CommandEmpty>
            <CommandGroup>
              {models.map((model) => {
                const displayName = model.label || model.name || model.id;
                const isSelected = model.id === selectedModelId;
                
                return (
                  <CommandItem
                    key={model.id}
                    value={model.id}
                    onSelect={() => handleSelect(model.id)}
                    className="flex flex-col items-start gap-1 py-2"
                  >
                    <div className="flex items-center gap-2 w-full">
                      <Check
                        className={cn(
                          "h-4 w-4 shrink-0",
                          isSelected ? "opacity-100" : "opacity-0"
                        )}
                      />
                      <span className="font-medium truncate flex-1">{displayName}</span>
                    </div>
                    
                    {(model.provider || model.contextWindow) && (
                      <div className="flex items-center gap-2 ml-6 text-xs text-muted-foreground">
                        {model.provider && (
                          <Badge variant="outline" className="text-xs">
                            {model.provider}
                          </Badge>
                        )}
                        {model.contextWindow && (
                          <span>{model.contextWindow.toLocaleString()} tokens</span>
                        )}
                      </div>
                    )}
                  </CommandItem>
                );
              })}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
};
