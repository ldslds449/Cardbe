<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import * as InputGroup from "$lib/components/ui/input-group";
  import { ScrollArea } from "$lib/components/ui/scroll-area";
  import Markdown from "../markdown.svelte";
  import SearchIcon from "@lucide/svelte/icons/search";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import AnnotationNodeComponent from "./annotation_node.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import {
    SvelteFlow,
    Background,
    Controls,
    ConnectionMode,
    SelectionMode,
    type Edge,
    type Viewport,
    type Connection,
    type OnBeforeDelete,
  } from "@xyflow/svelte";
  import "@xyflow/svelte/dist/style.css";
  import { Button, buttonVariants } from "$lib/components/ui/button";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu";
  import * as ContextMenu from "$lib/components/ui/context-menu";
  import * as Popover from "$lib/components/ui/popover";
  import PlusIcon from "@lucide/svelte/icons/plus";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import CircleHelpIcon from "@lucide/svelte/icons/circle-help";
  import ListPlusIcon from "@lucide/svelte/icons/list-plus";
  import TypeIcon from "@lucide/svelte/icons/type";
  import FileTextIcon from "@lucide/svelte/icons/file-text";
  import UnlinkIcon from "@lucide/svelte/icons/unlink";
  import * as Dialog from "$lib/components/ui/dialog";
  import * as Field from "$lib/components/ui/field";
  import * as Empty from "$lib/components/ui/empty";
  import TaskSelectionPicker from "../task/task_selection_picker.svelte";
  import { toast } from "svelte-sonner";
  import {
    parseCommandError,
    translateCommandError,
  } from "$lib/command-errors";
  import { formatNumber } from "$lib/i18n";
  import * as m from "$lib/paraglide/messages";
  import type { Column } from "../../type/column.svelte";
  import {
    get_task_id,
    deserialize_task,
    type TaskSerialized,
    type Task,
  } from "../../type/task.svelte";
  import type { Archive } from "../../type/archive.svelte";
  import TaskNodeComponent from "./task_node.svelte";
  import TaskEdgeComponent from "./task_edge.svelte";
  import {
    build_nodes,
    build_annotations,
    type CanvasNode,
    type FlowAnnotation,
    type NotePreview,
    build_edges,
    moved_layout,
    added_layout,
    connected_layout,
    available_task_columns,
    missing_archive_task_ids,
    type RelationshipEdge,
    type FlowData,
    type FlowLayout,
    type Relationship,
    type RelationshipKind,
  } from "./flow";

  let {
    onOpenNote,
    note_revision = 0,
    board_id,
    columns,
    archives = [],
    onViewArchivedTask,
    onArchiveTask,
    onDeleteTask,
    search_task_ids = null,
    read_only = false,
    onViewTask,
    onEditTask,
  }: {
    onOpenNote: (id: number) => void;
    note_revision?: number;
    board_id: number;
    columns: Column[];
    archives?: Archive[];
    onViewArchivedTask: (task: Task) => void;
    onArchiveTask: (task: Task) => Promise<boolean>;
    onDeleteTask: (task: Task) => void;
    search_task_ids?: ReadonlySet<string> | null;
    read_only?: boolean;
    onViewTask: (task: Task) => void;
    onEditTask: (task: Task) => void;
  } = $props();
  const nodeTypes = {
    task: TaskNodeComponent,
    annotation: AnnotationNodeComponent,
  };
  const edgeTypes = { task: TaskEdgeComponent };
  let data = $state.raw<FlowData>({
    layout: { nodes: [], viewport: null },
    relationships: [],
  });
  let nodes = $state.raw<CanvasNode[]>([]);
  let edges = $state.raw<RelationshipEdge[]>([]);
  let viewport = $state.raw<Viewport>({ x: 0, y: 0, zoom: 1 });
  let loaded = $state(false);
  let load_error = $state.raw<unknown>(null);
  let saving = $state(false);
  let add_open = $state(false);
  let selected_tasks = $state<string[]>([]);
  let selected_task_columns = $state<string[]>([]);
  let task_query = $state("");
  let canvas_archives = $state.raw<Pick<Archive, "task">[]>([]);
  let archive_loading = $state(false);
  let archive_error = $state.raw<unknown>(null);
  let canvas_ready = $state(false);
  let archive_request = 0;
  let notes = $state.raw<NotePreview[]>([]);
  let notes_loading = $state(false);
  let notes_error = $state.raw<unknown>(null);
  let notes_request = 0;
  let note_picker_open = $state(false);
  let note_query = $state("");
  let selected_note_ids = $state<number[]>([]);
  let editing_annotation_id = $state<string | null>(null);
  let insertion_position = $state.raw<{ x: number; y: number } | null>(null);
  function canvas_contextmenu(event: MouseEvent) {
    if (
      !(event.target instanceof Element) ||
      (!event.target.classList.contains("svelte-flow__pane") &&
        event.target !== event.currentTarget)
    ) {
      event.preventDefault();
      return;
    }
    if (event.target === event.currentTarget) {
      insertion_position = null;
      return;
    }
    const bounds = (event.currentTarget as HTMLElement).getBoundingClientRect();
    insertion_position = {
      x: (event.clientX - bounds.left - viewport.x) / viewport.zoom,
      y: (event.clientY - bounds.top - viewport.y) / viewport.zoom,
    };
  }
  const unplaced_notes = $derived(
    notes.filter(
      (note) =>
        !data.layout.annotations?.some(
          (entry) => entry.kind === "note" && entry.note_id === note.id,
        ),
    ),
  );
  const available_notes = $derived(
    unplaced_notes.filter((note) =>
      (note.title + "\n" + note.content)
        .toLocaleLowerCase()
        .includes(note_query.trim().toLocaleLowerCase()),
    ),
  );
  const selected_notes = $derived(
    unplaced_notes.filter((note) => selected_note_ids.includes(note.id)),
  );
  async function refresh_notes() {
    const request = ++notes_request;
    notes_loading = true;
    notes_error = null;
    try {
      const result = await invoke<NotePreview[]>("get_notes");
      if (!disposed && request === notes_request) {
        notes = result;
      }
    } catch (error) {
      if (!disposed && request === notes_request) {
        notes_error = error;
      }
    } finally {
      if (!disposed && request === notes_request) {
        notes_loading = false;
      }
    }
  }
  $effect(() => {
    void note_revision;
    untrack(() => void refresh_notes());
  });
  function edit_annotation(annotation: FlowAnnotation | null) {
    if (annotation?.kind === "note") {
      onOpenNote(annotation.note_id);
      return;
    }
    if (read_only || saving) {
      return;
    }
    editing_annotation_id = annotation?.id ?? null;
  }
  async function put_annotation(annotation: FlowAnnotation) {
    const layout = moved_layout(data.layout, nodes, viewport);
    const entries = layout.annotations ?? [];
    return save_layout({
      ...layout,
      annotations: [
        ...entries.filter((entry) => entry.id !== annotation.id),
        annotation,
      ],
    });
  }
  function new_annotation_position() {
    return {
      id: crypto.randomUUID(),
      x: insertion_position?.x ?? (80 - viewport.x) / viewport.zoom,
      y: insertion_position?.y ?? (80 - viewport.y) / viewport.zoom,
      width: 280,
    };
  }
  async function add_annotation() {
    const annotation: FlowAnnotation = {
      ...new_annotation_position(),
      kind: "text",
      text: "",
    };
    if (await put_annotation(annotation)) {
      editing_annotation_id = annotation.id;
    }
  }
  async function save_annotation_text(id: string, text: string) {
    const annotation = moved_layout(
      data.layout,
      nodes,
      viewport,
    ).annotations?.find((entry) => entry.id === id);
    if (!annotation || annotation.kind !== "text" || read_only || saving) {
      return false;
    }
    const saved = await put_annotation({ ...annotation, text });
    if (saved && editing_annotation_id === id) {
      editing_annotation_id = null;
    }
    return saved;
  }
  async function add_notes() {
    if (read_only || saving || !loaded || !selected_notes.length) {
      return;
    }
    const layout = moved_layout(data.layout, nodes, viewport);
    const saved = await save_layout({
      ...layout,
      annotations: [
        ...(layout.annotations ?? []),
        ...selected_notes.map((note, index): FlowAnnotation => {
          const position = new_annotation_position();
          return {
            ...position,
            x: position.x + (index % 3) * 320,
            y: position.y + Math.floor(index / 3) * 360,
            kind: "note",
            note_id: note.id,
          };
        }),
      ],
    });
    if (saved) {
      selected_note_ids = [];
      note_picker_open = false;
    }
  }
  let selected_edges = $state<string[]>([]);
  let disposed = false;
  let pending_layout: FlowLayout | null = null;
  function finish_saving() {
    saving = false;
    if (pending_layout && !disposed) {
      const next = { ...pending_layout, connections: data.layout.connections };
      pending_layout = null;
      void save_layout(next);
    }
  }
  const visible_ids = $derived(
    nodes
      .filter((node) => !node.hidden)
      .map((node) => `${node.id}:${node.type === "task" && node.data.archived}`)
      .join(","),
  );
  const ariaLabelConfig = $derived({
    "node.a11yDescription.default": m.flow_node_keyboard(),
    "node.a11yDescription.keyboardDisabled": m.flow_node_readonly_keyboard(),
    "node.a11yDescription.ariaLiveMessage": ({
      x,
      y,
    }: {
      direction: string;
      x: number;
      y: number;
    }) => m.flow_node_moved({ x: formatNumber(x), y: formatNumber(y) }),
    "edge.a11yDescription.default": read_only
      ? m.flow_node_readonly_keyboard()
      : m.flow_edge_keyboard(),
    "controls.ariaLabel": m.flow_controls(),
    "controls.zoomIn.ariaLabel": m.flow_zoom_in(),
    "controls.zoomOut.ariaLabel": m.flow_zoom_out(),
    "controls.fitView.ariaLabel": m.flow_fit_view(),
    "controls.interactive.ariaLabel": m.flow_interactivity(),
    "minimap.ariaLabel": m.flow_minimap(),
    "handle.ariaLabel": m.flow_handle(),
  });
  const relationship_label = (value: RelationshipKind) =>
    value === "depends_on" ? m.flow_depends_on() : m.flow_related_to();
  function update_selection(selection: Edge[]) {
    const ids = selection.map((edge) => edge.id);
    // Selection callbacks also fire when projected arrays change.
    if (ids.join(",") !== selected_edges.join(",")) {
      selected_edges = ids;
    }
  }
  const canvas_ids = $derived(
    new Set(data.layout.nodes.map((node) => `task_${node.task_id}`)),
  );
  const available_columns = $derived(
    available_task_columns(columns, canvas_ids),
  );
  const archive_ids = $derived(
    missing_archive_task_ids(data.layout, columns).join(","),
  );
  $effect(() => {
    const ids = archive_ids;
    void columns;
    void archives;
    if (loaded) {
      untrack(
        () => void refresh_archives(ids ? ids.split(",").map(Number) : []),
      );
    }
  });
  async function refresh_archives(
    ids = missing_archive_task_ids(data.layout, columns),
  ) {
    if (disposed) {
      return;
    }
    const request = ++archive_request;
    archive_loading = ids.length > 0;
    archive_error = null;
    const cached = new Map(
      archives.map((archive) => [get_task_id(archive.task.id), archive.task]),
    );
    const result: Pick<Archive, "task">[] = [];
    try {
      for (const taskId of ids) {
        if (disposed || request !== archive_request) {
          return;
        }
        const task = cached.get(taskId);
        if (task) {
          result.push({ task });
          continue;
        }
        try {
          const serialized = await invoke<TaskSerialized>("get_archive_task", {
            expectedBoardId: board_id,
            taskId,
          });
          result.push({ task: deserialize_task(serialized) });
        } catch (error) {
          if (parseCommandError(error)?.code !== "TASK_NOT_FOUND") {
            throw error;
          }
        }
      }
      if (!disposed && request === archive_request) {
        canvas_archives = result;
      }
    } catch (error) {
      if (!disposed && request === archive_request) {
        archive_error = error;
      }
    } finally {
      if (!disposed && request === archive_request) {
        archive_loading = false;
        canvas_ready = true;
      }
    }
  }
  $effect(() => {
    const next: CanvasNode[] = build_nodes(
      data.layout,
      columns,
      {
        read_only: read_only || saving,
        onOpen: (task) =>
          canvas_archives.some((archive) => archive.task.id === task.id)
            ? onViewArchivedTask(task)
            : onViewTask(task),
        onArchive: async (task) => {
          if (await onArchiveTask(task)) {
            await refresh_archives();
          }
        },
        onDelete: onDeleteTask,
        onEdit: onEditTask,
        onRemove: remove_node,
      },
      search_task_ids,
      canvas_archives,
    );
    next.push(
      ...build_annotations(data.layout, notes, {
        notes_unavailable: notes_loading || notes_error !== null,
        read_only: read_only || saving,
        onEdit: edit_annotation,
        editing_id: editing_annotation_id,
        onSaveText: save_annotation_text,
        onCancelEdit: () => (editing_annotation_id = null),
        onRemove: remove_node,
        onResize: save_positions,
      }),
    );
    untrack(() => {
      const current = new Map(nodes.map((node) => [node.id, node]));
      nodes = next.map((node) => ({
        ...node,
        selected: current.get(node.id)?.selected,
      }));
    });
  });
  $effect(() => {
    void visible_ids;
    edges = build_edges(
      data.relationships,
      untrack(() => nodes),
      relationship_label,
      data.layout.connections,
    ).map((edge) => ({ ...edge, selected: selected_edges.includes(edge.id) }));
  });

  async function load() {
    loaded = false;
    canvas_ready = false;
    archive_request++;
    load_error = null;
    pending_layout = null;
    try {
      const result = await invoke<FlowData>("get_flow", {
        expectedBoardId: board_id,
      });
      if (disposed) {
        return;
      }
      data = result;
      viewport = result.layout.viewport ?? { x: 0, y: 0, zoom: 1 };
      loaded = true;
    } catch (error) {
      if (!disposed) {
        load_error = error;
      }
    }
  }
  onMount(() => {
    void load();
    const on_focus = () => void refresh_notes();
    window.addEventListener("focus", on_focus);
    const listener = listen<{ kind?: string }>(
      "cardbe:data-changed",
      (event) => {
        if (event.payload.kind === "note") {
          void refresh_notes();
        }
      },
    );
    return () => {
      window.removeEventListener("focus", on_focus);
      void listener.then((unlisten) => unlisten());
      disposed = true;
    };
  });

  async function save_layout(layout: FlowLayout) {
    if (read_only || !loaded || disposed) {
      return false;
    }
    if (saving) {
      pending_layout = layout;
      return false;
    }
    data = { ...data, layout };
    saving = true;
    try {
      await invoke("save_flow_layout", { expectedBoardId: board_id, layout });
      if (!disposed) {
        data = { ...data, layout };
      }
      return !disposed;
    } catch (error) {
      pending_layout = null;
      toast.error(translateCommandError(error));
      if (!disposed) {
        await load();
      }
      return false;
    } finally {
      finish_saving();
    }
  }
  function save_positions() {
    void save_layout(moved_layout(data.layout, nodes, viewport));
  }
  function save_keyboard_position(event: KeyboardEvent) {
    if (
      event.key.startsWith("Arrow") &&
      event.target instanceof HTMLElement &&
      event.target.classList.contains("svelte-flow__node")
    ) {
      save_positions();
    }
  }
  async function add_nodes(ids: string[]) {
    if (read_only || saving || !loaded) {
      return;
    }
    const active_ids = new Set(
      columns.flatMap((column) => column.tasks.map((task) => task.id)),
    );
    const saved = await save_layout(
      added_layout(
        moved_layout(data.layout, nodes, viewport),
        ids.filter((id) => active_ids.has(id)),
        viewport,
        insertion_position ?? undefined,
      ),
    );
    if (saved) {
      selected_tasks = [];
      selected_task_columns = [];
      add_open = false;
    }
  }
  function remove_node(id: string) {
    void save_layout({
      ...moved_layout(data.layout, nodes, viewport),
      nodes: data.layout.nodes.filter((node) => `task_${node.task_id}` !== id),
      annotations: (data.layout.annotations ?? []).filter(
        (entry) => `annotation_${entry.id}` !== id,
      ),
    });
  }
  async function create_relationship(connection: Connection) {
    if (read_only || saving || !loaded) {
      return;
    }
    saving = true;
    try {
      const relationship = await invoke<Relationship>(
        "create_task_relationship",
        {
          expectedBoardId: board_id,
          relationship: {
            source: get_task_id(connection.source),
            target: get_task_id(connection.target),
            kind: "related_to",
          },
        },
      );
      if (!disposed) {
        data = {
          ...data,
          relationships: [...data.relationships, relationship],
        };
        const layout = connected_layout(
          pending_layout ?? moved_layout(data.layout, nodes, viewport),
          relationship,
          connection,
        );
        data = { ...data, layout };
        pending_layout = layout;
      }
    } catch (error) {
      toast.error(translateCommandError(error));
    } finally {
      finish_saving();
    }
  }
  async function remove_relationships(ids: string[]) {
    if (read_only || saving || !loaded) {
      return;
    }
    saving = true;
    try {
      for (const id of ids) {
        const relationship = edges.find((edge) => edge.id === id)?.data
          ?.relationship;
        if (!relationship) {
          continue;
        }
        await invoke("remove_task_relationship", {
          expectedBoardId: board_id,
          relationship,
        });
        if (!disposed) {
          data = {
            ...data,
            layout: {
              ...data.layout,
              connections: Object.fromEntries(
                Object.entries(data.layout.connections ?? {}).filter(
                  ([key]) => key !== id,
                ),
              ),
            },
            relationships: data.relationships.filter(
              (edge) =>
                !(
                  edge.source === relationship.source &&
                  edge.target === relationship.target &&
                  edge.kind === relationship.kind
                ),
            ),
          };
        }
      }
      selected_edges = [];
    } catch (error) {
      toast.error(translateCommandError(error));
      if (!disposed) {
        await load();
      }
    } finally {
      finish_saving();
    }
  }
  const before_delete: OnBeforeDelete<CanvasNode, Edge> = async ({
    nodes: removed,
    edges: removed_edges,
  }) => {
    if (removed.length) {
      const ids = new Set(removed.map((node) => node.id));
      await save_layout({
        ...moved_layout(data.layout, nodes, viewport),
        annotations: (data.layout.annotations ?? []).filter(
          (entry) => !ids.has(`annotation_${entry.id}`),
        ),
        nodes: data.layout.nodes.filter(
          (node) => !ids.has(`task_${node.task_id}`),
        ),
      });
    } else {
      await remove_relationships(removed_edges.map((edge) => edge.id));
    }
    // Canvas removal never deletes tasks or their logical relationships.
    return false;
  };
</script>

<div class="flow-canvas flex h-full min-h-0 flex-col bg-background">
  <div class="flex flex-wrap items-center gap-2 border-b p-3">
    <DropdownMenu.Root>
      <DropdownMenu.Trigger
        class={buttonVariants({ size: "sm", variant: "outline" })}
        disabled={read_only || saving || !loaded}
        ><PlusIcon />{m.flow_add_to_canvas()}<ChevronDownIcon
        /></DropdownMenu.Trigger
      >
      <DropdownMenu.Content align="start">
        <DropdownMenu.Item
          disabled={read_only || saving || !loaded}
          onclick={() => {
            insertion_position = null;
            add_open = true;
          }}><ListPlusIcon />{m.flow_add_tasks()}</DropdownMenu.Item
        >
        <DropdownMenu.Item
          disabled={read_only || saving || !loaded}
          onclick={() => {
            insertion_position = null;
            note_query = "";
            selected_note_ids = [];
            note_picker_open = true;
            void refresh_notes();
          }}><FileTextIcon />{m.flow_add_note()}</DropdownMenu.Item
        >
        <DropdownMenu.Separator />
        <DropdownMenu.Item
          disabled={read_only || saving || !loaded}
          onclick={() => {
            insertion_position = null;
            void add_annotation();
          }}><TypeIcon />{m.flow_add_annotation()}</DropdownMenu.Item
        >
      </DropdownMenu.Content>
    </DropdownMenu.Root>
    {#if selected_edges.length > 0}
      <Button
        size="sm"
        variant="ghost"
        disabled={read_only || saving}
        onclick={() => remove_relationships(selected_edges)}
        ><UnlinkIcon />{m.flow_disconnect()}</Button
      >
    {/if}
    <div class="ml-auto flex items-center gap-2">
      {#if saving || archive_loading}
        <span class="text-xs text-muted-foreground" role="status">
          {saving ? m.flow_saving() : m.board_loading()}
        </span>
      {/if}
      <Popover.Root>
        <Popover.Trigger
          class={buttonVariants({ size: "icon-sm", variant: "ghost" })}
          aria-label={m.flow_canvas_help()}
          title={m.flow_canvas_help()}><CircleHelpIcon /></Popover.Trigger
        >
        <Popover.Content align="end" class="w-72 space-y-2 text-sm">
          <p class="font-medium">{m.flow_canvas_help()}</p>
          <p class="whitespace-pre-line leading-7 text-muted-foreground">
            {m.flow_help()}
          </p>
          <p class="text-xs text-muted-foreground">{m.flow_local()}</p>
        </Popover.Content>
      </Popover.Root>
    </div>
  </div>
  {#if archive_error}
    <div
      role="alert"
      class="flex items-center gap-2 px-3 text-sm text-destructive"
    >
      <span>{translateCommandError(archive_error)}</span>
      <Button size="sm" variant="link" onclick={() => refresh_archives()}
        >{m.board_search_retry()}</Button
      >
    </div>
  {/if}
  {#if notes_error}
    <div
      role="alert"
      class="flex items-center gap-2 px-3 text-sm text-destructive"
    >
      <span>{translateCommandError(notes_error)}</span>
      <Button size="sm" variant="link" onclick={refresh_notes}
        >{m.note_retry()}</Button
      >
    </div>
  {/if}
  <ContextMenu.Root>
    <ContextMenu.Trigger
      class="relative min-h-0 flex-1"
      tabindex={0}
      aria-label={m.workspace_flow()}
      aria-haspopup="menu"
      disabled={read_only || !loaded}
      oncontextmenu={canvas_contextmenu}
    >
      {#if loaded && canvas_ready}
        <SvelteFlow
          bind:nodes
          bind:edges
          bind:viewport
          {nodeTypes}
          {edgeTypes}
          connectionMode={ConnectionMode.Loose}
          proOptions={{ hideAttribution: true }}
          {ariaLabelConfig}
          defaultMarkerColor="var(--muted-foreground)"
          onkeyup={save_keyboard_position}
          minZoom={0.1}
          maxZoom={4}
          fitView
          fitViewOptions={{ padding: 0.2, minZoom: 0.1, maxZoom: 1 }}
          panOnScroll
          zoomOnScroll={false}
          panOnDrag={[1]}
          panActivationKey={null}
          selectionOnDrag
          selectionKey={null}
          selectionMode={SelectionMode.Partial}
          onlyRenderVisibleElements
          nodesDraggable={!read_only && !saving}
          nodesConnectable={!read_only && !saving}
          deleteKey={read_only || saving ? null : ["Backspace", "Delete"]}
          onnodedragstop={save_positions}
          onselectiondragstop={save_positions}
          onmoveend={save_positions}
          onbeforeconnect={(connection) => {
            void create_relationship(connection);
            return false;
          }}
          onbeforedelete={before_delete}
          onselectionchange={({ edges: selection }) =>
            update_selection(selection)}
        >
          <Background patternColor="var(--border)" />
          <Controls
            showLock={false}
            fitViewOptions={{ padding: 0.2, duration: 200 }}
          />
        </SvelteFlow>
        {#if nodes.length === 0 && !archive_loading && !archive_error}
          <div
            class="pointer-events-none absolute inset-0 grid place-items-center p-6"
          >
            <Empty.Root class="bg-card/90">
              <Empty.Header
                ><Empty.Title>{m.flow_empty()}</Empty.Title><Empty.Description
                  >{m.flow_empty_description()}</Empty.Description
                ></Empty.Header
              >
              <Empty.Content
                ><Button
                  class="pointer-events-auto"
                  disabled={read_only || saving}
                  onclick={() => {
                    insertion_position = null;
                    add_open = true;
                  }}>{m.flow_add_tasks()}</Button
                ></Empty.Content
              >
            </Empty.Root>
          </div>
        {/if}
      {:else if load_error}
        <div class="grid h-full place-content-center gap-3 p-6 text-center">
          <p>{translateCommandError(load_error)}</p>
          <Button onclick={load}>{m.board_search_retry()}</Button>
        </div>
      {:else}<div class="grid h-full place-items-center text-muted-foreground">
          {m.board_loading()}
        </div>{/if}
    </ContextMenu.Trigger>
    <ContextMenu.Content>
      <ContextMenu.Item
        disabled={read_only || saving || !loaded}
        onclick={() => (add_open = true)}
        ><ListPlusIcon />{m.flow_add_tasks()}</ContextMenu.Item
      >
      <ContextMenu.Item
        disabled={read_only || saving || !loaded}
        onclick={() => {
          note_query = "";
          selected_note_ids = [];
          note_picker_open = true;
          void refresh_notes();
        }}><FileTextIcon />{m.flow_add_note()}</ContextMenu.Item
      >
      <ContextMenu.Separator />
      <ContextMenu.Item
        disabled={read_only || saving || !loaded}
        onclick={add_annotation}
        ><TypeIcon />{m.flow_add_annotation()}</ContextMenu.Item
      >
    </ContextMenu.Content>
  </ContextMenu.Root>
</div>

<Dialog.Root bind:open={add_open}>
  <Dialog.Content
    class="flex max-h-[calc(100dvh-4rem)] flex-col overflow-hidden"
  >
    <Dialog.Header
      ><Dialog.Title>{m.flow_add_tasks()}</Dialog.Title><Dialog.Description
        >{m.flow_add_description()}</Dialog.Description
      ></Dialog.Header
    >
    <Field.Group class="min-h-0 gap-4">
      <TaskSelectionPicker
        columns={available_columns}
        bind:query={task_query}
        bind:selected_column_ids={selected_task_columns}
        bind:selected_task_ids={selected_tasks}
        disabled={read_only || saving || !loaded}
      />

      <Button
        disabled={!selected_tasks.length || read_only || saving || !loaded}
        onclick={() => add_nodes(selected_tasks)}
        >{m.flow_add_selected({
          count: formatNumber(selected_tasks.length),
        })}</Button
      >
    </Field.Group>
  </Dialog.Content>
</Dialog.Root>

<Dialog.Root bind:open={note_picker_open}>
  <Dialog.Content class="gap-0 overflow-hidden p-0 sm:max-w-xl">
    <Dialog.Header class="px-6 pt-6 pb-4 text-start"
      ><Dialog.Title>{m.flow_add_note()}</Dialog.Title><Dialog.Description
        >{m.flow_note_description()}</Dialog.Description
      ></Dialog.Header
    >
    <div class="px-6 pb-4">
      <InputGroup.Root>
        <InputGroup.Input
          aria-label={m.note_search()}
          placeholder={m.note_search()}
          bind:value={note_query}
        />
        <InputGroup.Addon><SearchIcon /></InputGroup.Addon>
      </InputGroup.Root>
    </div>
    <ScrollArea class="h-80 border-t">
      <div class="p-2">
        {#if notes_loading}<p
            class="p-8 text-center text-sm text-muted-foreground"
            role="status"
          >
            {m.note_loading()}
          </p>
        {:else if notes_error}<p
            class="px-4 pt-6 text-sm text-destructive"
            role="alert"
          >
            {translateCommandError(notes_error)}
          </p>
          <Button class="mx-4 mt-3" variant="outline" onclick={refresh_notes}
            >{m.note_retry()}</Button
          >
        {:else}
          {#each available_notes as note (note.id)}<label
              class={`flex cursor-pointer items-start gap-3 rounded-md px-4 py-4 text-left hover:bg-muted/50 ${selected_note_ids.includes(note.id) ? "bg-accent/50" : ""}`}
              ><Checkbox
                class="mt-3"
                disabled={read_only || saving || !loaded}
                checked={selected_note_ids.includes(note.id)}
                aria-label={note.title.trim() || m.note_untitled()}
                onCheckedChange={(checked) => {
                  selected_note_ids = selected_note_ids.filter(
                    (id) => id !== note.id,
                  );
                  if (checked === true) {
                    selected_note_ids = [...selected_note_ids, note.id];
                  }
                }}
              /><span
                class="flex size-10 shrink-0 items-center justify-center rounded-md border bg-muted/40 text-muted-foreground"
                ><FileTextIcon class="size-5" /></span
              >
              <div class="min-w-0 flex-1">
                <p class="line-clamp-2 break-words text-sm font-medium">
                  {note.title.trim() || m.note_untitled()}
                </p>
                <div
                  inert
                  class="pointer-events-none mt-1 max-h-10 overflow-hidden break-words text-xs font-normal leading-5 text-muted-foreground [&_.markdown-render]:!text-xs [&_.markdown-render]:!leading-5"
                >
                  <Markdown
                    md={Array.from(note.content).slice(0, 500).join("") ||
                      m.note_empty()}
                    compact
                  />
                </div>
              </div>
            </label>
          {:else}<div
              class="flex flex-col items-center gap-3 px-4 py-12 text-center text-muted-foreground"
            >
              <FileTextIcon class="size-8 opacity-50" />
              <p class="text-sm">
                {note_query.trim()
                  ? m.note_no_matching()
                  : m.flow_no_available_notes()}
              </p>
            </div>{/each}
        {/if}
      </div>
    </ScrollArea>
    <Dialog.Footer class="border-t px-6 py-4">
      <Button
        disabled={!selected_notes.length ||
          notes_loading ||
          notes_error !== null ||
          read_only ||
          saving ||
          !loaded}
        onclick={add_notes}
      >
        {m.flow_add_selected_notes({
          count: formatNumber(selected_notes.length),
        })}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<style>
  .flow-canvas {
    --xy-background-color: var(--background);
    --xy-edge-stroke: var(--muted-foreground);
    --xy-edge-stroke-selected: var(--primary);
    --xy-edge-label-color: var(--foreground);
    --xy-edge-label-background-color: var(--background);
    --xy-controls-button-background-color: var(--card);
    --xy-controls-button-background-color-hover: var(--accent);
    --xy-controls-button-color: var(--card-foreground);
    --xy-controls-button-border-color: var(--border);
    --xy-handle-background-color: var(--primary);
    --xy-handle-border-color: var(--background);
    --xy-selection-background-color: color-mix(
      in srgb,
      var(--primary) 12%,
      transparent
    );
    --xy-selection-border: 1px solid var(--primary);
  }
  .flow-canvas :global(.svelte-flow__node:focus-visible) {
    outline: 2px solid var(--ring);
    outline-offset: 3px;
  }
  .flow-canvas :global(.svelte-flow__pane.selection),
  .flow-canvas :global(.svelte-flow__node.draggable),
  .flow-canvas :global(.svelte-flow__nodesselection-rect) {
    cursor: default;
  }
  .flow-canvas :global(.svelte-flow__pane.selection.dragging),
  .flow-canvas :global(.svelte-flow__node.draggable.dragging),
  .flow-canvas :global(.svelte-flow__nodesselection-rect:active) {
    cursor: grabbing;
  }
  .flow-canvas :global(.svelte-flow__handle.connectionindicator) {
    cursor: pointer;
  }
</style>
