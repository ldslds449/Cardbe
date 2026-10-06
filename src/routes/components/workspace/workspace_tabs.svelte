<script lang="ts">
  import { m } from "$lib/paraglide/messages.js";
  import "$lib/i18n/locale.svelte";
  import CalendarDaysIcon from "@lucide/svelte/icons/calendar-days";
  import ChartNoAxesCombinedIcon from "@lucide/svelte/icons/chart-no-axes-combined";
  import Columns3Icon from "@lucide/svelte/icons/columns-3";
  import RefreshCwIcon from "@lucide/svelte/icons/refresh-cw";
  import SparklesIcon from "@lucide/svelte/icons/sparkles";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import type { WorkspaceView } from "./workspace";
  import type { BoardRole } from "../../board.svelte";
  import { formatDateTime } from "$lib/i18n";

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

  const views = $derived<
    {
      id: WorkspaceView;
      label: string;
      icon: typeof SparklesIcon;
    }[]
  >([
    { id: "focus", label: m.workspace_today(), icon: SparklesIcon },
    { id: "board", label: m.board_title(), icon: Columns3Icon },
    { id: "calendar", label: m.workspace_calendar(), icon: CalendarDaysIcon },
    {
      id: "statistics",
      label: m.workspace_statistics(),
      icon: ChartNoAxesCombinedIcon,
    },
  ]);
</script>

<div
  class="flex h-10 shrink-0 items-end border-b px-4"
  aria-label={m.workspace_navigation()}
>
  <div
    class="flex h-full shrink-0 items-end"
    aria-label={m.workspace_views()}
    role="tablist"
  >
    {#each views as view (view.id)}
      {@const Icon = view.icon}
      <Button
        variant="ghost"
        class={`h-10 rounded-none border-b-2 px-4 text-sm transition-colors ${selected_view === view.id ? "border-b-primary text-foreground" : "border-b-transparent text-muted-foreground hover:text-foreground"}`}
        role="tab"
        aria-selected={selected_view === view.id}
        onclick={() => onSwitchView(view.id)}
      >
        <Icon />
        {view.label}
      </Button>
    {/each}
  </div>

  <div
    class="ml-auto flex min-w-0 max-w-[45%] self-center items-center gap-2 text-sm"
    aria-label={m.workspace_current_board({ name: active_board_name })}
  >
    <span class="truncate font-medium text-foreground" title={active_board_name}
      >{active_board_name}</span
    >
    {#if shared_role !== "owner" || onSync}
      <Badge variant="secondary" class="hidden shrink-0 sm:inline-flex"
        >{shared_role === "viewer"
          ? m.share_read_only()
          : m.share_can_edit()}</Badge
      >
      {#if sync_status === "conflict"}
        <Badge variant="destructive" class="shrink-0"
          >{m.share_conflict()}</Badge
        >
      {:else}
        <span class="hidden shrink-0 text-xs text-muted-foreground lg:inline">
          {sync_status === "syncing"
            ? m.share_syncing()
            : sync_status === "pending"
              ? m.share_pending()
              : sync_status === "error"
                ? m.share_sync_error()
                : last_synced_at
                  ? m.share_synced_at({
                      time: formatDateTime(last_synced_at, {
                        hour: "numeric",
                        minute: "2-digit",
                      }),
                    })
                  : m.share_background()}
        </span>
      {/if}
    {/if}
    {#if onSync}
      <Button
        size="icon-sm"
        variant="ghost"
        disabled={sync_status === "syncing" || sync_status === "conflict"}
        onclick={onSync}
        aria-label={m.share_sync_board()}
        title={sync_status === "conflict"
          ? m.share_resolve_conflict()
          : m.share_sync()}
        ><RefreshCwIcon
          class={sync_status === "syncing" ? "animate-spin" : ""}
        /></Button
      >
    {/if}
  </div>
</div>
