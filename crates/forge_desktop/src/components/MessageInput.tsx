import React, { useState, FormEvent, KeyboardEvent } from 'react';
import { useForge } from '@/contexts/ForgeContext';
import { Card, CardFooter } from "@/components/ui/card";
import { Textarea } from "@/components/ui/textarea";
import { Button } from "@/components/ui/button";
import { Send } from "lucide-react";

const MessageInput: React.FC = () => {
  const [message, setMessage] = useState('');
  const { sendMessage, isLoading } = useForge();
  const textareaRef = React.useRef<HTMLTextAreaElement>(null);

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if (message.trim() && !isLoading) {
      await sendMessage(message);
      setMessage('');
      // Reset textarea height
      if (textareaRef.current) {
        textareaRef.current.style.height = 'auto';
      }
    }
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit(e);
    }
  };

  // Auto-resize textarea as user types
  const handleInputChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const textarea = e.target;
    setMessage(textarea.value);
    
    // Reset height to get the correct scrollHeight
    textarea.style.height = 'auto';
    
    // Set the height based on scrollHeight (with max-height constraint handled by CSS)
    textarea.style.height = `${Math.min(textarea.scrollHeight, 200)}px`;
  };

  return (
    <Card className="rounded-none border-t border-x-0 border-b-0 shadow-md bg-card">
      <CardFooter className="p-4">
        <form onSubmit={handleSubmit} className="flex w-full gap-3 items-end">
          <Textarea
            ref={textareaRef}
            value={message}
            onChange={handleInputChange}
            onKeyDown={handleKeyDown}
            placeholder="Type a message..."
            disabled={isLoading}
            className="min-h-[60px] max-h-[200px] resize-none flex-1 rounded-lg focus-visible:ring-primary"
          />
          <Button 
            type="submit" 
            disabled={!message.trim() || isLoading}
            size="icon"
            className="h-[60px] w-[60px] rounded-full shadow-sm"
          >
            <Send className="h-5 w-5" />
          </Button>
        </form>
      </CardFooter>
    </Card>
  );
};

export default MessageInput;