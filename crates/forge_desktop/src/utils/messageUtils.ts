/**
 * Message grouping utility functions
 */

interface Message {
  id: string;
  content: string;
  sender: "user" | "system";
  timestamp: Date;
  isShowUserMessage?: boolean;
}

interface ToolCall {
  id: string;
  name: string;
  content: string;
  isError: boolean;
  result?: string;
}

interface ConversationSection {
  userMessage: Message | null;
  responseMessages: Message[];
  toolCalls: ToolCall[];
}

/**
 * Groups messages and tool calls into conversation sections
 * Each section consists of a user message, the AI responses, and related tool calls
 */
export function groupMessagesIntoSections(
  messages: Message[],
  toolCalls: ToolCall[],
): ConversationSection[] {
  const sections: ConversationSection[] = [];

  // If no messages, return empty array
  if (messages.length === 0) {
    return [];
  }

  let currentSection: ConversationSection = {
    userMessage: null,
    responseMessages: [],
    toolCalls: [],
  };

  // Process messages
  for (let i = 0; i < messages.length; i++) {
    const message = messages[i];

    if (message.sender === "user") {
      // If we have a previous section, push it before starting a new one
      if (
        currentSection.userMessage ||
        currentSection.responseMessages.length > 0
      ) {
        sections.push({ ...currentSection });
      }

      // Start a new section with this user message
      currentSection = {
        userMessage: message,
        responseMessages: [],
        toolCalls: [],
      };
    } else {
      // For system messages, add to current section
      currentSection.responseMessages.push(message);
    }
  }

  // Add the final section if it has content
  if (
    currentSection.userMessage ||
    currentSection.responseMessages.length > 0
  ) {
    sections.push(currentSection);
  }

  // Assign tool calls to relevant sections
  let toolIndex = 0;
  for (let i = 0; i < sections.length; i++) {
    // For each section, assign all tool calls that happened before the next section's user message
    const nextUserTimestamp =
      i < sections.length - 1 && sections[i + 1]?.userMessage
        ? sections[i + 1].userMessage?.timestamp
        : new Date(Date.now() + 1000000); // Far future date if no next message

    // Start from the current tool index
    while (toolIndex < toolCalls.length) {
      // Extract timestamp safely from tool call ID
      const toolIdParts = toolCalls[toolIndex].id.split("-");
      const timestampStr = toolIdParts.length > 1 ? toolIdParts[1] : null;
      const timestamp = timestampStr ? parseInt(timestampStr) : 0;
      const toolDate = timestamp ? new Date(timestamp) : new Date(0);

      const shouldAssignToCurrentSection =
        // We assign each tool call to a section if:
        // 1. We're in the first section and have a user message
        (i === 0 && sections[i]?.userMessage !== null) ||
        // 2. We're in a section with no next section
        i === sections.length - 1 ||
        // 3. The tool's creation time is before the next user message
        toolDate < (nextUserTimestamp || new Date(Date.now() + 1000000));

      if (shouldAssignToCurrentSection) {
        sections[i].toolCalls.push(toolCalls[toolIndex]);
        toolIndex++;
      } else {
        // If this tool should be in the next section, break
        break;
      }
    }
  }

  // Handle any remaining tool calls by adding them to the last section
  if (
    toolCalls.length > 0 &&
    toolIndex < toolCalls.length &&
    sections.length > 0
  ) {
    const lastSection = sections[sections.length - 1];
    for (let i = toolIndex; i < toolCalls.length; i++) {
      lastSection.toolCalls.push(toolCalls[i]);
    }
  }

  return sections;
}
