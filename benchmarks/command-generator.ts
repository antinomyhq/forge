import Handlebars from "handlebars";

/**
 * Loads data from CSV content string (internal helper)
 */
function loadCsvData(csvContent: string): Record<string, string>[] {
  const lines = csvContent.trim().split("\n");
  if (lines.length === 0) {
    return [];
  }

  const firstLine = lines[0];
  if (!firstLine) {
    return [];
  }

  const headers = firstLine.split(",").map((h) => h.trim());
  const data: Record<string, string>[] = [];

  for (let i = 1; i < lines.length; i++) {
    const line = lines[i];
    if (!line || !line.trim()) continue;

    const values = line.split(",").map((v) => v.trim());
    const row: Record<string, string> = {};

    for (let j = 0; j < headers.length; j++) {
      const header = headers[j];
      if (header) {
        row[header] = values[j] || "";
      }
    }

    data.push(row);
  }

  return data;
}

/**
 * Creates a cross product of multiple data arrays (internal helper)
 */
function createCrossProduct(
  sourcesData: Record<string, string>[][]
): Record<string, string>[] {
  if (sourcesData.length === 0) {
    return [];
  }

  return sourcesData.reduce((acc, sourceData) => {
    if (acc.length === 0) {
      return sourceData;
    }

    const result: Record<string, string>[] = [];
    for (const accItem of acc) {
      for (const sourceItem of sourceData) {
        result.push({ ...accItem, ...sourceItem });
      }
    }
    return result;
  }, [] as Record<string, string>[]);
}

/**
 * Generates a command from a template and data context
 */
export function generateCommand(
  commandTemplate: string,
  context: Record<string, string>
): string {
  const template = Handlebars.compile(commandTemplate, {
    strict: true,
  });
  return template(context);
}

/**
 * Generates all commands from a template and cross product data (internal helper)
 */
function generateCommands(
  commandTemplate: string,
  sourcesData: Record<string, string>[][]
): string[] {
  const crossProduct = createCrossProduct(sourcesData);
  return crossProduct.map((context) =>
    generateCommand(commandTemplate, context)
  );
}

/**
 * Pure function to get contexts from sources data
 */
export function getContextsFromSources(
  sourcesData: Record<string, string>[][]
): Record<string, string>[] {
  return createCrossProduct(sourcesData);
}
