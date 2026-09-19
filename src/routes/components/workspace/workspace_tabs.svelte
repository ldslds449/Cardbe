<script lang="ts">
  import CalendarDaysIcon from "@lucide/svelte/icons/calendar-days";
  import Columns3Icon from "@lucide/svelte/icons/columns-3";
  import SparklesIcon from "@lucide/svelte/icons/sparkles";

  import { Button } from "$lib/components/ui/button/index.js";

  import type { WorkspaceView } from "./workspace";

  let {
    selected_view,
    onSwitchView,
  }: {
    selected_view: WorkspaceView;
    onSwitchView: (view: WorkspaceView) => void;
  } = $props();

  const views: {
    id: WorkspaceView;
    label: string;
    icon: typeof SparklesIcon;
  }[] = [
    { id: "focus", label: "Today", icon: SparklesIcon },
    { id: "board", label: "Board", icon: Columns3Icon },
    { id: "calendar", label: "Calendar", icon: CalendarDaysIcon },
  ];
</script>

<div
  class="flex h-10 shrink-0 items-end gap-0 border-b px-4"
  aria-label="Workspace views"
  role="tablist"
>
  {#each views as view (view.id)}
    {@const Icon = view.icon}
    <Button
      variant="ghost"
      class={`h-10 rounded-none border-b-2 px-4 text-sm transition-colors ${
        selected_view === view.id
          ? "border-b-primary text-foreground"
          : "border-b-transparent text-muted-foreground hover:text-foreground"
      }`}
      role="tab"
      aria-selected={selected_view === view.id}
      onclick={() => onSwitchView(view.id)}
    >
      <Icon />
      {view.label}
    </Button>
  {/each}
</div>
