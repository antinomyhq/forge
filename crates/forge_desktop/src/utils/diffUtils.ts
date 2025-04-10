/**
 * Utilities for handling diff content from tool results
 */

// Types that mirror the Rust DiffJson structure
export interface DiffChange {
  tag: "delete" | "insert" | "equal";
  old_index: number | null;
  new_index: number | null;
  content: string;
}

export interface DiffHunk {
  old_start: number;
  old_lines: number;
  new_start: number;
  new_lines: number;
  changes: DiffChange[];
}

export interface DiffJsonData {
  op_name: string;
  path: string;
  unified_diff: string;
  hunks: DiffHunk[];
  has_changes: boolean;
  file_type: string;
}

// File change states
export type FileChangeType = "modified" | "added" | "deleted" | "renamed";

export interface FileChange {
  path: string;
  type: FileChangeType;
  oldPath?: string; // Used for renamed files
  additions: number;
  deletions: number;
  lastModified: Date;
  diffData?: DiffJsonData;
}

/**
 * Try to parse a string as JSON diff data
 */
export function tryParseDiffJson(content: string): DiffJsonData | null {
  try {
    const parsed = JSON.parse(content);
    // Check if the parsed object has the expected properties
    if (
      parsed &&
      typeof parsed === "object" &&
      "op_name" in parsed &&
      "path" in parsed &&
      "unified_diff" in parsed &&
      "hunks" in parsed &&
      "has_changes" in parsed &&
      "file_type" in parsed
    ) {
      return parsed as DiffJsonData;
    }
  } catch (e) {
    // Not valid JSON, that's okay
  }
  return null;
}

/**
 * Check if a string appears to be a diff (even non-JSON)
 */
export function isDiffContent(content: string): boolean {
  // Check if it's a JSON diff
  if (tryParseDiffJson(content)) {
    return true;
  }

  // Simple heuristics for unified diff format
  const lines = content.split("\n");
  const hasAdditions = lines.some(
    (line) => line.startsWith("+") && !line.startsWith("+++"),
  );
  const hasDeletions = lines.some(
    (line) => line.startsWith("-") && !line.startsWith("---"),
  );
  const hasHunks = lines.some((line) =>
    line.match(/^@@\s[-+]?\d+,\d+\s[-+]?\d+,\d+\s@@/),
  );

  return (hasAdditions || hasDeletions) && hasHunks;
}

/**
 * Calculate stats for a diff
 */
export function calculateDiffStats(diff: DiffJsonData): {
  additions: number;
  deletions: number;
} {
  let additions = 0;
  let deletions = 0;

  for (const hunk of diff.hunks) {
    for (const change of hunk.changes) {
      if (change.tag === "insert") {
        additions++;
      } else if (change.tag === "delete") {
        deletions++;
      }
    }
  }

  return { additions, deletions };
}

/**
 * Group a list of file changes by directory
 */
export function groupFileChangesByDirectory(
  changes: FileChange[],
): Record<string, FileChange[]> {
  const grouped: Record<string, FileChange[]> = {};

  for (const change of changes) {
    const path = change.path;
    const lastSlashIndex = path.lastIndexOf("/");

    let directory = "/";
    if (lastSlashIndex > 0) {
      directory = path.substring(0, lastSlashIndex);
    }

    if (!grouped[directory]) {
      grouped[directory] = [];
    }

    grouped[directory].push(change);
  }

  return grouped;
}
