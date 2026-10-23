import { render } from "svelte/server";
import { expect, it } from "vite-plus/test";
import * as m from "$lib/paraglide/messages";
import { setLocale } from "$lib/paraglide/runtime";
import ChecklistDisplay from "./checklist_display.svelte";
import ChecklistSummary from "./checklist_summary.svelte";
import TaskActionMenu from "./test-fixtures/task_action_menu.svelte";

it.each(["en", "zh-TW"] as const)(
  "renders shared checklist counts in %s",
  (locale) => {
    setLocale(locale);
    try {
      expect(
        render(ChecklistSummary, { props: { items: [] } }).body.replace(
          /<!--.*?-->/g,
          "",
        ),
      ).toBe("");
      const items = [
        { id: "1", text: "Done", completed: true },
        { id: "2", text: "Pending", completed: false },
      ];
      expect(render(ChecklistSummary, { props: { items } }).body).toContain(
        m.task_checklist_progress({ completed: 1, total: 2 }),
      );
    } finally {
      setLocale("en");
    }
  },
);

it.each(["en", "zh-TW"] as const)(
  "hides completed checklist items and previews cards in %s",
  (locale) => {
    setLocale(locale);
    try {
      const items = [
        { id: "1", text: "Done item", completed: true },
        ...[2, 3, 4, 5].map((n) => ({
          id: String(n),
          text: `Pending ${n}`,
          completed: false,
        })),
      ];
      const full = render(ChecklistDisplay, { props: { items } }).body;
      expect(full).not.toContain("Done item");
      expect(full).toContain("Pending 5");
      expect(full).toContain(m.task_checklist_show_completed({ count: 1 }));
      const preview = render(ChecklistDisplay, {
        props: { items, preview_limit: 3 },
      }).body;
      expect(preview).toContain("Pending 4");
      expect(preview).not.toContain("Pending 5");
      expect(preview).toContain("line-clamp-1");
      expect(preview).toContain(m.task_checklist_more_items({ count: 1 }));
      expect(preview).not.toContain(
        m.task_checklist_show_completed({ count: 1 }),
      );
      expect(full).not.toContain("line-clamp-1");

      const collapsed = render(ChecklistDisplay, {
        props: { items, show_items: false },
      }).body;
      expect(collapsed).not.toContain("Pending 2");
      expect(collapsed).not.toContain(
        m.task_checklist_show_completed({ count: 1 }),
      );
    } finally {
      setLocale("en");
    }
  },
);

it.each([false, true])(
  "uses the correct menu primitives (context: %s) and hides unavailable actions",
  (context) => {
    const full = render(TaskActionMenu, { props: { context } }).body;
    expect(full).toContain(m.task_view_details());
    expect(full).toContain(m.task_edit_action());
    expect(full).toContain(m.task_archive_action());
    expect(full).toContain('data-variant="destructive"');
    expect(full).toContain(
      context
        ? 'data-slot="context-menu-item"'
        : 'data-slot="dropdown-menu-item"',
    );
    const readonly = render(TaskActionMenu, {
      props: { context, editable: false },
    }).body;
    expect(readonly).toContain(m.task_view_details());
    expect(readonly).not.toContain(m.task_edit_action());
    expect(readonly).not.toContain(m.task_archive_action());
    expect(readonly).not.toContain('data-variant="destructive"');
  },
);
