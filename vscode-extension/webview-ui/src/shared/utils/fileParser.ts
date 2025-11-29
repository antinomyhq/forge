/// Parses file references from text in the format @[file/path]
///
/// # Arguments
/// - text: The text to parse
///
/// Returns an array of file paths found in the text
export const parseFileReferences = (text: string): string[] => {
  const regex = /@\[([^\]]+)\]/g;
  const matches: string[] = [];
  let match;

  while ((match = regex.exec(text)) !== null) {
    matches.push(match[1]!);
  }

  return matches;
};
