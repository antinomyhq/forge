import React, { useRef, useEffect, useState } from 'react';
import { useForge } from '@/contexts/ForgeContext';
import ToolCallView from './ToolCallView';
import ReactMarkdown from 'react-markdown';
import { ScrollArea } from "@/components/ui/scroll-area";
import { Avatar, AvatarFallback } from "@/components/ui/avatar";
import { Card, CardContent } from "@/components/ui/card";

// Helper function to determine if a message appears to be a thinking message
const isThinkingMessage = (content: string): boolean => {
  // Check if the content has any thinking-related tags
  return content.includes('<thinking>') || 
         content.includes('<analysis>') || 
         content.includes('<execution>') || 
         content.includes('<verification>') || 
         content.includes('<action_plan>');
};

type MessageType = 'user' | 'system' | 'thinking';

// Helper function to determine if a message should be dimmed
const shouldDimMessage = (message: { sender: string; isShowUserMessage?: boolean }): boolean => {
  return message.sender === 'system' && !message.isShowUserMessage;
};

const ChatView: React.FC = () => {
  const { messages, toolCalls, isLoading } = useForge();
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const scrollAreaRef = useRef<HTMLDivElement>(null);
  const [autoScroll, setAutoScroll] = useState(true);

  // Handle scrolling behavior
  useEffect(() => {
    if (autoScroll && messagesEndRef.current) {
      messagesEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [messages, toolCalls, isLoading, autoScroll]);

  // Detect when user manually scrolls up to disable auto-scrolling
  useEffect(() => {
    const container = scrollAreaRef.current;
    if (!container) return;

    const handleScroll = () => {
      const scrollElement = container.querySelector('[data-radix-scroll-area-viewport]');
      if (!scrollElement) return;
      
      const { scrollTop, scrollHeight, clientHeight } = scrollElement;
      // If user scrolls up more than 100px from bottom, disable auto-scroll
      const isNearBottom = scrollHeight - scrollTop - clientHeight < 100;
      setAutoScroll(isNearBottom);
    };

    container.addEventListener('scroll', handleScroll, true);
    return () => container.removeEventListener('scroll', handleScroll, true);
  }, []);

  const formatTimestamp = (date: Date) => {
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  };

  // Process messages to properly categorize thinking vs regular messages
  const processedMessages = messages.map(message => {
    if (message.sender === 'system' && isThinkingMessage(message.content)) {
      return { ...message, type: 'thinking' as MessageType };
    }
    return { ...message, type: message.sender as MessageType };
  });

  return (
    <ScrollArea className="h-full w-full px-4 pt-4 pb-2" ref={scrollAreaRef}>
      <div className="flex flex-col gap-6 max-w-4xl mx-auto">
        {processedMessages.map((message) => {
          const isThinking = message.type === 'thinking';
          const isUser = message.sender === 'user';
          
          if (isThinking) return null; // Hide thinking messages
          
          return (
            <div 
              key={message.id} 
              className={`flex gap-3 ${isUser ? 'flex-row-reverse' : 'flex-row'} items-start animate-in fade-in duration-200`}
            >
              <Avatar className={isUser ? 'bg-primary/10' : 'bg-secondary/80'}>
                <AvatarFallback>{isUser ? 'U' : 'AI'}</AvatarFallback>
              </Avatar>
              
              <div className="flex flex-col gap-1.5 max-w-[85%]">
                <Card className={`shadow-sm border-0 ${isUser ? 'bg-primary text-primary-foreground' : 'bg-muted/50'}`}>
                  <CardContent className={`p-3 ${!isUser && 'prose prose-sm dark:prose-invert max-w-none'}`}>
                    {message.sender === 'system' ? (
                      <div className={shouldDimMessage(message) ? 'text-muted-foreground' : ''}>
                        <ReactMarkdown>
                          {message.content}
                        </ReactMarkdown>
                      </div>
                    ) : (
                      <div className="whitespace-pre-wrap">{message.content}</div>
                    )}
                  </CardContent>
                </Card>
                
                <span className="text-xs text-muted-foreground px-1">
                  {formatTimestamp(message.timestamp)}
                </span>
              </div>
            </div>
          );
        })}
        
        {toolCalls.map(toolCall => (
          <ToolCallView key={toolCall.id} toolCall={toolCall} />
        ))}
        
        {isLoading && (
          <div className="flex items-center gap-3 self-start">
            <Avatar className="bg-secondary/80">
              <AvatarFallback>AI</AvatarFallback>
            </Avatar>
            
            <Card className="flex items-center px-4 py-3 shadow-sm border-0 bg-muted/50">
              <div className="flex gap-1">
                <div className="w-2 h-2 rounded-full bg-muted-foreground animate-bounce" style={{ animationDelay: '-0.32s' }}></div>
                <div className="w-2 h-2 rounded-full bg-muted-foreground animate-bounce" style={{ animationDelay: '-0.16s' }}></div>
                <div className="w-2 h-2 rounded-full bg-muted-foreground animate-bounce"></div>
              </div>
            </Card>
          </div>
        )}
        
        <div ref={messagesEndRef} />
      </div>
    </ScrollArea>
  );
};

export default ChatView;