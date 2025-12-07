import tmp from "tmp";
import { parse as parseCsv } from "csv-parse";

/**
 * Formats a date with local timezone information
 */
export function formatTimestamp(date: Date): string {
  const offset = -date.getTimezoneOffset();
  const sign = offset >= 0 ? "+" : "-";
  const hours = Math.floor(Math.abs(offset) / 60)
    .toString()
    .padStart(2, "0");
  const minutes = (Math.abs(offset) % 60).toString().padStart(2, "0");
  const timezone = `${sign}${hours}:${minutes}`;

  return `${date.toISOString().replace("Z", "")}${timezone}`;
}

/**
 * Escapes special regex characters in a string
 */
export function escapeRegex(str: string): string {
  return str.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * Creates a temporary directory asynchronously
 */
export async function createTempDir(
  prefix: string,
): Promise<{ name: string; removeCallback: () => void }> {
  return new Promise((resolve, reject) => {
    tmp.dir({ prefix }, (err, path, cleanupCallback) => {
      if (err) reject(err);
      else resolve({ name: path, removeCallback: cleanupCallback });
    });
  });
}

/**
 * Parses CSV content asynchronously
 */
export async function parseCsvAsync(
  content: string,
  options: {
    columns: boolean;
    skip_empty_lines: boolean;
  },
): Promise<Record<string, string>[]> {
  return new Promise((resolve, reject) => {
    parseCsv(content, options, (err, records: unknown) => {
      if (err) reject(err);
      else resolve(records as Record<string, string>[]);
    });
  });
}
