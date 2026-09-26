const CARD_REFERENCE_PATTERN = /\[\[([^\]\n|]+)\|(task_\d+)\]\]/g;
const CARD_REFERENCE_HREF_PATTERN = /^#card-(task_\d+)$/;

function escape_markdown_label(label: string): string {
  return label.replace(/([\\\[\]])/g, "\\$1");
}

export function create_card_reference(title: string, task_id: string): string {
  const safe_title =
    title
      .replaceAll("]", " ")
      .replaceAll("|", " ")
      .replace(/[\r\n]/g, " ")
      .trim() || "Untitled card";
  return `[[${safe_title}|${task_id}]]`;
}

export function render_card_references(markdown: string): string {
  return markdown.replace(
    CARD_REFERENCE_PATTERN,
    (_match, title: string, task_id: string) =>
      `[${escape_markdown_label(title.trim())}](#card-${task_id})`,
  );
}

export function task_id_from_card_reference(href: unknown): string | undefined {
  if (typeof href !== "string") return undefined;
  return CARD_REFERENCE_HREF_PATTERN.exec(href)?.[1];
}
