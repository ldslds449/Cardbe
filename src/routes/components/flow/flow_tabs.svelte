<script lang="ts">
  import { tick } from "svelte";
  import { DragDropProvider, PointerSensor } from "@dnd-kit-svelte/svelte";
  import { distance_activation_constraint } from "../dnd/activation";
  import FlowTabSortable from "./flow_tab_sortable.svelte";
  import { Button, buttonVariants } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu";
  import PlusIcon from "@lucide/svelte/icons/plus";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import Trash2Icon from "@lucide/svelte/icons/trash-2";
  import * as m from "$lib/paraglide/messages";
  import { dragged_tab_ids, type FlowTab } from "./flow";

  let {
    tabs,
    active_id,
    panel_id,
    read_only = false,
    busy = false,
    renaming_id = $bindable(null),
    onSelect,
    onCreate,
    onRename,
    onMove,
    onReorder,
    onDelete,
  }: {
    tabs: FlowTab[];
    active_id: string;
    panel_id: string;
    read_only?: boolean;
    busy?: boolean;
    renaming_id?: string | null;
    onSelect: (id: string) => void;
    onCreate: () => void;
    onRename: (id: string, name: string) => Promise<boolean>;
    onMove: (id: string, offset: number) => void;
    onReorder: (ids: string[]) => void;
    onDelete: (id: string) => void;
  } = $props();
  let menu_open = $state(false);
  let draft = $state("");
  let editor = $state<HTMLInputElement | null>(null);
  let committing = false;
  // Keyboard users reorder from the tab menu, so dragging uses pointers only.
  const sensors = [
    PointerSensor.configure({
      activationConstraints: () => [distance_activation_constraint(8)] as never,
    }),
  ];
  let drag_order = $state.raw<string[] | null>(null);
  const shown_tabs = $derived(
    drag_order
      ? drag_order.flatMap((id) => tabs.filter((tab) => tab.id === id))
      : tabs,
  );
  function drag_over(event: any) {
    const { source, target } = event.operation ?? {};
    if (source?.type !== "flow-tab" || target?.type !== "flow-tab") {
      return;
    }
    const next = dragged_tab_ids(
      drag_order ?? tabs.map((tab) => tab.id),
      String(source.id),
      String(target.id),
    );
    if (next) {
      drag_order = next;
    }
  }
  function drag_end(event: any) {
    const order = drag_order;
    drag_order = null;
    if (
      !event.canceled &&
      order &&
      order.join("\n") !== tabs.map((tab) => tab.id).join("\n")
    ) {
      onReorder(order);
    }
  }
  const label = (tab: FlowTab) => tab.name.trim() || m.flow_tab_default();
  const tab_element_id = (id: string) => `flow-tab-${id}`;

  $effect(() => {
    const id = renaming_id;
    if (id !== null) {
      draft = tabs.find((tab) => tab.id === id)?.name ?? "";
      void tick().then(() => {
        editor?.focus();
        editor?.select();
      });
    }
  });

  async function focus_tab(id: string) {
    await tick();
    document.getElementById(tab_element_id(id))?.focus();
  }
  function select(id: string) {
    onSelect(id);
    void focus_tab(id);
  }
  function start_rename(id: string) {
    if (!read_only && !busy) {
      renaming_id = id;
    }
  }
  async function commit_rename() {
    const id = renaming_id;
    if (id === null || committing) {
      return;
    }
    committing = true;
    try {
      if (await onRename(id, draft)) {
        renaming_id = null;
        void focus_tab(id);
      }
    } finally {
      committing = false;
    }
  }
  function cancel_rename() {
    const id = renaming_id;
    renaming_id = null;
    if (id !== null) {
      void focus_tab(id);
    }
  }
  function tab_keydown(event: KeyboardEvent, index: number) {
    // Alt+Arrow is the keyboard alternative to dragging tabs.
    if (
      event.altKey &&
      (event.key === "ArrowLeft" || event.key === "ArrowRight")
    ) {
      event.preventDefault();
      if (!read_only && !busy) {
        const id = tabs[index].id;
        onMove(id, event.key === "ArrowLeft" ? -1 : 1);
        void focus_tab(id);
      }
      return;
    }
    const target =
      event.key === "ArrowLeft"
        ? index - 1
        : event.key === "ArrowRight"
          ? index + 1
          : event.key === "Home"
            ? 0
            : event.key === "End"
              ? tabs.length - 1
              : null;
    if (target !== null) {
      event.preventDefault();
      select(tabs[(target + tabs.length) % tabs.length].id);
    }
  }
</script>

<div class="flex min-w-0 items-center gap-1">
  <DragDropProvider {sensors} onDragOver={drag_over} onDragEnd={drag_end}>
    <div
      class="flex min-w-0 items-center gap-0.5 overflow-x-auto rounded-lg bg-muted p-0.5 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
      role="tablist"
      aria-label={m.flow_tabs()}
    >
      {#each shown_tabs as tab, index (tab.id)}
        {@const active = tab.id === active_id}
        <FlowTabSortable
          id={tab.id}
          {index}
          disabled={read_only || busy || renaming_id !== null}
          class={`flex h-7 shrink-0 items-center rounded-md transition-colors has-[[role=tab]:focus-visible]:outline-2 has-[[role=tab]:focus-visible]:-outline-offset-2 has-[[role=tab]:focus-visible]:outline-ring ${active ? "bg-background text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground"}`}
        >
          {#snippet children(handle)}
            {#if renaming_id === tab.id}
              <Input
                bind:ref={editor}
                bind:value={draft}
                maxlength={60}
                aria-label={m.flow_tab_name()}
                placeholder={m.flow_tab_default()}
                class="mx-0.5 h-6 w-40 px-2 text-sm"
                disabled={busy}
                onblur={commit_rename}
                onkeydown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    void commit_rename();
                  } else if (event.key === "Escape") {
                    event.preventDefault();
                    cancel_rename();
                  }
                }}
              />
            {:else}
              <Button
                {@attach handle}
                id={tab_element_id(tab.id)}
                variant="ghost"
                role="tab"
                aria-selected={active}
                aria-controls={active ? panel_id : undefined}
                tabindex={active ? 0 : -1}
                aria-keyshortcuts={read_only
                  ? undefined
                  : "Alt+ArrowLeft Alt+ArrowRight"}
                title={label(tab)}
                class={`h-full max-w-48 rounded-md px-2.5 text-sm font-normal hover:bg-transparent focus-visible:border-transparent focus-visible:ring-0 ${active && !read_only ? "pr-0.5 font-medium" : active ? "font-medium" : ""}`}
                onclick={() => select(tab.id)}
                ondblclick={() => start_rename(tab.id)}
                oncontextmenu={(event) => {
                  event.preventDefault();
                  select(tab.id);
                  if (!read_only) {
                    menu_open = true;
                  }
                }}
                onkeydown={(event) => tab_keydown(event, index)}
                ><span class="truncate">{label(tab)}</span></Button
              >
              {#if active && !read_only}
                <DropdownMenu.Root bind:open={menu_open}>
                  <DropdownMenu.Trigger
                    class={`${buttonVariants({ size: "icon-sm", variant: "ghost" })} mr-0.5 size-6 text-muted-foreground hover:bg-muted focus-visible:border-transparent focus-visible:bg-muted focus-visible:ring-0`}
                    aria-label={m.flow_tab_options({ name: label(tab) })}
                    title={m.flow_tab_options({ name: label(tab) })}
                    disabled={busy}><ChevronDownIcon /></DropdownMenu.Trigger
                  >
                  <DropdownMenu.Content
                    align="start"
                    onCloseAutoFocus={(event) => {
                      // Keep focus in the rename field opened from this menu.
                      if (renaming_id !== null) {
                        event.preventDefault();
                      }
                    }}
                  >
                    <DropdownMenu.Item onclick={() => start_rename(tab.id)}
                      ><PencilIcon />{m.flow_tab_rename()}</DropdownMenu.Item
                    >
                    <DropdownMenu.Separator />
                    <DropdownMenu.Item
                      variant="destructive"
                      disabled={tabs.length <= 1}
                      onclick={() => onDelete(tab.id)}
                      ><Trash2Icon />{m.flow_tab_delete()}</DropdownMenu.Item
                    >
                  </DropdownMenu.Content>
                </DropdownMenu.Root>
              {/if}
            {/if}
          {/snippet}
        </FlowTabSortable>
      {/each}
    </div>
  </DragDropProvider>
  {#if !read_only}
    <Button
      size="icon-sm"
      variant="ghost"
      class="shrink-0 text-muted-foreground"
      aria-label={m.flow_tab_add()}
      title={m.flow_tab_add()}
      disabled={busy}
      onclick={onCreate}><PlusIcon /></Button
    >
  {/if}
</div>
