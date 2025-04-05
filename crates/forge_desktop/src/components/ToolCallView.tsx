import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { ChevronDown, ChevronUp, AlertCircle, CheckCircle } from "lucide-react";
import { Separator } from "@/components/ui/separator";
import { Badge } from "@/components/ui/badge";

interface ToolCallProps {
  toolCall: {
    id: string;
    name: string;
    content: string;
    isError: boolean;
    result?: string;
  };
}

const ToolCallView: React.FC<ToolCallProps> = ({ toolCall }) => {
  const [expanded, setExpanded] = React.useState(false);

  const toggleExpanded = () => {
    setExpanded(!expanded);
  };

  return (
    <Card className={`w-full shadow-sm animate-in fade-in duration-200 ${
      toolCall.isError ? 'border-destructive/40' : 'border-primary/20'
    }`}>
      <CardHeader className="py-3 cursor-pointer" onClick={toggleExpanded}>
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <CardTitle className="text-sm font-medium">
              {toolCall.name}
            </CardTitle>
            <Badge variant={toolCall.isError ? "destructive" : "outline"} className="h-5">
              {toolCall.isError ? (
                <>
                  <AlertCircle className="mr-1 h-3 w-3" />
                  Error
                </>
              ) : (
                <>
                  <CheckCircle className="mr-1 h-3 w-3" />
                  Success
                </>
              )}
            </Badge>
          </div>
          
          <Button 
            variant="ghost" 
            size="icon" 
            className="h-7 w-7" 
            onClick={(e) => {
              e.stopPropagation();
              toggleExpanded();
            }}
          >
            {expanded ? <ChevronUp className="h-4 w-4" /> : <ChevronDown className="h-4 w-4" />}
          </Button>
        </div>
      </CardHeader>
      
      {expanded && (
        <CardContent className="pb-3 pt-0">
          <div className="bg-muted rounded-md p-3 font-mono text-xs overflow-x-auto whitespace-pre">
            {toolCall.content}
          </div>
          
          {toolCall.result && (
            <>
              <Separator className="my-2" />
              <div>
                <span className="text-sm font-medium">Result:</span>
                <div className="bg-muted rounded-md p-3 font-mono text-xs mt-1 overflow-x-auto whitespace-pre">
                  {toolCall.result}
                </div>
              </div>
            </>
          )}
        </CardContent>
      )}
    </Card>
  );
};

export default ToolCallView;