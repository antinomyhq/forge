import React, { useState } from 'react';
import { X, Check } from 'lucide-react';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';

interface FileTagProps {
  filePath: string;
  onRemove: () => void;
  inline?: boolean;
  readOnly?: boolean;
  copyFormat?: 'tag' | 'path'; // New prop to control copy format
}

// Helper function to get basename from a path (replaces path.basename functionality)
const getBasename = (filepath: string | null | undefined): string => {
  if (!filepath) {
    return 'Unknown';
  }
  // Handle both Windows and Unix-style paths
  const normalizedPath = filepath.replace(/\\/g, '/');
  const parts = normalizedPath.split('/');
  return parts[parts.length - 1] || filepath;
};

const FileTag: React.FC<FileTagProps> = ({ 
  filePath, 
  onRemove, 
  inline = false, 
  readOnly = false,
  copyFormat = 'tag'
}) => {
  const [copied, setCopied] = useState(false);
  
  // Get the value to use when copying the tag
  const getCopyValue = (): string => {
    return copyFormat === 'tag' ? `@[${filePath}]` : filePath;
  };

  const handleCopy = (e: React.MouseEvent<HTMLDivElement>) => {
    e.stopPropagation();
    // Copy the full tag format to clipboard
    navigator.clipboard.writeText(getCopyValue());
    
    // Show copied indicator
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };
  // Extract just the filename from the path using our custom function
  const fileName = getBasename(filePath);

  return (
    <TooltipProvider>
      <Tooltip delayDuration={300}>
        <TooltipTrigger asChild>
          <div 
            className={`inline-flex items-center bg-blue-50 border border-blue-100 rounded-md py-0.5 px-1.5 mr-1 ${inline ? 'my-0 mx-0.5' : 'mb-1'} text-xs group whitespace-nowrap hover:bg-blue-100 transition-colors cursor-pointer`}
            onClick={handleCopy} // Add click handler to copy the file tag
            title="Click to copy"
          >
            <span className="truncate max-w-[120px] text-blue-700 font-medium">{fileName}</span>
            {copied && (
              <span className="ml-1 text-green-600" title="Copied!">
                <Check className="h-3 w-3" />
              </span>
            )}
            {!readOnly && (
              <button 
                onClick={(e: React.MouseEvent<HTMLButtonElement>) => {
                  e.preventDefault();
                  onRemove();
                }}
                className="ml-1 text-muted-foreground hover:text-destructive transition-colors"
                aria-label="Remove file tag"
              >
                <X className="h-3 w-3" />
              </button>
            )}
          </div>
        </TooltipTrigger>
        <TooltipContent side="top">
          <p className="max-w-[300px] break-all text-xs">{filePath || 'Unknown path'}</p>
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
};

export default FileTag;