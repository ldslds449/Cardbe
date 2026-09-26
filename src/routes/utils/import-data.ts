export interface ImportSummary {
  columns: number;
  tasks: number;
  archives: number;
  templates: number;
}

export function parse_import_summary(json_data: string): ImportSummary {
  const parsed: unknown = JSON.parse(json_data);
  if (typeof parsed !== "object" || parsed === null) {
    throw new Error("The import file must contain a JSON object");
  }

  const candidate = parsed as {
    columns?: unknown;
    archives?: unknown;
    templates?: unknown;
  };
  if (!Array.isArray(candidate.columns) || !Array.isArray(candidate.archives)) {
    throw new Error("The import file must contain columns and archives arrays");
  }

  const tasks = candidate.columns.reduce<number>((count, column) => {
    if (typeof column !== "object" || column === null) return count;
    const column_tasks = (column as { tasks?: unknown }).tasks;
    return count + (Array.isArray(column_tasks) ? column_tasks.length : 0);
  }, 0);

  return {
    columns: candidate.columns.length,
    tasks,
    archives: candidate.archives.length,
    templates: Array.isArray(candidate.templates)
      ? candidate.templates.length
      : 0,
  };
}
