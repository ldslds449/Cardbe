<script lang="ts">
    import * as Sheet from "$lib/components/ui/sheet/index.js";
    import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
    import { Separator } from "$lib/components/ui/separator/index.js";
    import { Spinner } from "$lib/components/ui/spinner/index.js";
    import * as InputGroup from "$lib/components/ui/input-group/index.js";
    import ArchiveIcon from "@lucide/svelte/icons/archive";
    import SearchIcon from "@lucide/svelte/icons/search";
    import XIcon from "@lucide/svelte/icons/x";

    import ArchiveItem from "./archive_item.svelte";
    import UnarchiveTaskDialog from "./unarchive_task_dialog.svelte";
    import ViewTaskDialog from "../dialog/view_task_dialog.svelte";
    import { task_matches_search } from "../calendar/calendar";

    import type { Task } from "../../type/task.svelte";
    import type { Column } from "../../type/column.svelte";
    import type { Archive } from "../../type/archive.svelte";

    let {
        open = $bindable(false),
        archives,
        loading = false,
        columns,
        onUnarchive,
    }: {
        open: boolean;
        archives: Archive[];
        loading?: boolean;
        columns: Column[];
        onUnarchive: (column_id: string, task_id: string) => void;
    } = $props();

    let search_text = $state("");

    // 1. sort by date
    // 2. reduce into date unit
    let archive_reduced = $derived(
        archives
            .filter((archive) => task_matches_search(archive.task, search_text))
            .toSorted((a, b) => b.time.getTime() - a.time.getTime())
            .reduce(
                (acc, el) => {
                    const date_str = [
                        el.time.getFullYear(),
                        String(el.time.getMonth() + 1).padStart(2, "0"),
                        String(el.time.getDate()).padStart(2, "0"),
                    ].join("-");
                    const current = acc.get(date_str) || {
                        tasks: [],
                    };

                    acc.set(date_str, {
                        tasks: [...current.tasks, el.task],
                    });
                    return acc;
                },
                new Map<
                    string,
                    {
                        tasks: Task[];
                    }
                >(),
            ),
    );

    let unarchive_dialog_open = $state(false);
    let unarchive_target = $state<Task | null>(null);
    let view_task_dialog_open = $state(false);
    let view_task = $state<Task | null>(null);
    function unarchive_callback(task_id: string) {
        unarchive_target = archives.find((archive) => archive.task.id === task_id)?.task ?? null;
        unarchive_dialog_open = true;
    }
    function show_detail_callback(task: Task) {
        view_task = task;
        view_task_dialog_open = true;
    }
</script>

<div>
    <Sheet.Root bind:open>
        <Sheet.Content
            class="h-full"
            resizable
            defaultWidth={384}
            minWidth={320}
            maxWidth={960}
            widthStorageKey="cardbe.archive-panel-width"
        >
            <Sheet.Header>
                <Sheet.Title class="flex items-center gap-2">
                    <ArchiveIcon class="size-5 text-muted-foreground" />
                    Archives
                </Sheet.Title>
                <Sheet.Description>Review all archived tasks</Sheet.Description>
            </Sheet.Header>
            <div class="px-4">
                <InputGroup.Root>
                    <InputGroup.Input
                        placeholder="Search archived tasks"
                        aria-label="Search archived tasks"
                        bind:value={search_text}
                    />
                    <InputGroup.Addon>
                        <SearchIcon />
                    </InputGroup.Addon>
                    {#if search_text.length > 0}
                        <InputGroup.Addon align="inline-end">
                            <InputGroup.Button
                                aria-label="Clear archive search"
                                title="Clear search"
                                size="icon-xs"
                                onclick={() => {
                                    search_text = "";
                                }}
                            >
                                <XIcon />
                            </InputGroup.Button>
                        </InputGroup.Addon>
                    {/if}
                </InputGroup.Root>
            </div>
            <div class="min-h-0">
                <ScrollArea class="h-full" orientation="vertical">
                    {#if loading}
                        <div class="flex items-center justify-center gap-2 p-6 text-sm text-muted-foreground">
                            <Spinner />
                            Loading archives...
                        </div>
                    {/if}
                    {#each archive_reduced as [date, reduced_pair] (date)}
                        <Separator></Separator>
                        <ArchiveItem
                            {date}
                            tasks={reduced_pair.tasks}
                            onUnarchive={unarchive_callback}
                            onShowDetail={show_detail_callback}
                        ></ArchiveItem>
                    {/each}
                    {#if search_text.trim() && archive_reduced.size === 0}
                        <p class="p-6 text-center text-sm text-muted-foreground">
                            No matching archived tasks
                        </p>
                    {/if}
                </ScrollArea>
            </div>
        </Sheet.Content>
    </Sheet.Root>

    <UnarchiveTaskDialog
        bind:open={unarchive_dialog_open}
        task={unarchive_target}
        {columns}
        onConfirm={onUnarchive}
    />

    <ViewTaskDialog bind:open={view_task_dialog_open} task={view_task}></ViewTaskDialog>
</div>
