import { describe, expect, it } from "vite-plus/test";
import { readFileSync } from "node:fs";
import { Position } from "@xyflow/svelte";
import {
  build_nodes,
  build_annotations,
  build_edges,
  moved_layout,
  added_layout,
  connected_layout,
  available_task_columns,
  missing_archive_task_ids,
  card_edge_point,
  scaled_layout,
  clamp_scale,
  type FlowLayout,
} from "./flow";
import { create_task } from "../../type/task.svelte";
import { create_column } from "../../type/column.svelte";

const layout: FlowLayout = {
  nodes: [
    { task_id: 1, x: 10, y: 20 },
    { task_id: 2, x: 240, y: 20 },
    { task_id: 99, x: -40, y: 100 },
  ],
  viewport: { x: 0, y: 0, zoom: 1 },
};

it("projects live note references, preserves missing references, and saves annotation movement", () => {
  const annotated: FlowLayout = {
    ...layout,
    annotations: [
      {
        id: "text-1",
        kind: "text",
        text: "說明 🐈\n第二行",
        x: 10,
        y: 20,
        width: 280,
      },
      { id: "note-1", kind: "note", note_id: 1, x: 300, y: 20, width: 280 },
      { id: "missing-1", kind: "note", note_id: 99, x: 600, y: 20, width: 280 },
    ],
  };
  const actions = {
    notes_unavailable: false,
    read_only: false,
    locked: false,
    onEdit: () => {},
    onRemove: () => {},
    onScale: () => {},
    editing_id: null,
    onSaveText: async () => true,
    onCancelEdit: () => {},
  };
  const notes = [{ id: 1, title: "Original", content: "Body" }];
  const nodes = build_annotations(annotated, notes, actions);
  expect(nodes.map((node) => node.id)).toEqual([
    "annotation_text-1",
    "annotation_note-1",
    "annotation_missing-1",
  ]);
  expect(nodes[1].data.note).toEqual(notes[0]);
  expect(
    build_annotations(
      {
        ...annotated,
        annotations: [{ ...annotated.annotations![0], scale: 0.5 }],
      },
      notes,
      actions,
    )[0].width,
  ).toBe(140);
  expect(nodes[2].data.note).toBeUndefined();
  expect(nodes[1].data.annotation).not.toHaveProperty("content");
  expect(
    build_annotations(
      annotated,
      [{ ...notes[0], content: "Edited" }],
      actions,
    )[1].data.note?.content,
  ).toBe("Edited");
  expect(
    build_annotations(annotated, [], actions)[1].data.note,
  ).toBeUndefined();
  nodes[0].position = { x: -50, y: 80 };
  // Rendered widths include the card scale and are never stored.
  nodes[0].width = 420;
  const moved = moved_layout(annotated, nodes.slice(0, 2), {
    x: 0,
    y: 0,
    zoom: 1,
  });
  expect(moved.annotations?.[0]).toMatchObject({
    x: -50,
    y: 80,
    width: 280,
    text: "說明 🐈\n第二行",
  });
  expect(moved.annotations?.[2]).toEqual(annotated.annotations?.[2]);
  expect(moved.nodes).toEqual(layout.nodes);
  expect(annotated.annotations?.[0].width).toBe(280);
});

it("adds multiple tasks inside a panned viewport without duplicates or overlapping positions", () => {
  const viewport = { x: -900, y: 200, zoom: 0.5 };
  const result = added_layout(
    layout,
    ["task_1", "task_3", "task_3", "task_4"],
    viewport,
  );
  expect(result.nodes).toHaveLength(5);
  expect(result.nodes.slice(0, 3)).toEqual(layout.nodes);
  const first = result.nodes[3];
  expect(first.x * viewport.zoom + viewport.x).toBe(40);
  expect(first.y * viewport.zoom + viewport.y).toBe(40);
  expect(result.nodes[4].x - first.x).toBe(300);
  expect(layout.nodes).toHaveLength(3);
});
it("adds existing tasks at the canvas context position, retaining spacing and deduplication", () => {
  const result = added_layout(
    layout,
    ["task_1", "task_3", "task_3", "task_4"],
    { x: -900, y: 200, zoom: 0.5 },
    { x: 760, y: -120 },
  );
  expect(result.nodes.slice(3)).toEqual([
    { task_id: 3, x: 760, y: -120 },
    { task_id: 4, x: 1060, y: -120 },
  ]);
  expect(result.nodes.slice(0, 3)).toEqual(layout.nodes);
});
const actions = {
  read_only: false,
  locked: false,
  onOpen: () => {},
  onEdit: () => {},
  onRemove: () => {},
  onArchive: () => {},
  onDelete: () => {},
  onScale: () => {},
};
const columns = [
  create_column("column_1", "Ready", "", [
    create_task("task_1", "First"),
    create_task("task_2", "Second"),
    create_task("task_3", "Not on canvas"),
  ]),
];

describe("Flow task projection", () => {
  it("anchors lines to every card border without moving connector circles inward", () => {
    const bounds = { x: -100, y: 80, width: 256, height: 120 };
    for (const radius of [8, 12, 16]) {
      expect(
        card_edge_point(-100 - radius, 140, Position.Left, bounds),
      ).toEqual({ x: -100, y: 140 });
      expect(
        card_edge_point(156 + radius, 140, Position.Right, bounds),
      ).toEqual({ x: 156, y: 140 });
      expect(card_edge_point(28, 80 - radius, Position.Top, bounds)).toEqual({
        x: 28,
        y: 80,
      });
      expect(
        card_edge_point(28, 200 + radius, Position.Bottom, bounds),
      ).toEqual({ x: 28, y: 200 });
    }
    expect(card_edge_point(50, 70, Position.Right)).toEqual({ x: 50, y: 70 });
  });
  it("offers only active tasks and reads archived details only for missing canvas members", () => {
    const members = new Set(["task_1", "task_99"]);
    expect(
      available_task_columns(columns, members).flatMap((column) =>
        column.tasks.map((task) => task.id),
      ),
    ).toEqual(["task_2", "task_3"]);
    expect(missing_archive_task_ids(layout, columns)).toEqual([99]);
    expect(
      missing_archive_task_ids({ nodes: [], viewport: null }, columns),
    ).toEqual([]);
    const remaining = [
      create_column("column_1", "Ready", "", [create_task("task_2", "Second")]),
    ];
    expect(missing_archive_task_ids(layout, remaining)).toEqual([1, 99]);
  });

  it("dashes only connections with an archived endpoint and returns them to solid on restoration", () => {
    const archive = {
      time: new Date(),
      task: create_task("task_99", "Archived"),
    };
    const relationships = [
      { source: 1, target: 99, source_arrow: false, target_arrow: false },
      { source: 99, target: 2, source_arrow: false, target_arrow: false },
      { source: 1, target: 2, source_arrow: false, target_arrow: false },
    ];
    const edges = build_edges(
      relationships,
      build_nodes(layout, columns, actions, null, [archive]),
      (edge) => `${edge.source}-${edge.target}`,
    );
    expect(edges[0].style).toBe("stroke-dasharray: 5 4");
    expect(edges[1].style).toBe("stroke-dasharray: 5 4");
    expect(edges[2].style).toBeUndefined();
    const restored = [
      ...columns,
      create_column("column_2", "Restored", "", [archive.task]),
    ];
    expect(
      build_edges(
        relationships,
        build_nodes(layout, restored, actions, null, [archive]),
        (edge) => `${edge.source}-${edge.target}`,
      ).every((edge) => edge.style === undefined),
    ).toBe(true);
  });
  it("keeps archived canvas tasks and their connections visible, and projects restored tasks as active", () => {
    const archive = {
      time: new Date(),
      task: create_task("task_99", "Archived"),
    };
    const nodes = build_nodes(layout, columns, actions, null, [archive]);
    expect(nodes[2].data.archived).toBe(true);
    expect(nodes[2].position).toEqual({ x: -40, y: 100 });
    expect(
      build_edges(
        [{ source: 1, target: 99, source_arrow: false, target_arrow: false }],
        nodes,
        (edge) => `${edge.source}-${edge.target}`,
      ),
    ).toHaveLength(1);
    const restored = [create_column("column_1", "Ready", "", [archive.task])];
    expect(
      build_nodes(layout, restored, actions, null, [archive])[0].data.archived,
    ).toBe(false);
  });

  it("preserves dragged connection sides when the backend reverses an undirected pair and after reload", () => {
    const relationship = {
      source: 1,
      target: 2,
      source_arrow: false,
      target_arrow: false,
    };
    const saved = connected_layout(layout, relationship, {
      source: "task_2",
      target: "task_1",
      sourceHandle: "right",
      targetHandle: "bottom",
    });
    const restored = JSON.parse(JSON.stringify(saved)) as FlowLayout;
    const edge = build_edges(
      [relationship],
      build_nodes(restored, columns, actions),
      (edge) => `${edge.source}-${edge.target}`,
      restored.connections,
    )[0];
    expect(edge.sourceHandle).toBe("bottom");
    expect(edge.targetHandle).toBe("right");
    // Lines are labelled only for assistive technology, never on the canvas.
    expect(edge.label).toBeUndefined();
    expect(edge.ariaLabel).toBe("1-2");
  });

  it("keeps every connector side through movement, pan, zoom, further additions, and serialization", () => {
    const sides = ["left", "right", "top", "bottom"];
    for (const sourceHandle of sides) {
      for (const targetHandle of sides) {
        for (const reverse of [false, true]) {
          const relationship = {
            source: 1,
            target: 2,
            source_arrow: false,
            target_arrow: false,
          };
          const connected = connected_layout(layout, relationship, {
            source: reverse ? "task_2" : "task_1",
            target: reverse ? "task_1" : "task_2",
            sourceHandle,
            targetHandle,
          });
          const nodes = build_nodes(connected, columns, actions);
          nodes[0].position = { x: 800, y: -500 };
          const moved = moved_layout(connected, nodes, {
            x: -600,
            y: 300,
            zoom: 0.4,
          });
          const added = added_layout(moved, ["task_3"], moved.viewport!);
          const restored = JSON.parse(JSON.stringify(added)) as FlowLayout;
          const edge = build_edges(
            [relationship],
            build_nodes(restored, columns, actions),
            (edge) => `${edge.source}-${edge.target}`,
            restored.connections,
          )[0];
          expect(edge.sourceHandle).toBe(reverse ? targetHandle : sourceHandle);
          expect(edge.targetHandle).toBe(reverse ? sourceHandle : targetHandle);
          expect(connected.nodes[0].x).toBe(10);
        }
      }
    }
  });
  it("snaps dragged sizes to 5% steps between half and double size", () => {
    expect(clamp_scale(0.83)).toBe(0.85);
    expect(clamp_scale(0.1)).toBe(0.5);
    expect(clamp_scale(3)).toBe(2);
    expect(clamp_scale(1.01)).toBe(1);
  });
  it("scales chosen cards and omits the default size", () => {
    const annotated: FlowLayout = {
      ...layout,
      annotations: [
        { id: "a", kind: "text", text: "", x: 0, y: 0, width: 280 },
        {
          id: "b",
          kind: "text",
          text: "",
          x: 0,
          y: 0,
          width: 280,
          scale: 1.25,
        },
      ],
    };
    const small = scaled_layout(
      annotated,
      new Set(["task_1", "annotation_a"]),
      () => 0.75,
    );
    expect(small.nodes[0]).toStrictEqual({
      task_id: 1,
      x: 10,
      y: 20,
      scale: 0.75,
    });
    expect(small.nodes[1]).toBe(layout.nodes[1]);
    expect(small.annotations?.[0].scale).toBe(0.75);
    expect(build_nodes(small, columns, actions)[0].data.scale).toBe(0.75);
    expect(build_nodes(small, columns, actions)[1].data.scale).toBe(1);
    const reset = scaled_layout(
      small,
      new Set(["task_1", "annotation_b"]),
      () => 1,
    );
    // Stepping changes each card from its own size.
    const larger = scaled_layout(
      annotated,
      new Set(["annotation_a", "annotation_b"]),
      (scale) => clamp_scale(scale + 0.1),
    );
    expect(larger.annotations?.map((entry) => entry.scale)).toEqual([
      1.1, 1.35,
    ]);
    expect(reset.nodes[0]).toStrictEqual({ task_id: 1, x: 10, y: 20 });
    expect(reset.annotations?.[1]).not.toHaveProperty("scale");
    // Moving cards keeps their size.
    expect(moved_layout(small, [], small.viewport!).nodes[0].scale).toBe(0.75);
  });
  it("uses only canvas members and current task data, with existing editor callbacks", () => {
    const nodes = build_nodes(layout, columns, actions);
    expect(nodes.map((node) => node.id)).toEqual(["task_1", "task_2"]);
    expect(nodes[0].data.task).toBe(columns[0].tasks[0]);
    expect(nodes[0].data.onEdit).toBe(actions.onEdit);
    const changed = [
      create_column("column_2", "Done", "", [create_task("task_1", "Updated")]),
    ];
    expect(build_nodes(layout, changed, actions)[0].data).toMatchObject({
      task: { title: "Updated" },
      column: "Done",
    });
  });

  it("filters nodes and edges using existing search membership without deleting saved data", () => {
    const nodes = build_nodes(layout, columns, actions, new Set(["task_1"]));
    expect(nodes[0].hidden).toBe(false);
    expect(nodes[1].hidden).toBe(true);
    expect(
      build_edges(
        [{ source: 1, target: 2, source_arrow: false, target_arrow: false }],
        nodes,
        (edge) => `${edge.source}-${edge.target}`,
      ),
    ).toEqual([]);
    expect(layout.nodes).toHaveLength(3);
  });

  it("persists moved positions and viewport without storing task content or losing archived positions", () => {
    const nodes = build_nodes(layout, columns, actions);
    nodes[0].position = { x: 500, y: -50 };
    const moved = moved_layout(layout, nodes, { x: 100, y: -100, zoom: 0.5 });
    expect(moved.nodes[0]).toEqual({ task_id: 1, x: 500, y: -50 });
    expect(moved.nodes[2]).toEqual(layout.nodes[2]);
    expect(moved.viewport?.zoom).toBe(0.5);
    expect(layout.nodes[0].x).toBe(10);
  });

  it("draws large arrows only at the ends that ask for them", () => {
    const nodes = build_nodes(layout, columns, actions);
    const label = () => "";
    const arrow = { type: "arrowclosed", width: 22, height: 22 };
    const [plain, forward, backward, both] = [
      [false, false],
      [false, true],
      [true, false],
      [true, true],
    ].map(
      ([source_arrow, target_arrow]) =>
        build_edges(
          [{ source: 1, target: 2, source_arrow, target_arrow }],
          nodes,
          label,
        )[0],
    );
    expect(plain.markerStart).toBeUndefined();
    expect(plain.markerEnd).toBeUndefined();
    expect(forward).toMatchObject({ markerEnd: arrow });
    expect(forward.markerStart).toBeUndefined();
    expect(backward).toMatchObject({ markerStart: arrow });
    expect(backward.markerEnd).toBeUndefined();
    expect(both).toMatchObject({ markerStart: arrow, markerEnd: arrow });
    expect(plain.id).toBe("1:2");
  });

  it("integrates with native views and keeps canvas removal separate from task deletion", () => {
    const read = (path: string) =>
      readFileSync(new URL(path, import.meta.url), "utf8");
    expect(read("../workspace/workspace_tabs.svelte")).toContain('id: "flow"');
    expect(read("../../+page.svelte")).toContain("<FlowView");
    const view = read("./flow_view.svelte");
    expect(view).not.toContain('invoke("delete_task"');
    expect(view).toContain('"create_task_relationship"');
    expect(view).toContain("translateCommandError(error)");
    expect(view).toContain("var(--background)");
    expect(view).toContain("var(--primary)");
  });
});
