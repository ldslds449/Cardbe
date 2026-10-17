import { render } from "svelte/server";
import { expect, it } from "vite-plus/test";
import * as m from "$lib/paraglide/messages";
import { setLocale } from "$lib/paraglide/runtime";
import AnnotationNode from "./annotation_node.svelte";
import { build_annotations, type FlowAnnotation } from "./flow";

it.each(["en", "zh-TW"] as const)(
  "renders annotations and note reference states in %s",
  (locale) => {
    setLocale(locale);
    try {
      const text: FlowAnnotation = {
        id: "text-1",
        kind: "text",
        text: "<script>alert(1)</script>\n\u8aaa\u660e \u{1f408}",
        x: 0,
        y: 0,
        width: 280,
      };
      const reference: FlowAnnotation = { ...text, kind: "note", note_id: 1 };
      const notes = [{ id: 1, title: "Original", content: "Body" }];
      const actions = {
        read_only: false,
        locked: false,
        notes_unavailable: false,
        onEdit: () => {},
        onRemove: () => {},
        onScale: () => {},
        editing_id: null,
        onSaveText: async () => true,
        onCancelEdit: () => {},
      };
      const body = (
        annotation: FlowAnnotation,
        entries = notes,
        read_only = false,
        notes_unavailable = false,
        editing = false,
      ) => {
        const [node] = build_annotations(
          { nodes: [], viewport: null, annotations: [annotation] },
          entries,
          {
            ...actions,
            read_only,
            notes_unavailable,
            editing_id: editing ? annotation.id : null,
          },
        );
        return render(AnnotationNode, {
          props: {
            id: node.id,
            data: node.data,
            type: "annotation",
            selected: false,
            dragging: false,
            isConnectable: false,
            positionAbsoluteX: 0,
            positionAbsoluteY: 0,
            zIndex: 0,
            draggable: !read_only,
            deletable: !read_only,
            selectable: true,
          },
        }).body;
      };
      expect(body(text)).not.toContain('data-slot="button"');
      expect(body(text)).not.toContain(m.flow_edit_annotation());
      expect(body(text, notes, false, false, true)).toContain("<textarea");
      expect(body(text, notes, false, false, true)).toContain("outline-dashed");
      expect(body(text)).not.toContain("outline-dashed");
      expect(body(text, notes, false, false, true)).toContain(
        'maxlength="4000"',
      );
      expect(body(text)).not.toContain("<textarea");
      expect(body(text)).not.toContain("bg-accent/70");
      expect(body(text)).not.toContain("shadow-sm");
      expect(body(text)).not.toContain("border-border");
      expect(body(text)).toContain('tabindex="0"');
      expect(body(text)).toContain("&lt;script");
      expect(body(text)).not.toContain("<script>alert");
      expect(body(text, notes, true)).not.toContain(m.flow_edit_annotation());
      expect(body(reference)).toContain(m.flow_note_reference());
      expect(body(reference)).toContain("bg-background");
      expect(body(reference)).not.toContain("bg-accent/70");
      expect(body(reference)).toContain("Original");
      expect(body(reference)).toContain("Body");
      expect(body(reference)).not.toContain('data-slot="button"');
      expect(body(reference, notes, true)).not.toContain(m.flow_open_note());
      expect(body(reference, notes, true)).not.toContain(
        m.flow_remove_annotation(),
      );
      expect(body(reference, [])).toContain(m.error_note_not_found());
      expect(body(reference, [])).not.toContain(m.flow_open_note());
      expect(body(reference, [], false, true)).toContain(
        m.flow_note_unavailable(),
      );
      expect(body(reference, [], false, true)).not.toContain(
        m.error_note_not_found(),
      );
    } finally {
      setLocale("en");
    }
  },
);
