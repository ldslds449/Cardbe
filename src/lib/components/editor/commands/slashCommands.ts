import type { Editor } from "@tiptap/core";

export type SlashCommand = {
  id: string;
  label: string;
  description: string;
  keywords: string[];
  action: (editor: Editor) => void;
};

export const slashCommands: SlashCommand[] = [
  {
    id: "paragraph",
    label: "Text",
    description: "Start with a plain paragraph",
    keywords: ["text", "paragraph", "p"],
    action: (editor) => editor.chain().focus().setParagraph().run(),
  },
  {
    id: "heading-1",
    label: "Heading 1",
    description: "Large section heading",
    keywords: ["heading", "h1", "title"],
    action: (editor) =>
      editor.chain().focus().toggleHeading({ level: 1 }).run(),
  },
  {
    id: "heading-2",
    label: "Heading 2",
    description: "Medium section heading",
    keywords: ["heading", "h2"],
    action: (editor) =>
      editor.chain().focus().toggleHeading({ level: 2 }).run(),
  },
  {
    id: "heading-3",
    label: "Heading 3",
    description: "Small section heading",
    keywords: ["heading", "h3"],
    action: (editor) =>
      editor.chain().focus().toggleHeading({ level: 3 }).run(),
  },
  {
    id: "bullet-list",
    label: "Bullet List",
    description: "Create a bulleted list",
    keywords: ["list", "bullet", "ul"],
    action: (editor) => editor.chain().focus().toggleBulletList().run(),
  },
  {
    id: "ordered-list",
    label: "Numbered List",
    description: "Create a numbered list",
    keywords: ["list", "number", "ordered", "ol"],
    action: (editor) => editor.chain().focus().toggleOrderedList().run(),
  },
  {
    id: "task-list",
    label: "Task List",
    description: "Create a checklist",
    keywords: ["list", "task", "todo", "checkbox", "check"],
    action: (editor) => editor.chain().focus().toggleTaskList().run(),
  },
  {
    id: "blockquote",
    label: "Quote",
    description: "Quote another person",
    keywords: ["quote", "blockquote"],
    action: (editor) => editor.chain().focus().toggleBlockquote().run(),
  },
  {
    id: "code-block",
    label: "Code Block",
    description: "Write a fenced code block",
    keywords: ["code", "pre", "programming"],
    action: (editor) => editor.chain().focus().toggleCodeBlock().run(),
  },
  {
    id: "horizontal-rule",
    label: "Horizontal Rule",
    description: "Separate sections with a rule",
    keywords: ["rule", "divider", "separator", "hr"],
    action: (editor) => editor.chain().focus().setHorizontalRule().run(),
  },
];

export function filterSlashCommands(query: string): SlashCommand[] {
  const normalized = query.trim().toLowerCase();
  if (!normalized) {
    return slashCommands;
  }

  return slashCommands.filter((command) =>
    [command.label, command.description, ...command.keywords].some((value) =>
      value.toLowerCase().includes(normalized),
    ),
  );
}
