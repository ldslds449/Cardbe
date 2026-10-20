import type { Node, Edge, Viewport, Connection } from "@xyflow/svelte";
import { MarkerType, Position } from "@xyflow/svelte";
import type { Column } from "../../type/column.svelte";
import type { Task } from "../../type/task.svelte";
import type { Archive } from "../../type/archive.svelte";
import * as m from "$lib/paraglide/messages";

// Arrows are only visual; each pair of tasks has at most one line.
export interface Relationship {
  source: number;
  target: number;
  source_arrow: boolean;
  target_arrow: boolean;
}
export interface FlowLayout {
  annotations?: FlowAnnotation[];
  // Scale is omitted while a card keeps the default size.
  nodes: { task_id: number; x: number; y: number; scale?: number }[];
  viewport: Viewport | null;
  connections?: Record<
    string,
    { source_handle: string; target_handle: string }
  >;
}
export type FlowAnnotation = {
  id: string;
  x: number;
  y: number;
  width: number;
  scale?: number;
} & ({ kind: "text"; text: string } | { kind: "note"; note_id: number });
export interface NotePreview {
  id: number;
  title: string;
  content: string;
}
export type AnnotationNode = Node<
  {
    annotation: FlowAnnotation;
    note?: NotePreview;
    notes_unavailable: boolean;
    // Also true while saving; `locked` is only the board's read-only state.
    read_only: boolean;
    locked: boolean;
    onEdit: (annotation: FlowAnnotation) => void;
    onRemove: (id: string) => void;
    onScale: (
      id: string,
      change: (scale: number) => number,
      selection: boolean,
    ) => void;
    editing_id: string | null;
    onSaveText: (id: string, text: string) => Promise<boolean>;
    onCancelEdit: () => void;
  },
  "annotation"
>;
export type CanvasNode = TaskNode | AnnotationNode;

export function build_annotations(
  layout: FlowLayout,
  notes: NotePreview[],
  actions: Pick<
    AnnotationNode["data"],
    | "read_only"
    | "locked"
    | "onEdit"
    | "onRemove"
    | "onScale"
    | "notes_unavailable"
    | "editing_id"
    | "onSaveText"
    | "onCancelEdit"
  >,
): AnnotationNode[] {
  const by_id = new Map(notes.map((note) => [note.id, note]));
  return (layout.annotations ?? []).map((annotation) => ({
    id: `annotation_${annotation.id}`,
    type: "annotation",
    position: { x: annotation.x, y: annotation.y },
    // The stored width is unscaled; the card's content is zoomed to fit.
    width: annotation.width * (annotation.scale ?? 1),
    data: {
      annotation,
      note:
        annotation.kind === "note" ? by_id.get(annotation.note_id) : undefined,
      ...actions,
    },
  }));
}
export interface FlowTab {
  id: string;
  name: string;
}

// The default tab is stored without a name.
export function tab_label(tab: FlowTab): string {
  return tab.name.trim() || m.flow_tab_default();
}
export interface FlowData {
  tab_id: string;
  layout: FlowLayout;
  relationships: Relationship[];
  tabs: FlowTab[];
}

export function moved_tab_ids(
  tabs: FlowTab[],
  id: string,
  offset: number,
): string[] | null {
  const ids = tabs.map((tab) => tab.id);
  const from = ids.indexOf(id);
  const to = from + offset;
  if (from < 0 || to < 0 || to >= ids.length || offset === 0) {
    return null;
  }
  ids.splice(from, 1);
  ids.splice(to, 0, id);
  return ids;
}

// Prefer the tab to the right, matching common browser tab behavior.
// Order while a tab is dragged over another tab; null when nothing moves.
export function dragged_tab_ids(
  ids: string[],
  source: string,
  target: string,
): string[] | null {
  const from = ids.indexOf(source);
  const to = ids.indexOf(target);
  if (from === -1 || to === -1 || from === to) {
    return null;
  }
  const next = [...ids];
  next.splice(to, 0, ...next.splice(from, 1));
  return next;
}

export function tab_after_delete(tabs: FlowTab[], id: string): string | null {
  const index = tabs.findIndex((tab) => tab.id === id);
  const remaining = tabs.filter((tab) => tab.id !== id);
  return (
    remaining[Math.min(Math.max(index, 0), remaining.length - 1)]?.id ?? null
  );
}
export type TaskNode = Node<
  {
    task: Task;
    column: string;
    // Also true while saving; `locked` is only the board's read-only state.
    read_only: boolean;
    locked: boolean;
    archived: boolean;
    onOpen: (task: Task) => void;
    onEdit: (task: Task) => void;
    onRemove: (id: string) => void;
    onArchive: (task: Task) => void;
    onDelete: (task: Task) => void;
    onScale: (
      id: string,
      change: (scale: number) => number,
      selection: boolean,
    ) => void;
    scale: number;
  },
  "task"
>;
// Card sizes scale a whole card, including its text, and snap to 5% steps between 50% and 200%.
export function clamp_scale(value: number): number {
  return Math.min(2, Math.max(0.5, Math.round(value * 20) / 20));
}

// Resizes the given task and annotation nodes, omitting the default size.
export function scaled_layout(
  layout: FlowLayout,
  ids: ReadonlySet<string>,
  change: (scale: number) => number,
): FlowLayout {
  const resized = <T extends { scale?: number }>(entry: T): T => {
    const { scale: current, ...rest } = entry;
    const scale = change(current ?? 1);
    return (scale === 1 ? rest : { ...rest, scale }) as T;
  };
  return {
    ...layout,
    nodes: layout.nodes.map((node) =>
      ids.has(`task_${node.task_id}`) ? resized(node) : node,
    ),
    ...(layout.annotations
      ? {
          annotations: layout.annotations.map((annotation) =>
            ids.has(`annotation_${annotation.id}`)
              ? resized(annotation)
              : annotation,
          ),
        }
      : {}),
  };
}
export type RelationshipEdge = Edge<{ relationship: Relationship }>;

export function card_edge_point(
  x: number,
  y: number,
  side: Position,
  bounds?: { x: number; y: number; width: number; height: number },
): { x: number; y: number } {
  if (!bounds) {
    return { x, y };
  }
  switch (side) {
    case Position.Left:
      return { x: bounds.x, y };
    case Position.Right:
      return { x: bounds.x + bounds.width, y };
    case Position.Top:
      return { x, y: bounds.y };
    case Position.Bottom:
      return { x, y: bounds.y + bounds.height };
  }
}

export function available_task_columns(
  columns: Column[],
  canvas_ids: ReadonlySet<string>,
): Column[] {
  return columns
    .map((column) => ({
      ...column,
      tasks: column.tasks.filter(
        (task) => /^task_\d+$/.test(task.id) && !canvas_ids.has(task.id),
      ),
    }))
    .filter((column) => column.tasks.length > 0);
}

export function missing_archive_task_ids(
  layout: FlowLayout,
  columns: Column[],
): number[] {
  const active = new Set(
    columns.flatMap((column) => column.tasks.map((task) => task.id)),
  );
  return layout.nodes
    .filter((node) => !active.has(`task_${node.task_id}`))
    .map((node) => node.task_id);
}

export function added_layout(
  layout: FlowLayout,
  ids: string[],
  viewport: Viewport,
  position = {
    x: (40 - viewport.x) / viewport.zoom,
    y: (40 - viewport.y) / viewport.zoom,
  },
): FlowLayout {
  const existing = new Set(layout.nodes.map((node) => `task_${node.task_id}`));
  const additions = [...new Set(ids)].filter((id) => !existing.has(id));
  return {
    ...layout,
    nodes: [
      ...layout.nodes,
      ...additions.map((id, index) => ({
        task_id: Number(id.slice(5)),
        x: position.x + (index % 3) * 300,
        y: position.y + Math.floor(index / 3) * 160,
      })),
    ],
  };
}

// Adds one task near the saved viewport's top-left, below any item already placed there.
export function appended_layout(
  layout: FlowLayout,
  task_id: number,
): FlowLayout | null {
  if (layout.nodes.some((node) => node.task_id === task_id)) {
    return null;
  }
  const viewport = layout.viewport ?? { x: 0, y: 0, zoom: 1 };
  const position = {
    x: (40 - viewport.x) / viewport.zoom,
    y: (40 - viewport.y) / viewport.zoom,
  };
  const taken = [...layout.nodes, ...(layout.annotations ?? [])];
  while (
    taken.some(
      (item) =>
        Math.abs(item.x - position.x) < 1 && Math.abs(item.y - position.y) < 1,
    )
  ) {
    position.y += 160;
  }
  return added_layout(layout, [`task_${task_id}`], viewport, position);
}

export function relationship_id(edge: Relationship): string {
  return `${edge.source}:${edge.target}`;
}

export function connected_layout(
  layout: FlowLayout,
  relationship: Relationship,
  connection: Connection,
): FlowLayout {
  const reversed = `task_${relationship.source}` !== connection.source;
  return {
    ...layout,
    connections: {
      ...layout.connections,
      [relationship_id(relationship)]: {
        source_handle:
          (reversed ? connection.targetHandle : connection.sourceHandle) ??
          "right",
        target_handle:
          (reversed ? connection.sourceHandle : connection.targetHandle) ??
          "left",
      },
    },
  };
}

export function build_nodes(
  layout: FlowLayout,
  columns: Column[],
  actions: Pick<
    TaskNode["data"],
    | "read_only"
    | "locked"
    | "onOpen"
    | "onEdit"
    | "onRemove"
    | "onArchive"
    | "onDelete"
    | "onScale"
  >,
  search_ids: ReadonlySet<string> | null = null,
  archives: Pick<Archive, "task">[] = [],
): TaskNode[] {
  const tasks = new Map<
    string,
    { task: Task; column: string; archived: boolean }
  >(
    columns.flatMap((column) =>
      column.tasks.map(
        (task) =>
          [task.id, { task, column: column.name, archived: false }] as const,
      ),
    ),
  );
  for (const archive of archives) {
    if (!tasks.has(archive.task.id)) {
      tasks.set(archive.task.id, {
        task: archive.task,
        column: "",
        archived: true,
      });
    }
  }
  return layout.nodes.flatMap((position) => {
    const id = `task_${position.task_id}`;
    const entry = tasks.get(id);
    return entry
      ? [
          {
            id,
            type: "task" as const,
            position: { x: position.x, y: position.y },
            hidden: search_ids !== null && !search_ids.has(id),
            data: { ...entry, ...actions, scale: position.scale ?? 1 },
            ariaLabel: entry.task.title,
          },
        ]
      : [];
  });
}

const arrow = { type: MarkerType.ArrowClosed, width: 22, height: 22 };

export function build_edges(
  relationships: Relationship[],
  nodes: CanvasNode[],
  label: (edge: Relationship) => string,
  connections: FlowLayout["connections"] = {},
): RelationshipEdge[] {
  const visible = new Set(
    nodes.filter((node) => !node.hidden).map((node) => node.id),
  );
  const archived = new Set(
    nodes
      .filter((node) => node.type === "task" && node.data.archived)
      .map((node) => node.id),
  );
  return relationships
    .filter(
      (edge) =>
        visible.has(`task_${edge.source}`) &&
        visible.has(`task_${edge.target}`),
    )
    .map((edge) => ({
      id: relationship_id(edge),
      type: "task",
      source: `task_${edge.source}`,
      target: `task_${edge.target}`,
      sourceHandle:
        connections?.[relationship_id(edge)]?.source_handle ?? "right",
      targetHandle:
        connections?.[relationship_id(edge)]?.target_handle ?? "left",
      ariaLabel: label(edge),
      style:
        archived.has(`task_${edge.source}`) ||
        archived.has(`task_${edge.target}`)
          ? "stroke-dasharray: 5 4"
          : undefined,
      markerStart: edge.source_arrow ? arrow : undefined,
      markerEnd: edge.target_arrow ? arrow : undefined,
      data: { relationship: edge },
    }));
}

// Preserve positions for archived/filtered tasks absent from the rendered nodes.
export function moved_layout(
  layout: FlowLayout,
  nodes: CanvasNode[],
  viewport: Viewport,
): FlowLayout {
  const positions = new Map(nodes.map((node) => [node.id, node.position]));
  const rendered = new Map(nodes.map((node) => [node.id, node]));
  return {
    ...layout,
    ...(layout.annotations
      ? {
          annotations: layout.annotations.map((annotation) => {
            const node = rendered.get(`annotation_${annotation.id}`);
            return { ...annotation, ...node?.position };
          }),
        }
      : {}),
    nodes: layout.nodes.map((node) => ({
      ...node,
      ...positions.get(`task_${node.task_id}`),
    })),
    viewport: { ...viewport },
  };
}
