<script lang="ts">
  import CalendarDaysIcon from "@lucide/svelte/icons/calendar-days";
  import Columns3Icon from "@lucide/svelte/icons/columns-3";
  import RefreshCwIcon from "@lucide/svelte/icons/refresh-cw";
  import SparklesIcon from "@lucide/svelte/icons/sparkles";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import type { WorkspaceView } from "./workspace";
  import type { BoardRole } from "../../board.svelte";

  let {
    selected_view,
    active_board_name,
    shared_role = "owner",
    sync_status = "local",
    last_synced_at,
    onSync,
    onSwitchView,
  }: {
    selected_view: WorkspaceView;
    active_board_name: string;
    shared_role?: BoardRole;
    sync_status?: string;
    last_synced_at?: number;
    onSync?: () => void;
    onSwitchView: (view: WorkspaceView) => void;
  } = $props();

  const views: { id: WorkspaceView; label: string; icon: typeof SparklesIcon }[] = [
    { id: "focus", label: "Today", icon: SparklesIcon },
    { id: "board", label: "Board", icon: Columns3Icon },
    { id: "calendar", label: "Calendar", icon: CalendarDaysIcon },
  ];
  const time_formatter = new Intl.DateTimeFormat("en-US", { hour: "numeric", minute: "2-digit" });
</script>

<div class="flex h-10 shrink-0 items-end border-b px-4" aria-label="Workspace navigation">
  <div class="flex h-full shrink-0 items-end" aria-label="Workspace views" role="tablist">
    {#each views as view (view.id)}
      {@const Icon = view.icon}
      <Button variant="ghost" class={`h-10 rounded-none border-b-2 px-4 text-sm transition-colors ${selected_view === view.id ? "border-b-primary text-foreground" : "border-b-transparent text-muted-foreground hover:text-foreground"}`} role="tab" aria-selected={selected_view === view.id} onclick={() => onSwitchView(view.id)}>
        <Icon /> {view.label}
      </Button>
    {/each}
  </div>

  <div class="ml-auto flex min-w-0 max-w-[55%] self-center items-center gap-2 text-sm" aria-label={`Current board: ${active_board_name}`}>
    <span class="truncate font-medium text-foreground" title={active_board_name}>{active_board_name}</span>
    {#if shared_role !== "owner"}
      <Badge variant="secondary" class="hidden shrink-0 sm:inline-flex">{shared_role === "viewer" ? "Shared - read only" : "Shared - can edit"}</Badge>
      {#if sync_status === "conflict"}<Badge variant="destructive" class="shrink-0">Conflict</Badge>{:else}<span class="hidden shrink-0 text-xs text-muted-foreground lg:inline">
        {sync_status === "syncing" ? "Syncing..." : sync_status === "pending" ? "Pending" : sync_status === "error" ? "Sync error" : last_synced_at ? `Synced ${time_formatter.format(new Date(last_synced_at))}` : "Background sync"}
      </span>{/if}
      {#if onSync}
        <Button size="icon-sm" variant="ghost" disabled={sync_status === "syncing" || sync_status === "conflict"} onclick={onSync} aria-label="Sync shared board now" title={sync_status === "conflict" ? "Resolve conflict in Board sharing" : "Sync now"}><RefreshCwIcon class={sync_status === "syncing" ? "animate-spin" : ""} /></Button>
      {/if}
    {/if}
  </div>
</div>
