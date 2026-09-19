<script lang="ts">
    import * as Card from "$lib/components/ui/card/index.js";
    import * as Collapsible from "$lib/components/ui/collapsible/index.js";
    import * as DropdownMenu from "$lib/components/ui/dropdown-menu/index.js";
    import { Button } from "$lib/components/ui/button/index.js";
    import { useSortable } from "@dnd-kit-svelte/svelte/sortable";

    import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
    import ChevronUpIcon from "@lucide/svelte/icons/chevron-up";
    import ClockIcon from "@lucide/svelte/icons/clock";
    import PencilIcon from "@lucide/svelte/icons/pencil";
    import CopyIcon from "@lucide/svelte/icons/copy";
    import LayoutTemplateIcon from "@lucide/svelte/icons/layout-template";
    import ArchiveIcon from "@lucide/svelte/icons/archive";
    import TrashIcon from "@lucide/svelte/icons/trash-2";
    import EllipsisIcon from "@lucide/svelte/icons/ellipsis";
    import FullscreenIcon from "@lucide/svelte/icons/fullscreen";
    import CircleAlertIcon from "@lucide/svelte/icons/circle-alert";
    import Repeat2Icon from "@lucide/svelte/icons/repeat-2";
    import Share2Icon from "@lucide/svelte/icons/share-2";

    import { recurrence_label, type Task } from "../../type/task.svelte";
    import { display_task_color } from "../../utils/task-color";
    import Markdown from "../markdown.svelte";
    import LabelBadge from "../label_badge.svelte";
    import ChecklistDisplay from "./checklist_display.svelte";
    import DeleteTaskDialog from "./delete_task_dialog.svelte";
    import { due_status } from "./due-status";

    interface CardItemProps {
        task: Task;
        index: number;
        group?: string | number;
        data?: { group: string | number };
        expand_content: boolean;
        read_only?: boolean;
        onViewTask: () => void;
        onEditTask: () => void;
        onDuplicateTask: () => void;
        onSaveAsTemplate?: () => void;
        onExportTask?: () => void;
        onDeleteTask: () => void;
        onArchiveTask: () => void;
    }

    let {
        task,
        index,
        group,
        data,
        expand_content,
        read_only = false,
        onViewTask = () => {},
        onEditTask = () => {},
        onDuplicateTask = () => {},
        onSaveAsTemplate,
        onExportTask = () => {},
        onDeleteTask = () => {},
        onArchiveTask = () => {},
    }: CardItemProps = $props();

    const { ref, isDragging, isDropTarget } = useSortable({
        id: () => task.id,
        index: () => index,
        type: "item",
        accept: "item",
        group: () => group,
        data: () => data,
        disabled: () => read_only,
    });

    let show_content = $state(false);
    let prevent_menu_focus_restore = false;
    let has_expandable_content = $derived(
        task.items.length > 0 || task.description.trim().length > 0,
    );
    let delete_confirm_open = $state(false);
    let due_metadata = $derived(
        (() => {
            if (!task.due_time) {
                return undefined;
            }

            const now = new Date();
            const include_year = task.due_time.getFullYear() !== now.getFullYear();
            const date = task.due_time.toLocaleDateString("en", {
                month: "short",
                day: "numeric",
                year: include_year ? "numeric" : undefined,
            });
            const has_time =
                task.due_time.getHours() !== 0 ||
                task.due_time.getMinutes() !== 0 ||
                task.due_time.getSeconds() !== 0;
            const time = has_time
                ? `, ${String(task.due_time.getHours()).padStart(2, "0")}:${String(task.due_time.getMinutes()).padStart(2, "0")}`
                : "";
            const status = due_status(task.due_time, now);
            return {
                text:
                    status === "today"
                        ? `Due today · ${date}${time}`
                        : status === "soon"
                          ? `Due soon · ${date}${time}`
                          : `${date}${time}`,
                status,
            };
        })(),
    );

    let previous_expand_content = $state<boolean | undefined>(undefined);
    $effect(() => {
        if (
            previous_expand_content !== undefined &&
            expand_content !== previous_expand_content
        ) {
            show_content = false;
        }
        previous_expand_content = expand_content;
    });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
    data-task-card
    class="group relative max-w-full select-none overflow-hidden"
    {@attach ref}
    oncontextmenu={(event) => {
        if (read_only) return;
        event.preventDefault();
        onEditTask();
    }}
>
    <Card.Root
        data-task-card-surface
        class="relative my-1 w-full gap-2 border-2 py-3 transition-[border-color,box-shadow] duration-150 hover:border-muted-foreground/40 hover:shadow-sm {read_only ? 'cursor-pointer' : ''}"
        role={read_only ? "button" : undefined}
        tabindex={read_only ? 0 : undefined}
        onclick={read_only ? onViewTask : undefined}
        onkeydown={read_only
            ? (event) => {
                  if (event.key === "Enter" || event.key === " ") {
                      event.preventDefault();
                      onViewTask();
                  }
              }
            : undefined}
        ondblclick={read_only ? undefined : onViewTask}
    >
                <Card.Header class="w-full px-4">
                    <Card.Title
                        class="break-all pr-16"
                        style="color: {display_task_color(task.color)};"
                    >
                        {task.title}
                    </Card.Title>
                    <Card.Description>
                        {#if due_metadata != undefined}
                            <span
                                class={`inline-flex items-center gap-1.5 text-xs ${
                                    due_metadata.status === "overdue"
                                        ? "text-destructive"
                                        : due_metadata.status === "today"
                                          ? "font-medium text-warning"
                                          : due_metadata.status === "soon"
                                            ? "font-medium text-warning"
                                            : "text-muted-foreground"
                                }`}
                            >
                                {#if due_metadata.status === "overdue"}
                                    <CircleAlertIcon class="size-3.5 shrink-0" />
                                {:else}
                                    <ClockIcon class="size-3.5 shrink-0" />
                                {/if}
                                <span>{due_metadata.text}</span>
                            </span>
                        {/if}
                        {#if task.recurrence}
                            <span class="ml-3 inline-flex items-center gap-1.5 text-xs text-muted-foreground">
                                <Repeat2Icon class="size-3.5 shrink-0" />
                                <span>{recurrence_label(task.recurrence)}</span>
                            </span>
                        {/if}
                    </Card.Description>
                </Card.Header>
                {#if !read_only}<div
                    class="absolute top-3 right-3 z-20 flex items-center gap-1 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100"
                    onpointerdown={(event) => event.stopPropagation()}
                    ondblclick={(event) => event.stopPropagation()}
                >
                    {#if has_expandable_content && !expand_content}
                        <Button
                            variant="ghost"
                            size="icon-sm"
                            class="size-6 text-muted-foreground hover:bg-muted/50 hover:text-foreground"
                            aria-label={show_content ? "Collapse task content" : "Expand task content"}
                            title={show_content ? "Collapse task content" : "Expand task content"}
                            onpointerup={(event) => event.currentTarget.blur()}
                            onclick={() => {
                                show_content = !show_content;
                            }}
                        >
                            {#if show_content}
                                <ChevronUpIcon class="size-4" />
                            {:else}
                                <ChevronDownIcon class="size-4" />
                            {/if}
                        </Button>
                    {/if}
                    <DropdownMenu.Root>
                        <DropdownMenu.Trigger>
                            {#snippet child({ props })}
                                <Button
                                    {...props}
                                    variant="ghost"
                                    size="icon-sm"
                                    class="size-6 text-muted-foreground hover:bg-muted/50 hover:text-foreground"
                                    aria-label="Task actions"
                                    title="Task actions"
                                >
                                    <EllipsisIcon class="size-4" />
                                </Button>
                            {/snippet}
                        </DropdownMenu.Trigger>
                        <DropdownMenu.Content
                            align="end"
                            class="min-w-36"
                            onCloseAutoFocus={(event) => {
                                if (prevent_menu_focus_restore) event.preventDefault();
                                prevent_menu_focus_restore = false;
                            }}
                        >
                            <DropdownMenu.Item onclick={onViewTask}>
                                <FullscreenIcon />
                                View Details
                            </DropdownMenu.Item>
                            <DropdownMenu.Item onclick={onEditTask}>
                                <PencilIcon />
                                Edit Task
                            </DropdownMenu.Item>
                            <DropdownMenu.Item onclick={onDuplicateTask}>
                                <CopyIcon />
                                Duplicate Task
                            </DropdownMenu.Item>
                            {#if onSaveAsTemplate}
                                <DropdownMenu.Item onclick={onSaveAsTemplate}>
                                    <LayoutTemplateIcon />
                                    Save as Template
                                </DropdownMenu.Item>
                            {/if}
                            <DropdownMenu.Item onclick={onExportTask}>
                                <Share2Icon />
                                Share Task
                            </DropdownMenu.Item>
                            <DropdownMenu.Item
                                onSelect={() => {
                                    // This trigger disappears with the archived card. Restoring
                                    // focus to it can make the scroll viewport jump to the top.
                                    prevent_menu_focus_restore = true;
                                    onArchiveTask();
                                }}
                            >
                                <ArchiveIcon />
                                Archive Task
                            </DropdownMenu.Item>
                            <DropdownMenu.Separator />
                            <DropdownMenu.Item
                                variant="destructive"
                                onclick={() => {
                                    delete_confirm_open = true;
                                }}
                            >
                                <TrashIcon />
                                Delete Task
                            </DropdownMenu.Item>
                        </DropdownMenu.Content>
                    </DropdownMenu.Root>
                </div>{/if}
                {#if task.items.length > 0 || (task.description.trim().length > 0 && (expand_content || show_content))}
                    <Card.Content
                        class="w-full break-all px-4 text-zinc-600 dark:text-zinc-400"
                    >
                        {#if task.items.length > 0}
                            <ChecklistDisplay
                                items={task.items}
                                show_items={expand_content || show_content}
                            />
                        {/if}
                        {#if task.description.trim().length > 0}
                            <Collapsible.Root open={expand_content || show_content}>
                                <Collapsible.Content>
                                    <div
                                        class={task.items.length > 0
                                            ? "mt-3 rounded-md bg-muted/40 px-3 py-2"
                                            : "rounded-md bg-muted/40 px-3 py-2"}
                                    >
                                        <Markdown md={task.description} compact />
                                    </div>
                                </Collapsible.Content>
                            </Collapsible.Root>
                        {/if}
                    </Card.Content>
                {/if}
                {#if task.labels.length > 0}
                    <Card.Footer class="px-4">
                        <div class="flex flex-wrap gap-1.5 w-full">
                            {#each task.labels as lbl (lbl)}
                                <LabelBadge label={lbl} />
                            {/each}
                        </div>
                    </Card.Footer>
                {/if}
    </Card.Root>

    {#if !read_only}
        <DeleteTaskDialog
            bind:open={delete_confirm_open}
            task_title={task.title}
            onConfirm={onDeleteTask}
        />
    {/if}
</div>
