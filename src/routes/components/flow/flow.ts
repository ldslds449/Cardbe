import type { Node, Edge, Viewport, Connection } from "@xyflow/svelte";
import { MarkerType, Position } from "@xyflow/svelte";
import type { Column } from "../../type/column.svelte";
import type { Task } from "../../type/task.svelte";
import type { Archive } from "../../type/archive.svelte";

export type RelationshipKind = "related_to" | "depends_on";
export interface Relationship {
  source: number;
  target: number;
  kind: RelationshipKind;
}
export interface FlowLayout {
  annotations?: FlowAnnotation[];
  nodes: { task_id: number; x: number; y: number }[];
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
    read_only: boolean;
    onEdit: (annotation: FlowAnnotation) => void;
    onRemove: (id: string) => void;
    onResize: () => void;
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
    | "onEdit"
    | "onRemove"
    | "onResize"
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
    width: annotation.width,
    data: {
      annotation,
      note:
        annotation.kind === "note" ? by_id.get(annotation.note_id) : undefined,
      ...actions,
    },
  }));
}
export interface FlowData {
  layout: FlowLayout;
  relationships: Relationship[];
}
export type TaskNode = Node<
  {
    task: Task;
    column: string;
    read_only: boolean;
    archived: boolean;
    onOpen: (task: Task) => void;
    onEdit: (task: Task) => void;
    onRemove: (id: string) => void;
    onArchive: (task: Task) => void;
    onDelete: (task: Task) => void;
  },
  "task"
>;
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

export function relationship_id(edge: Relationship): string {
  return `${edge.kind}:${edge.source}:${edge.target}`;
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
    "read_only" | "onOpen" | "onEdit" | "onRemove" | "onArchive" | "onDelete"
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
            data: { ...entry, ...actions },
            ariaLabel: entry.task.title,
          },
        ]
      : [];
  });
}

export function build_edges(
  relationships: Relationship[],
  nodes: CanvasNode[],
  label: (kind: RelationshipKind) => string,
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
      ariaLabel: label(edge.kind),
      style:
        archived.has(`task_${edge.source}`) ||
        archived.has(`task_${edge.target}`)
          ? "stroke-dasharray: 5 4"
          : undefined,
      markerEnd:
        edge.kind === "depends_on"
          ? { type: MarkerType.ArrowClosed }
          : undefined,
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
            return {
              ...annotation,
              ...node?.position,
              width: node?.width ?? node?.measured?.width ?? annotation.width,
            };
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
