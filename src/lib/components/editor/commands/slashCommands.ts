import type { Editor } from "@tiptap/core";
import { m } from "$lib/paraglide/messages";

export type SlashCommand = {
  id: string;
  label: string;
  description: string;
  keywords: string[];
  action: (editor: Editor) => void;
};

export function getSlashCommands(): SlashCommand[] {
  return [
    {
      id: "paragraph",
      label: m.editor_paragraph(),
      description: m.editor_slash_paragraph_description(),
      keywords: ["text", "paragraph", "p"],
      action: (editor) => editor.chain().focus().setParagraph().run(),
    },
    {
      id: "heading-1",
      label: m.editor_heading_1(),
      description: m.editor_slash_heading_1_description(),
      keywords: ["heading", "h1", "title"],
      action: (editor) =>
        editor.chain().focus().toggleHeading({ level: 1 }).run(),
    },
    {
      id: "heading-2",
      label: m.editor_heading_2(),
      description: m.editor_slash_heading_2_description(),
      keywords: ["heading", "h2"],
      action: (editor) =>
        editor.chain().focus().toggleHeading({ level: 2 }).run(),
    },
    {
      id: "heading-3",
      label: m.editor_heading_3(),
      description: m.editor_slash_heading_3_description(),
      keywords: ["heading", "h3"],
      action: (editor) =>
        editor.chain().focus().toggleHeading({ level: 3 }).run(),
    },
    {
      id: "bullet-list",
      label: m.editor_bullet_list(),
      description: m.editor_slash_bullet_list_description(),
      keywords: ["list", "bullet", "ul"],
      action: (editor) => editor.chain().focus().toggleBulletList().run(),
    },
    {
      id: "ordered-list",
      label: m.editor_numbered_list(),
      description: m.editor_slash_numbered_list_description(),
      keywords: ["list", "number", "ordered", "ol"],
      action: (editor) => editor.chain().focus().toggleOrderedList().run(),
    },
    {
      id: "task-list",
      label: m.editor_task_list(),
      description: m.editor_slash_task_list_description(),
      keywords: ["list", "task", "todo", "checkbox", "check"],
      action: (editor) => editor.chain().focus().toggleTaskList().run(),
    },
    {
      id: "blockquote",
      label: m.editor_blockquote(),
      description: m.editor_slash_blockquote_description(),
      keywords: ["quote", "blockquote"],
      action: (editor) => editor.chain().focus().toggleBlockquote().run(),
    },
    {
      id: "code-block",
      label: m.editor_code_block(),
      description: m.editor_slash_code_block_description(),
      keywords: ["code", "pre", "programming"],
      action: (editor) => editor.chain().focus().toggleCodeBlock().run(),
    },
    {
      id: "horizontal-rule",
      label: m.editor_horizontal_rule(),
      description: m.editor_slash_horizontal_rule_description(),
      keywords: ["rule", "divider", "separator", "hr"],
      action: (editor) => editor.chain().focus().setHorizontalRule().run(),
    },
  ];
}

export function filterSlashCommands(query: string): SlashCommand[] {
  const slashCommands = getSlashCommands();
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
