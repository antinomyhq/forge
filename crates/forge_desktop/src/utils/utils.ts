import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/**
 * Formats a timestamp into a readable time string
 */
export function formatTime(date: Date): string {
  return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

/**
 * Detects code blocks in a markdown string and returns an array of
 * { language, code } objects
 */
export function extractCodeBlocks(
  markdown: string,
): { language: string; code: string }[] {
  const codeBlockRegex = /```(\w*)\n([\s\S]*?)```/g;
  const codeBlocks: { language: string; code: string }[] = [];

  let match;
  while ((match = codeBlockRegex.exec(markdown)) !== null) {
    codeBlocks.push({
      language: match[1] || "text",
      code: match[2].trim(),
    });
  }

  return codeBlocks;
}

/**
 * Extracts the path from a file path displayed in a code block header
 * Example: "src/components/UserList.tsx" from "┌─ src/components/UserList.tsx ─┐"
 */
export function extractFilePath(header: string): string | null {
  // Match common file header patterns like "filename.ext" or "path/to/file.ext"
  const match = header.match(/[a-zA-Z0-9_\-./]+\.[a-zA-Z0-9]+/);
  return match ? match[0] : null;
}

/**
 * Truncates text to a specified length with ellipsis
 */
export function truncateText(text: string, maxLength: number): string {
  if (text.length <= maxLength) return text;
  return text.slice(0, maxLength) + "...";
}
