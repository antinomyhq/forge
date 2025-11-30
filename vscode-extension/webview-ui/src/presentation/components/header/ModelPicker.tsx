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
import { Check, ChevronsUpDown, Cpu, Wrench, Zap, Brain } from "lucide-react";
import { cn } from "@/lib/utils";
import { AIModel } from "@domain/models";

interface ModelPickerProps {
  models: ReadonlyArray<AIModel>;
  selectedModelId: string;
  onModelChange: (modelId: string) => void;
  disabled?: boolean;
  compact?: boolean; // New prop for icon-only mode
}

/// ModelPicker provides a searchable dropdown for model selection using shadcn Command
export const ModelPicker: React.FC<ModelPickerProps> = ({
  models,
  selectedModelId,
  onModelChange,
  disabled = false,
  compact = false,
}) => {
  const [open, setOpen] = React.useState(false);

  // Derive selected model name from models array to prevent sync issues
  const selectedModel = React.useMemo(
    () => models.find((m) => m.id === selectedModelId),
    [models, selectedModelId]
  );
  const selectedModelName = selectedModel?.name || selectedModel?.id || "Select model...";

  const handleSelect = (modelId: string) => {
    onModelChange(modelId);
    setOpen(false);
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        {compact ? (
          <Button
            variant="ghost"
            role="combobox"
            aria-expanded={open}
            disabled={disabled}
            size="sm"
            className="h-8 px-2 gap-1.5"
          >
            <Cpu className="h-4 w-4" />
            <span className="text-xs">{selectedModelName}</span>
            <ChevronsUpDown className="h-3 w-3 opacity-50" />
          </Button>
        ) : (
          <Button
            variant="outline"
            role="combobox"
            aria-expanded={open}
            disabled={disabled}
            className="w-full xs:max-w-[250px] xs:min-w-[120px] justify-between"
          >
            <div className="flex items-center gap-2 min-w-0 overflow-hidden flex-1">
              <Cpu className="h-4 w-4 shrink-0" />
              <span className="truncate">{selectedModelName}</span>
            </div>
            <ChevronsUpDown className="ml-2 h-4 w-4 shrink-0 opacity-50" />
          </Button>
        )}
      </PopoverTrigger>
      <PopoverContent className="w-[300px] p-0" align="start">
        <Command key={`${selectedModelId}-${open}`}>
          <CommandInput placeholder="Search models..." />
          <CommandList>
            <CommandEmpty>
              {models.length === 0 ? "Loading models..." : "No models found"}
            </CommandEmpty>
            <CommandGroup>
              {models.map((model) => {
                const isSelected = model.id === selectedModelId;
                
                // Create searchable value combining ID, name, and description
                const searchValue = [
                  model.id,
                  model.name,
                  model.description,
                ]
                  .filter(Boolean)
                  .join(' ');
                
                return (
                  <CommandItem
                    key={model.id}
                    value={searchValue}  // Search by combined text
                    keywords={[model.id, model.name].filter(Boolean) as string[]}
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
                      <div className="flex flex-col flex-1 min-w-0">
                        <div className="flex items-baseline gap-2">
                          <span className="font-medium truncate">
                            {model.name || model.id}
                          </span>
                        </div>
                      </div>
                    </div>
                    
                    {/* Model metadata */}
                    <div className="flex items-center gap-2 ml-6 text-xs text-muted-foreground">
                      {model.context_length && (
                        <span>{Number(model.context_length).toLocaleString()} tokens</span>
                      )}
                      {model.tools_supported && (
                        <div className="flex items-center gap-1" title="Supports tool calls">
                          <Wrench className="h-3 w-3" />
                          <span>Tools</span>
                        </div>
                      )}
                      {model.supports_parallel_tool_calls && (
                        <div className="flex items-center gap-1" title="Supports parallel tool calls">
                          <Zap className="h-3 w-3" />
                          <span>Parallel</span>
                        </div>
                      )}
                      {model.supports_reasoning && (
                        <div className="flex items-center gap-1" title="Supports extended thinking">
                          <Brain className="h-3 w-3" />
                          <span>Reasoning</span>
                        </div>
                      )}
                    </div>
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
