import { useFileViewerStore } from "@/stores/FileViewerStore";
import { File as FileIcon } from "lucide-react";

// Define type for file path detection options
interface FilePathDetectionOptions {
  detectRelative?: boolean;
  enableLineNumbers?: boolean;
  baseDir?: string; // Used for converting relative paths to absolute paths
}

// Default options
const defaultOptions: FilePathDetectionOptions = {
  detectRelative: true,
  enableLineNumbers: true,
  baseDir: undefined,
};

/**
 * Extracts line number from a file path if present
 *
 * @param path The path that might contain a line number
 * @returns An object with the cleaned path and optional line number
 */
export interface PathWithLineNumber {
  path: string;
  lineNumber: number | null;
}

export const extractLineNumber = (path: string): PathWithLineNumber => {
  // Check for common patterns like file.js:42 or file.js#L42 or file.js(42)
  const linePatterns = [
    /^(.+):(\d+)$/, // file.js:42
    /^(.+)#L(\d+)$/, // file.js#L42
    /^(.+)\((\d+)\)$/, // file.js(42)
    /^(.+) line (\d+)$/, // file.js line 42
  ];

  for (const pattern of linePatterns) {
    const match = path.match(pattern);
    if (match && match[1] && match[2]) {
      return {
        path: match[1],
        lineNumber: parseInt(match[2], 10),
      };
    }
  }

  return { path, lineNumber: null };
};

/**
 * Checks if a string appears to be a valid file path
 *
 * @param text The text to check
 * @returns boolean indicating if text is likely a file path
 */
export const isFilePath = (text: string): boolean => {
  // First, extract any line numbers
  const { path } = extractLineNumber(text);

  // Patterns to identify likely file paths
  // 1. Absolute paths (Unix-like)
  const unixAbsolutePathPattern = /^\/([^/\0]+(\/)?)+$/;

  // 2. Absolute paths (Windows-like)
  const windowsAbsolutePathPattern = /^[A-Za-z]:((\\|\/)[^\\/:*?"<>|\0]+)+$/;

  // 3. Relative paths with file extension (avoid mismatching URLs)
  const relativeFilePathPattern =
    /^(\.{1,2}\/)?([^\/\0]+\/)*([\w-]+(\.[\w-]+)+)$/;

  // 4. Simple filename with extension (avoid matching single words)
  const fileNameWithExtensionPattern = /^[\w-][\w\s-]*\.\w{1,10}$/;

  // 5. Common project paths
  const projectPathPattern =
    /^(src|lib|app|test|tests|dist|build)\/[\w\/\.-]+$/;

  return (
    unixAbsolutePathPattern.test(path) ||
    windowsAbsolutePathPattern.test(path) ||
    relativeFilePathPattern.test(path) ||
    fileNameWithExtensionPattern.test(path) ||
    projectPathPattern.test(path)
  );
};

/**
 * Finds file paths within a text string
 *
 * @param text The text to search for file paths
 * @returns An array of identified file paths
 */
export const findFilePaths = (text: string): string[] => {
  // First, try to find paths that are explicitly marked with backticks or quotes
  const markedPathsPattern =
    /(`|"|')((?:\/|\.\/|\.\.\/|[A-Za-z]:\\)(?:[^`"'\0])+)(?:`|"|')/g;
  const matches: string[] = [];

  let match: RegExpExecArray | null;
  while ((match = markedPathsPattern.exec(text)) !== null) {
    if (isFilePath(match[2])) {
      matches.push(match[2]);
    }
  }

  // Then look for unmarked paths
  const words = text.split(/\s+/);
  for (const word of words) {
    // Avoid re-checking paths we've already found
    if (!matches.includes(word) && isFilePath(word)) {
      matches.push(word);
    }
  }

  return matches;
};

/**
 * Wraps file paths in text with clickable elements
 *
 * @param text The text containing file paths
 * @param handleClick Function to call when a file path is clicked
 * @returns React elements with clickable file paths
 */
export const makeFilePathsClickable = (
  text: string,
  handleClick: (path: string) => void,
): JSX.Element => {
  if (!text) return <></>;

  const paths = findFilePaths(text);
  if (paths.length === 0) return <p>{text}</p>;

  // Sort by length (longest first) to prevent matching substrings
  paths.sort((a, b) => b.length - a.length);

  // Split the text by file paths and create React elements
  let result: JSX.Element[] = [];
  let currentText = text;

  paths.forEach((path, index) => {
    // Extract any line number before processing
    const { path: cleanPath, lineNumber } = extractLineNumber(path);

    // Split current text by this path
    const parts = currentText.split(path);

    // Add all parts except the last one, with clickable paths in between
    for (let i = 0; i < parts.length - 1; i++) {
      // Add the text before the path
      if (parts[i]) {
        result.push(<span key={`text-${index}-${i}`}>{parts[i]}</span>);
      }

      // Add the clickable path with enhanced styling
      result.push(
        <span
          key={`path-${index}-${i}`}
          className="inline-flex items-center text-blue-600 hover:text-blue-800 cursor-pointer hover:bg-blue-50 px-1 py-0.5 rounded-sm gap-1 group whitespace-nowrap"
          onClick={() => {
            // Pass both path and line number to the handler
            if (lineNumber) {
              handleClick(`${cleanPath}:${lineNumber}`);
            } else {
              handleClick(cleanPath);
            }
          }}
          title={`Open ${path}`}
        >
          <FileIcon className="h-3 w-3 opacity-70 group-hover:opacity-100" />
          <span className="group-hover:underline">{path}</span>
          {lineNumber && (
            <span className="bg-blue-100 text-blue-800 text-xs px-1 rounded-sm">
              {lineNumber}
            </span>
          )}
        </span>,
      );
    }

    // Update currentText to be the last part
    currentText = parts[parts.length - 1];
  });

  // Add any remaining text
  if (currentText) {
    result.push(<span key="remaining-text">{currentText}</span>);
  }

  return <p className="whitespace-pre-wrap">{result}</p>;
};

/**
 * Utility to extract file paths from text.
 * Can detect both absolute and relative paths.
 */
export function detectFilePaths(
  text: string,
  options: FilePathDetectionOptions = {},
): RegExpMatchArray[] {
  const opts = { ...defaultOptions, ...options };

  // Patterns to detect file paths in text
  const patterns = [
    // Absolute paths: /path/to/file.ext or /path/to/file.ext:123 (with line numbers)
    // For macOS and Linux
    /\/((?:[\w.-]+\/)*[\w.-]+\.\w+)(?::(\d+))?/g,

    // Windows absolute paths: C:\path\to\file.ext or C:\path\to\file.ext:123 (with line numbers)
    /([A-Z]:\\(?:[\w.-]+\\)*[\w.-]+\.\w+)(?::(\d+))?/gi,
  ];

  // If detecting relative paths is enabled
  if (opts.detectRelative) {
    // Relative paths like './src/file.js' or '../components/Button.tsx:42'
    patterns.push(/(\.{1,2}\/(?:[\w.-]+\/)*[\w.-]+\.\w+)(?::(\d+))?/g);

    // Simple file references like 'file.js' or 'components/Button.tsx:42'
    patterns.push(
      /\b((?:[\w.-]+\/)*[\w.-]+\.\w{1,10})(?::(\d+))?(?=\s|$|\)|\]|\}|,|\.|\?)/g,
    );
  }

  let matches: RegExpMatchArray[] = [];

  // Execute each pattern to find all matches
  for (const pattern of patterns) {
    let match;
    while ((match = pattern.exec(text)) !== null) {
      matches.push(match);
    }
  }

  return matches;
}

/**
 * React component that renders text with clickable file paths
 */
interface FilePathTextProps {
  text: string;
  options?: FilePathDetectionOptions;
  className?: string;
  // Custom renderer for matched paths (optional)
  renderPath?: (
    path: string,
    line: string | undefined,
    openFile: () => void,
  ) => JSX.Element;
}

export const FilePathText: React.FC<FilePathTextProps> = ({
  text,
  options = {},
  className = "",
  renderPath,
}) => {
  const { openFile } = useFileViewerStore();

  if (!text) return null;

  const paths = detectFilePaths(text, options);

  // If no file paths detected, just return the original text
  if (paths.length === 0) {
    return <span className={className}>{text}</span>;
  }

  // Track positions for slicing the text
  let lastIndex = 0;
  const elements: JSX.Element[] = [];

  // Process each match
  paths.forEach((match, idx) => {
    const [fullMatch, filePath, lineNumber] = match;
    const startIndex = match.index || 0;

    // Add text before the match
    if (startIndex > lastIndex) {
      elements.push(
        <span key={`text-${idx}`}>
          {text.substring(lastIndex, startIndex)}
        </span>,
      );
    }

    // Determine the path to open
    let pathToOpen = filePath;

    // Convert relative path to absolute if baseDir is provided
    if (
      options.baseDir &&
      (filePath.startsWith("./") ||
        filePath.startsWith("../") ||
        !filePath.startsWith("/"))
    ) {
      // Simple path joining for demonstration - might need more robust handling
      pathToOpen = `${options.baseDir}/${filePath}`;
    }

    // Generate full path including line number if available
    const fullPath = lineNumber ? `${pathToOpen}:${lineNumber}` : pathToOpen;

    // Handler for clicking on the path
    const handleOpen = () => {
      openFile(fullPath);
    };

    // Add the clickable path element
    if (renderPath) {
      // Use custom renderer if provided
      elements.push(renderPath(filePath, lineNumber, handleOpen));
    } else {
      // Default rendering
      elements.push(
        <span
          key={`path-${idx}`}
          onClick={handleOpen}
          className="text-blue-600 dark:text-blue-400 hover:underline cursor-pointer font-mono"
          title={`Open ${fullPath}`}
        >
          {fullMatch}
        </span>,
      );
    }

    lastIndex = startIndex + fullMatch.length;
  });

  // Add any remaining text
  if (lastIndex < text.length) {
    elements.push(<span key="text-end">{text.substring(lastIndex)}</span>);
  }

  return <span className={className}>{elements}</span>;
};
