<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import * as DropdownMenu from "#lib/components/ui/dropdown-menu/index.js";
  import { Badge } from "#lib/components/ui/badge/index.js";
  import { Button } from "#lib/components/ui/button/index.js";

  import type { BoardSummary } from "../../board.svelte";
  import { board_scope_name } from "./task-filters.svelte";

  let {
    id,
    boards,
    board_ids,
    onChange,
  }: {
    /** Prefix for the label IDs; the caller labels the menu with `${id}-label`. */
    id: string;
    boards: BoardSummary[];
    /** Null selects every board, including boards created later. */
    board_ids: number[] | null;
    onChange: (board_ids: number[] | null) => void;
  } = $props();

  function select_board(board_id: number, checked: boolean) {
    const selected = (board_ids ?? boards.map((board) => board.id)).filter(
      (item) => item !== board_id,
    );
    onChange(
      checked ? [...selected, board_id].sort((a, b) => a - b) : selected,
    );
  }
</script>

<DropdownMenu.Root>
  <DropdownMenu.Trigger>
    {#snippet child({ props })}
      <Button
        {...props}
        variant="outline"
        class="h-9 w-full min-w-0 justify-between font-normal"
        aria-labelledby={`${id}-label ${id}-selection`}
      >
        <span id={`${id}-selection`} class="truncate"
          >{board_scope_name(board_ids, boards)}</span
        >
        <ChevronDownIcon class="size-4" />
      </Button>
    {/snippet}
  </DropdownMenu.Trigger>
  <DropdownMenu.Content class="max-h-80" align="start">
    <DropdownMenu.CheckboxItem
      checked={board_ids === null ||
        (boards.length > 0 && board_ids.length === boards.length)}
      indeterminate={board_ids !== null &&
        board_ids.length > 0 &&
        board_ids.length < boards.length}
      closeOnSelect={false}
      onCheckedChange={(checked) => onChange(checked ? null : [])}
      >{m.explorer_all_boards()}</DropdownMenu.CheckboxItem
    >
    <DropdownMenu.Separator />
    {#each boards as board (board.id)}
      <DropdownMenu.CheckboxItem
        checked={board_ids === null || board_ids.includes(board.id)}
        closeOnSelect={false}
        onCheckedChange={(checked) => select_board(board.id, checked)}
        >{board.name}
        {#if board.is_shared || board.shared_role !== "owner"}
          <Badge variant="outline" class="ml-auto text-[11px]">
            {board.shared_role === "viewer"
              ? m.board_read_only()
              : board.shared_role === "editor"
                ? m.board_can_edit()
                : m.explorer_shared()}
          </Badge>
        {/if}</DropdownMenu.CheckboxItem
      >
    {/each}
  </DropdownMenu.Content>
</DropdownMenu.Root>
