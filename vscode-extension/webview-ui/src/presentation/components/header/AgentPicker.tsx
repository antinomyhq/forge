import { Check, ChevronsUpDown, Bot } from "lucide-react";
import { useState } from "react";

import { Button } from "@/components/ui/button";
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
import { cn } from "@/lib/utils";
import type { Agent } from "@/domain/models";

interface AgentPickerProps {
  agents: ReadonlyArray<Agent>;
  selectedAgentId: string;
  selectedAgentName: string;
  onAgentChange: (agentId: string) => void;
  disabled?: boolean;
  compact?: boolean;
}

export function AgentPicker({
  agents,
  selectedAgentId,
  selectedAgentName,
  onAgentChange,
  disabled = false,
  compact = false,
}: AgentPickerProps) {
  const [open, setOpen] = useState(false);

  const handleSelect = (agentId: string) => {
    onAgentChange(agentId);
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
            className="h-7 gap-1 px-2 text-xs font-normal hover:bg-transparent"
          >
            <Bot className="h-3.5 w-3.5" />
            <span className="max-w-[120px] truncate">{selectedAgentName}</span>
            <ChevronsUpDown className="ml-1 h-3 w-3 shrink-0 opacity-50" />
          </Button>
        ) : (
          <Button
            variant="outline"
            role="combobox"
            aria-expanded={open}
            disabled={disabled}
            className="justify-between"
          >
            {selectedAgentName || "Select agent..."}
            <ChevronsUpDown className="ml-2 h-4 w-4 shrink-0 opacity-50" />
          </Button>
        )}
      </PopoverTrigger>
      <PopoverContent className="w-[300px] p-0">
        <Command>
          <CommandInput placeholder="Search agents..." />
          <CommandList>
            <CommandEmpty>No agent found.</CommandEmpty>
            <CommandGroup>
              {agents.map((agent) => {
                const displayName = agent.name || agent.id;
                return (
                  <CommandItem
                    key={agent.id}
                    value={displayName}
                    onSelect={() => handleSelect(agent.id)}
                    className="flex items-start gap-2"
                  >
                    <Check
                      className={cn(
                        "mt-0.5 h-4 w-4 shrink-0",
                        selectedAgentId === agent.id
                          ? "opacity-100"
                          : "opacity-0",
                      )}
                    />
                    <div className="flex-1 space-y-0.5">
                      <div className="font-medium">
                        {displayName}
                      </div>
                      {agent.description && (
                        <p className="text-xs text-muted-foreground line-clamp-2">
                          {agent.description}
                        </p>
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
}
