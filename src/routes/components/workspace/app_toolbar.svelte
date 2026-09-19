<script lang="ts">
  import { toggleMode } from "mode-watcher";

  import ArchiveIcon from "@lucide/svelte/icons/archive";
  import BellIcon from "@lucide/svelte/icons/bell";
  import BellOffIcon from "@lucide/svelte/icons/bell-off";
  import ClockIcon from "@lucide/svelte/icons/clock";
  import EyeIcon from "@lucide/svelte/icons/eye";
  import EyeClosedIcon from "@lucide/svelte/icons/eye-closed";
  import LayoutTemplateIcon from "@lucide/svelte/icons/layout-template";
  import MoonIcon from "@lucide/svelte/icons/moon";
  import StickyNoteIcon from "@lucide/svelte/icons/sticky-note";
  import Repeat2Icon from "@lucide/svelte/icons/repeat-2";
  import SearchIcon from "@lucide/svelte/icons/search";
  import SunIcon from "@lucide/svelte/icons/sun";
  import XIcon from "@lucide/svelte/icons/x";

  import { Button } from "$lib/components/ui/button/index.js";
  import * as InputGroup from "$lib/components/ui/input-group/index.js";
  import * as Kbd from "$lib/components/ui/kbd/index.js";
  import * as Menubar from "$lib/components/ui/menubar/index.js";
  import { cn } from "$lib/utils";

  import { board } from "../../board.svelte";
  import type { WorkspaceView } from "./workspace";

  let {
    app_name,
    search_text = $bindable(),
    archive_open = $bindable(),
    expired_open = $bindable(),
    recurring_open = $bindable(),
    note_open = $bindable(),
    task_expand_mode = $bindable(),
    selected_view,
    update_check_in_progress,
    onPrepareImport,
    onPrepareTaskImport,
    onOpenBoardShare,
    onOpenTaskTemplates,
    onAddTask,
    onAddColumn,
    onCheckForUpdates,
  }: {
    app_name: string;
    search_text: string;
    archive_open: boolean;
    expired_open: boolean;
    recurring_open: boolean;
    note_open: boolean;
    task_expand_mode: boolean;
    selected_view: WorkspaceView;
    update_check_in_progress: boolean;
    onPrepareImport: () => void | Promise<void>;
    onPrepareTaskImport: () => void | Promise<void>;
    onOpenBoardShare: () => void;
    onOpenTaskTemplates: () => void;
    onAddTask: () => void;
    onAddColumn: () => void;
    onCheckForUpdates: (manual?: boolean) => void | Promise<void>;
  } = $props();

</script>

<Menubar.Root class="h-12 shrink-0 rounded-none border-x-0 border-t-0 px-4">
  <div class="flex h-full w-full flex-row items-center gap-2">
    <h3 class="flex-none text-2xl font-semibold leading-none tracking-tight">{app_name}</h3>

    <div class="flex min-w-0 flex-1 items-center px-2">
      <div class="flex flex-row items-center gap-1">
        <Menubar.Menu>
          <Menubar.Trigger>File</Menubar.Trigger>
          <Menubar.Content>
            <Menubar.Item onclick={() => void onPrepareTaskImport()}>Import Task</Menubar.Item>
            <Menubar.Item onclick={() => void onPrepareImport()}>Import Data</Menubar.Item>
            <Menubar.Item onclick={() => void board.export_to_file()}>Export Data</Menubar.Item>
          </Menubar.Content>
        </Menubar.Menu>
        <Menubar.Menu>
          <Menubar.Trigger>Edit</Menubar.Trigger>
          <Menubar.Content>
            <Menubar.Item
              disabled={!board.can_undo || board.undo_in_progress}
              onclick={() => void board.undo()}
            >
              Undo
              <Menubar.Shortcut>
                <Kbd.Group>
                  <Kbd.Root>Ctrl</Kbd.Root>
                  <Kbd.Root>Z</Kbd.Root>
                </Kbd.Group>
              </Menubar.Shortcut>
            </Menubar.Item>
          </Menubar.Content>
        </Menubar.Menu>
        <Menubar.Menu>
          <Menubar.Trigger>Board</Menubar.Trigger>
          <Menubar.Content>
            <Menubar.Item onclick={onAddTask}>
              New Card
              <Menubar.Shortcut>
                <Kbd.Group>
                  <Kbd.Root>Ctrl</Kbd.Root>
                  <Kbd.Root>Shift</Kbd.Root>
                  <Kbd.Root>T</Kbd.Root>
                </Kbd.Group>
              </Menubar.Shortcut>
            </Menubar.Item>
            <Menubar.Item onclick={onAddColumn}>
              New Column
              <Menubar.Shortcut>
                <Kbd.Group>
                  <Kbd.Root>Ctrl</Kbd.Root>
                  <Kbd.Root>Shift</Kbd.Root>
                  <Kbd.Root>C</Kbd.Root>
                </Kbd.Group>
              </Menubar.Shortcut>
            </Menubar.Item>
            <Menubar.Separator />
            <Menubar.Item onclick={onOpenBoardShare}>Share Board...</Menubar.Item>
          </Menubar.Content>
        </Menubar.Menu>
        <Menubar.Menu>
          <Menubar.Trigger>Help</Menubar.Trigger>
          <Menubar.Content>
            <Menubar.Item
              disabled={update_check_in_progress}
              onclick={() => void onCheckForUpdates(true)}
            >
              Check for Updates
            </Menubar.Item>
          </Menubar.Content>
        </Menubar.Menu>
      </div>
    </div>

    <InputGroup.Root class="ml-auto w-40 shrink-0 sm:w-48 lg:w-64">
      <InputGroup.Input aria-label="Search tasks" placeholder="Search" bind:value={search_text} />
      <InputGroup.Addon>
        <SearchIcon />
      </InputGroup.Addon>
      {#if search_text.length > 0}
        <InputGroup.Addon align="inline-end">
          <InputGroup.Button
            aria-label="Clear search"
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

    <div class="flex shrink-0 items-center gap-1">
    <Button
      onclick={() => {
        note_open = !note_open;
      }}
      variant="outline"
      size="icon"
      class="self-center flex-none"
      aria-pressed={note_open}
      aria-label={note_open ? "Close notes" : "Open notes"}
      title={note_open ? "Close notes" : "Open notes"}
    >
      <StickyNoteIcon />
    </Button>

    <Button
      onclick={onOpenTaskTemplates}
      variant="outline"
      size="icon"
      class="self-center flex-none"
      aria-label="Open task templates"
      title="Open task templates"
    >
      <LayoutTemplateIcon />
    </Button>

    <Button
      onclick={() => {
        archive_open = !archive_open;
        if (archive_open) void board.ensure_archives_loaded();
      }}
      variant="outline"
      size="icon"
      class="self-center flex-none"
      aria-pressed={archive_open}
      aria-label={archive_open ? "Close archives" : "Open archives"}
      title={archive_open ? "Close archives" : "Open archives"}
    >
      <ArchiveIcon />
    </Button>

    <Button
      onclick={() => {
        board.get_expired_tasks();
        expired_open = !expired_open;
      }}
      variant="outline"
      size="icon"
      class="self-center flex-none"
      aria-pressed={expired_open}
      aria-label={expired_open ? "Close expired tasks" : "Open expired tasks"}
      title={expired_open ? "Close expired tasks" : "Open expired tasks"}
    >
      <ClockIcon />
    </Button>

    <Button
      onclick={() => {
        recurring_open = !recurring_open;
      }}
      variant="outline"
      size="icon"
      class="self-center flex-none"
      aria-pressed={recurring_open}
      aria-label={recurring_open ? "Close recurring tasks" : "Open recurring tasks"}
      title={recurring_open ? "Close recurring tasks" : "Open recurring tasks"}
    >
      <Repeat2Icon />
    </Button>

    <Button
      onclick={() => {
        void board.set_notify_enabled(!board.notify_enabled);
      }}
      variant="outline"
      size="icon"
      class="self-center flex-none"
      disabled={board.notification_setting_updating}
      aria-pressed={board.notify_enabled}
      aria-label={board.notify_enabled ? "Disable notifications" : "Enable notifications"}
      title={board.notify_enabled ? "Disable notifications" : "Enable notifications"}
    >
      <BellIcon
        class={cn(
          "h-[1.2rem] w-[1.2rem] !transition-all ",
          board.notify_enabled ? "rotate-0 scale-100" : "rotate-90 scale-0",
        )}
      />
      <BellOffIcon
        class={cn(
          "absolute h-[1.2rem] w-[1.2rem] !transition-all ",
          board.notify_enabled ? "rotate-90 scale-0" : "rotate-0 scale-100",
        )}
      />
    </Button>

    <div
      class="size-9 flex-none"
      title={selected_view !== "board" ? "Available in Board view" : undefined}
    >
      <Button
        onclick={() => {
          task_expand_mode = !task_expand_mode;
        }}
        variant="outline"
        size="icon"
        class="self-center"
        disabled={selected_view !== "board"}
        aria-pressed={task_expand_mode}
        aria-label={selected_view !== "board"
          ? "Task detail expansion is available in Board view"
          : task_expand_mode
            ? "Hide task details"
            : "Show task details"}
        title={selected_view !== "board"
          ? undefined
          : task_expand_mode
            ? "Hide task details"
            : "Show task details"}
      >
        <EyeIcon
          class={cn(
            "h-[1.2rem] w-[1.2rem] !transition-all ",
            task_expand_mode ? "rotate-0 scale-100" : "rotate-90 scale-0",
          )}
        />
        <EyeClosedIcon
          class={cn(
            "absolute h-[1.2rem] w-[1.2rem] !transition-all ",
            task_expand_mode ? "rotate-90 scale-0" : "rotate-0 scale-100",
          )}
        />
      </Button>
    </div>

    <Button
      onclick={toggleMode}
      variant="outline"
      size="icon"
      class="self-center flex-none"
      aria-label="Toggle color theme"
      title="Toggle color theme"
    >
      <SunIcon
        class="h-[1.2rem] w-[1.2rem] rotate-0 scale-100 !transition-all dark:-rotate-90 dark:scale-0"
      />
      <MoonIcon
        class="absolute h-[1.2rem] w-[1.2rem] rotate-90 scale-0 !transition-all dark:rotate-0 dark:scale-100"
      />
    </Button>
    </div>
  </div>
</Menubar.Root>
