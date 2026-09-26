<script lang="ts">
  import { toggleMode } from "mode-watcher";

  import ArchiveIcon from "@lucide/svelte/icons/archive";
  import BellIcon from "@lucide/svelte/icons/bell";
  import BellOffIcon from "@lucide/svelte/icons/bell-off";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import ClockIcon from "@lucide/svelte/icons/clock";
  import EyeIcon from "@lucide/svelte/icons/eye";
  import EyeClosedIcon from "@lucide/svelte/icons/eye-closed";
  import LayoutDashboardIcon from "@lucide/svelte/icons/layout-dashboard";
  import LayoutTemplateIcon from "@lucide/svelte/icons/layout-template";
  import LinkIcon from "@lucide/svelte/icons/link";
  import MoonIcon from "@lucide/svelte/icons/moon";
  import StickyNoteIcon from "@lucide/svelte/icons/sticky-note";
  import Repeat2Icon from "@lucide/svelte/icons/repeat-2";
  import RadioTowerIcon from "@lucide/svelte/icons/radio-tower";
  import ShieldCheckIcon from "@lucide/svelte/icons/shield-check";
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
    board_panel_open = $bindable(),
    search_text = $bindable(),
    archive_open = $bindable(),
    expired_open = $bindable(),
    recurring_open = $bindable(),
    note_open = $bindable(),
    task_expand_mode = $bindable(),
    selected_view,
    update_check_in_progress,
    read_only = false,
    web_publish_count = 0,
    web_publish_status = "idle",
    web_publish_detail = "",
    pending_device_count = 0,
    removed_access_count = 0,
    onOpenDeviceRequests,
    onOpenRemovedAccess,
    onPrepareImport,
    onExportAllBoards,
    onImportAllBoards,
    onPrepareTaskImport,
    onOpenBoardShare,
    onOpenIrohShare,
    onOpenTaskTemplates,
    onAddTask,
    onAddColumn,
    onCheckForUpdates,
  }: {
    board_panel_open: boolean;
    search_text: string;
    archive_open: boolean;
    expired_open: boolean;
    recurring_open: boolean;
    note_open: boolean;
    task_expand_mode: boolean;
    selected_view: WorkspaceView;
    update_check_in_progress: boolean;
    read_only?: boolean;
    web_publish_count?: number;
    web_publish_status?: "idle" | "updating" | "error";
    web_publish_detail?: string;
    pending_device_count?: number;
    removed_access_count?: number;
    onOpenDeviceRequests: () => void;
    onOpenRemovedAccess: () => void;
    onPrepareImport: () => void | Promise<void>;
    onExportAllBoards: () => void | Promise<void>;
    onImportAllBoards: () => void | Promise<void>;
    onPrepareTaskImport: () => void | Promise<void>;
    onOpenBoardShare: () => void;
    onOpenIrohShare: () => void;
    onOpenTaskTemplates: () => void;
    onAddTask: () => void;
    onAddColumn: () => void;
    onCheckForUpdates: (manual?: boolean) => void | Promise<void>;
  } = $props();

</script>

<Menubar.Root class="h-12 shrink-0 rounded-none border-x-0 border-t-0 px-4">
  <div class="flex h-full w-full flex-row items-center gap-2">
    <Button
      variant={board_panel_open ? "secondary" : "ghost"}
      size="sm"
      class="h-8 shrink-0 gap-1.5 px-2.5 text-sm font-semibold"
      onclick={() => (board_panel_open = !board_panel_open)}
      aria-label={board_panel_open ? "Close boards" : "Open boards"}
      aria-expanded={board_panel_open}
      aria-controls="board-drawer"
      title={board_panel_open ? "Close boards" : "Open boards"}
    >
      <LayoutDashboardIcon class="size-4" />
      <span>Cardbe</span>
      <ChevronDownIcon
        class={cn("size-3.5 transition-transform", board_panel_open && "rotate-180")}
      />
    </Button>

    <div class="flex min-w-0 flex-1 items-center pl-1 pr-2">
      <div class="flex flex-row items-center gap-1">
        <Menubar.Menu>
          <Menubar.Trigger>File</Menubar.Trigger>
          <Menubar.Content>
            <Menubar.Item onclick={() => void onPrepareTaskImport()}>Import Task</Menubar.Item>
            <Menubar.Item onclick={() => void board.export_to_file()}>Export This Board…</Menubar.Item>
            <Menubar.Item onclick={() => void onPrepareImport()}>Import Board as New…</Menubar.Item>
            <Menubar.Separator />
            <Menubar.Item onclick={() => void onExportAllBoards()}>Back Up Everything…</Menubar.Item>
            <Menubar.Item onclick={() => void onImportAllBoards()}>Restore Everything…</Menubar.Item>
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
            {#if !read_only}<Menubar.Item onclick={onAddTask}>
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
            <Menubar.Separator />{/if}
            <Menubar.Item onclick={onOpenIrohShare}>Board sharing...</Menubar.Item>
            <Menubar.Item onclick={onOpenBoardShare}>Web publish...</Menubar.Item>
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

    {#if pending_device_count > 0}
      <Button variant="outline" size="sm" class="shrink-0 gap-1.5 border-primary/40 bg-primary/5 text-primary" onclick={onOpenDeviceRequests} aria-label={`${pending_device_count} device approval requests`} title="Review device approval requests"><ShieldCheckIcon class="size-4" /><span class="hidden xl:inline">Device requests</span><span class="rounded-full bg-primary px-1.5 text-xs text-primary-foreground">{pending_device_count}</span></Button>
    {/if}
    {#if removed_access_count > 0}
      <Button variant="outline" size="sm" class="shrink-0 gap-1.5 border-destructive/40 bg-destructive/5 text-destructive" onclick={onOpenRemovedAccess} aria-label={`${removed_access_count} shared boards need access`} title="Request access to shared boards again"><LinkIcon class="size-4" /><span class="hidden xl:inline">Access removed</span><span class="rounded-full bg-destructive px-1.5 text-xs text-white">{removed_access_count}</span></Button>
    {/if}
    {#if web_publish_count > 0}
      <Button
        variant="outline"
        size="sm"
        class={cn(
          "shrink-0 gap-1.5",
          web_publish_status === "error" && "border-destructive/50 text-destructive hover:bg-destructive/10 hover:text-destructive",
        )}
        onclick={onOpenBoardShare}
        aria-label={`Open Web publish. ${web_publish_detail}`}
        title={web_publish_detail}
      >
        <span class="relative">
          <RadioTowerIcon class="size-4" />
          <span class={cn(
            "absolute -right-1 -top-1 size-2 rounded-full border border-background",
            web_publish_status === "error" ? "bg-destructive" : web_publish_status === "updating" ? "animate-pulse bg-amber-500" : "bg-emerald-500",
          )}></span>
        </span>
        <span class="hidden xl:inline">Web publish</span>
        <span class="text-xs tabular-nums text-muted-foreground">{web_publish_count}</span>
      </Button>
    {/if}

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
