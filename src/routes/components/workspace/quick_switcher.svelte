<script lang="ts">
  import { formatNumber } from "$lib/i18n";
  import { m } from "$lib/paraglide/messages.js";
  import "$lib/i18n/locale.svelte";
  import CheckIcon from "@lucide/svelte/icons/check";
  import HouseIcon from "@lucide/svelte/icons/house";
  import LayoutDashboardIcon from "@lucide/svelte/icons/layout-dashboard";
  import ListFilterIcon from "@lucide/svelte/icons/list-filter";
  import StickyNoteIcon from "@lucide/svelte/icons/sticky-note";
  import UsersIcon from "@lucide/svelte/icons/users";
  import * as Command from "$lib/components/ui/command/index.js";
  import type { Focus } from "../../type/focus";
  import type { BoardItem } from "./board_sidebar.svelte";
  import { HOME_PAGE_ID, NOTES_PAGE_ID } from "./workspace";

  let {
    open = $bindable(),
    boards,
    active_board_id,
    focuses,
    active_focus_id,
    onOpenHome,
    onOpenNotes,
    onOpenFocus,
    onSwitch,
  }: {
    open: boolean;
    boards: BoardItem[];
    active_board_id: number | null;
    focuses: Focus[];
    active_focus_id: string | null;
    onOpenHome: () => void;
    onOpenNotes: () => void;
    onOpenFocus: (id: string) => void;
    onSwitch: (id: number) => void;
  } = $props();

  let search = $state("");

  $effect(() => {
    if (open) {
      search = "";
    }
  });

  // Item values only identify rows; names are matched as keywords, the same
  // substring match the sidebar used, so "board" does not match every board.
  function filter(_value: string, query: string, keywords?: string[]) {
    const needle = query.trim().toLocaleLowerCase();
    return (keywords ?? []).some((keyword) =>
      keyword.toLocaleLowerCase().includes(needle),
    )
      ? 1
      : 0;
  }

  function choose(action: () => void) {
    open = false;
    action();
  }
</script>

<Command.Dialog
  bind:open
  title={m.quick_switch_title()}
  description={m.quick_switch_description()}
  {filter}
>
  <Command.Input
    bind:value={search}
    placeholder={m.board_drawer_search()}
    aria-label={m.board_drawer_search()}
  />
  <Command.List class="max-h-[min(400px,60dvh)]">
    <Command.Empty>{m.quick_switch_empty()}</Command.Empty>
    <Command.Group>
      <Command.Item
        value="home"
        keywords={[m.home_title()]}
        onSelect={() => choose(onOpenHome)}
      >
        <HouseIcon class="text-primary" />
        <span class="min-w-0 flex-1 truncate">{m.home_title()}</span>
        {#if active_focus_id === HOME_PAGE_ID}
          <CheckIcon aria-label={m.quick_switch_current()} />
        {/if}
      </Command.Item>
      <Command.Item
        value="notes"
        keywords={[m.note_title()]}
        onSelect={() => choose(onOpenNotes)}
      >
        <StickyNoteIcon class="text-primary" />
        <span class="min-w-0 flex-1 truncate">{m.note_title()}</span>
        {#if active_focus_id === NOTES_PAGE_ID}
          <CheckIcon aria-label={m.quick_switch_current()} />
        {/if}
      </Command.Item>
    </Command.Group>
    {#if focuses.length > 0}
      <Command.Group heading={m.focus_section()}>
        {#each focuses as view (view.id)}
          <Command.Item
            value={`focus:${view.id}`}
            keywords={[view.name]}
            onSelect={() => choose(() => onOpenFocus(view.id))}
          >
            <ListFilterIcon class="text-primary" />
            <span class="min-w-0 flex-1 truncate">{view.name}</span>
            {#if view.id === active_focus_id}
              <CheckIcon aria-label={m.quick_switch_current()} />
            {/if}
          </Command.Item>
        {/each}
      </Command.Group>
    {/if}
    {#if boards.length > 0}
      <Command.Group heading={m.board_list()}>
        {#each boards as item (item.id)}
          <Command.Item
            value={`board:${item.id}`}
            keywords={[item.name]}
            onSelect={() => choose(() => onSwitch(item.id))}
          >
            {#if item.shared_role === "owner"}
              <LayoutDashboardIcon class="text-primary" />
            {:else}
              <UsersIcon class="text-primary" />
            {/if}
            <span class="min-w-0 flex-1 truncate">{item.name}</span>
            {#if item.id === active_board_id && !active_focus_id}
              <CheckIcon aria-label={m.quick_switch_current()} />
            {/if}
            <Command.Shortcut
              class="tabular-nums"
              title={m.task_count({ count: item.task_count })}
              >{formatNumber(item.task_count)}</Command.Shortcut
            >
          </Command.Item>
        {/each}
      </Command.Group>
    {/if}
  </Command.List>
</Command.Dialog>
