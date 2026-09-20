<script lang="ts">
  import CalendarDaysIcon from "@lucide/svelte/icons/calendar-days";
  import Columns3Icon from "@lucide/svelte/icons/columns-3";
  import SparklesIcon from "@lucide/svelte/icons/sparkles";
  import { Button } from "$lib/components/ui/button/index.js";
  import type { WorkspaceView } from "./workspace";

  let { selected_view, active_board_name, onSwitchView }: {
    selected_view: WorkspaceView; active_board_name: string;
    onSwitchView: (view: WorkspaceView) => void;
  } = $props();
  const views: { id: WorkspaceView; label: string; icon: typeof SparklesIcon }[] = [
    { id: "focus", label: "Today", icon: SparklesIcon },
    { id: "board", label: "Board", icon: Columns3Icon },
    { id: "calendar", label: "Calendar", icon: CalendarDaysIcon },
  ];
</script>

<div class="relative flex h-10 shrink-0 items-end border-b px-4" aria-label="Workspace navigation">
  <div class="flex h-full items-end" aria-label="Workspace views" role="tablist">
    {#each views as view (view.id)}
      {@const Icon = view.icon}
      <Button variant="ghost" class={`h-10 rounded-none border-b-2 px-4 text-sm transition-colors ${selected_view === view.id ? "border-b-primary text-foreground" : "border-b-transparent text-muted-foreground hover:text-foreground"}`} role="tab" aria-selected={selected_view === view.id} onclick={() => onSwitchView(view.id)}>
        <Icon /> {view.label}
      </Button>
    {/each}
  </div>
  <!-- Out of flow so a changing board name never moves the view tabs. -->
  <div
    class="pointer-events-none absolute right-4 top-1/2 flex max-w-44 -translate-y-1/2 items-center gap-1.5 text-sm sm:max-w-64"
    aria-label={`Current board: ${active_board_name}`}
  >
    <span class="hidden shrink-0 text-xs text-muted-foreground sm:inline">Board</span>
    <span class="hidden shrink-0 text-xs text-muted-foreground sm:inline" aria-hidden="true">·</span>
    <span class="truncate font-medium text-foreground" title={active_board_name}>{active_board_name}</span>
  </div>
</div>
