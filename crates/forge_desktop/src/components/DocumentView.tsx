import React, { useRef, useEffect, useState } from "react";
import { useForgeStore } from "@/stores/ForgeStore";
import { ScrollArea } from "@/components/ui/scroll-area";
import ConversationSection from "./document/ConversationSection";
import { groupMessagesIntoSections } from "@/utils/messageUtils";

const DocumentView: React.FC = () => {
  const { messages, toolCalls, isLoading } = useForgeStore();
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const scrollAreaRef = useRef<HTMLDivElement>(null);
  const [autoScroll, setAutoScroll] = useState(true);

  // Group messages into conversation sections with tool calls
  const conversationSections = groupMessagesIntoSections(messages, toolCalls);

  // Handle scrolling behavior
  useEffect(() => {
    if (autoScroll && messagesEndRef.current) {
      messagesEndRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [messages, isLoading, autoScroll]);

  // Detect when user manually scrolls up to disable auto-scrolling
  useEffect(() => {
    const container = scrollAreaRef.current;
    if (!container) return;

    const handleScroll = () => {
      const scrollElement = container.querySelector(
        "[data-radix-scroll-area-viewport]",
      );
      if (!scrollElement) return;

      const { scrollTop, scrollHeight, clientHeight } = scrollElement;
      // If user scrolls up more than 100px from bottom, disable auto-scroll
      const isNearBottom = scrollHeight - scrollTop - clientHeight < 100;
      setAutoScroll(isNearBottom);
    };

    container.addEventListener("scroll", handleScroll, true);
    return () => container.removeEventListener("scroll", handleScroll, true);
  }, []);

  return (
    <div className="h-full w-full">
      <ScrollArea
        className="document-view h-full w-full px-6 py-4"
        ref={scrollAreaRef}
      >
        <div className="document-content max-w-4xl mx-auto">
          {conversationSections.map((section, index) => (
            <ConversationSection
              key={`section-${index}`}
              userMessage={section.userMessage}
              responseMessages={section.responseMessages}
              toolCalls={section.toolCalls}
              isLatest={index === conversationSections.length - 1}
            />
          ))}

          {/* Loading indicator */}
          {isLoading && (
            <div className="flex py-2 my-2">
              <div className="flex gap-1 px-3 py-2">
                <div
                  className="w-2 h-2 rounded-full bg-muted-foreground animate-bounce"
                  style={{ animationDelay: "-0.32s" }}
                ></div>
                <div
                  className="w-2 h-2 rounded-full bg-muted-foreground animate-bounce"
                  style={{ animationDelay: "-0.16s" }}
                ></div>
                <div className="w-2 h-2 rounded-full bg-muted-foreground animate-bounce"></div>
              </div>
            </div>
          )}

          <div ref={messagesEndRef} />
        </div>
      </ScrollArea>
    </div>
  );
};

export default DocumentView;
