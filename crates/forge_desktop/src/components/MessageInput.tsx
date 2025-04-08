import React, { useState, FormEvent, useRef, DragEvent } from 'react';
import { useForgeStore } from '@/stores/ForgeStore';
import { Card, CardFooter } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Send, X } from "lucide-react";
import { cn } from '@/lib/utils';
import TipTapEditor from './TipTapEditor';

const MessageInput: React.FC = () => {
  const [message, setMessage] = useState('');
  const [isDragging, setIsDragging] = useState(false);
  const { sendMessage, cancelStream, isLoading, taggedFiles, addTaggedFile, removeTaggedFile } = useForgeStore();
  
  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if ((message.trim() || taggedFiles.length > 0) && !isLoading) {
      await sendMessage(message);
      setMessage('');
    }
  };

  const handleDragOver = (e: DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    if (!isDragging) {
      setIsDragging(true);
    }
  };

  const handleDragLeave = (e: DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    setIsDragging(false);
  };

  // Use this function for files dropped directly on our components
  const handleDrop = (e: DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    setIsDragging(false);
    
    const filePath = e.dataTransfer.getData('text/plain');
    if (filePath) {
      // Add the file to the tagged files in the store
      addTaggedFile(filePath);
    }
  };

  // This function is called by TipTap when a file is dropped in the editor
  const handleFileDrop = (filePath: string) => {
    setIsDragging(false);
    if (filePath) {
      addTaggedFile(filePath);
    }
  };

  return (
    <Card className="rounded-none border-t border-x-0 border-b-0 shadow-md bg-card">
      <CardFooter className="p-4">
        <form onSubmit={handleSubmit} className="flex w-full gap-2">
          <div 
            className="flex w-full gap-3 items-end"
            onDragOver={handleDragOver}
            onDragLeave={handleDragLeave}
            onDrop={handleDrop}
          >
            <TipTapEditor
              content={message}
              onChange={setMessage}
              onSubmit={handleSubmit}
              taggedFiles={taggedFiles}
              onRemoveFile={removeTaggedFile}
              onFileDrop={handleFileDrop}
              disabled={isLoading}
              isDragging={isDragging}
              placeholder="Type a message or drop files here..."
              className="flex-1 min-h-[60px]"
            />
            
            {isLoading ? (
              <Button 
                type="button" 
                onClick={() => cancelStream()}
                size="icon"
                variant="destructive"
                className="h-[60px] w-[60px] rounded-full shadow-sm animate-pulse"
              >
                <X className="h-5 w-5" />
              </Button>
            ) : (
              <Button 
                type="submit" 
                disabled={!message.trim() && taggedFiles.length === 0}
                size="icon"
                className="h-[60px] w-[60px] rounded-full shadow-sm"
              >
                <Send className="h-5 w-5" />
              </Button>
            )}
          </div>
        </form>
      </CardFooter>
    </Card>
  );
};

export default MessageInput;