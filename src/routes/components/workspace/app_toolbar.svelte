<script lang="ts">
  import { formatNumber } from "$lib/i18n";
  import { m } from "$lib/paraglide/messages.js";
  import "$lib/i18n/locale.svelte";
  import { toggleMode } from "mode-watcher";
  import BellIcon from "@lucide/svelte/icons/bell";
  import BellOffIcon from "@lucide/svelte/icons/bell-off";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import EyeIcon from "@lucide/svelte/icons/eye";
  import EyeClosedIcon from "@lucide/svelte/icons/eye-closed";
  import LayoutDashboardIcon from "@lucide/svelte/icons/layout-dashboard";
  import LayoutTemplateIcon from "@lucide/svelte/icons/layout-template";
  import LoaderCircleIcon from "@lucide/svelte/icons/loader-circle";
  import LinkIcon from "@lucide/svelte/icons/link";
  import ListTodoIcon from "@lucide/svelte/icons/list-todo";
  import MoonIcon from "@lucide/svelte/icons/moon";
  import StickyNoteIcon from "@lucide/svelte/icons/sticky-note";
  import RadioTowerIcon from "@lucide/svelte/icons/radio-tower";
  import ShieldCheckIcon from "@lucide/svelte/icons/shield-check";
  import SearchIcon from "@lucide/svelte/icons/search";
  import XIcon from "@lucide/svelte/icons/x";
  import SunIcon from "@lucide/svelte/icons/sun";

  import { Button } from "$lib/components/ui/button/index.js";
  import * as InputGroup from "$lib/components/ui/input-group/index.js";
  import * as Kbd from "$lib/components/ui/kbd/index.js";
  import * as Menubar from "$lib/components/ui/menubar/index.js";
  import { cn } from "$lib/utils";
  import HelpDialogs from "./help_dialogs.svelte";
  import LanguageSettings from "./language_settings.svelte";

  import { board } from "../../board.svelte";
  import type { WorkspaceView } from "./workspace";

  let {
    board_panel_open = $bindable(),
    search_text = $bindable(),
    search_pending = false,
    search_error = false,
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
    onOpenAllTasks,
    onAddTask,
    onAddColumn,
    onCheckForUpdates,
  }: {
    board_panel_open: boolean;
    search_text: string;
    search_pending?: boolean;
    search_error?: boolean;
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
    onOpenAllTasks: () => void;
    onAddTask: () => void;
    onAddColumn: () => void;
    onCheckForUpdates: (manual?: boolean) => void | Promise<void>;
  } = $props();
  let about_open = $state(false);
  let settings_open = $state(false);
  let diagnostics_open = $state(false);
</script>

<Menubar.Root class="h-12 shrink-0 rounded-none border-x-0 border-t-0 px-3">
  <div class="flex h-full w-full flex-row items-center gap-1.5">
    <Button
      variant={board_panel_open ? "secondary" : "ghost"}
      size="sm"
      class="h-8 shrink-0 gap-1.5 px-2.5 text-sm font-semibold"
      onclick={() => (board_panel_open = !board_panel_open)}
      aria-label={board_panel_open ? m.board_close() : m.board_open()}
      aria-expanded={board_panel_open}
      aria-controls="board-drawer"
      title={board_panel_open ? m.board_close() : m.board_open()}
    >
      <LayoutDashboardIcon class="size-4" />
      <span>Cardbe</span>
      <ChevronDownIcon
        class={cn(
          "size-3.5 transition-transform",
          board_panel_open && "rotate-180",
        )}
      />
    </Button>

    <div class="flex min-w-0 flex-1 items-center">
      <div
        class="flex flex-row items-center gap-0.5 rounded-md border bg-muted/30 p-0.5"
      >
        <Menubar.Menu>
          <Menubar.Trigger>{m.menu_file()}</Menubar.Trigger>
          <Menubar.Content>
            <Menubar.Item onclick={() => void onPrepareTaskImport()}
              >{m.task_import()}</Menubar.Item
            >
            <Menubar.Item onclick={() => void board.export_to_file()}
              >{m.menu_export_board()}</Menubar.Item
            >
            <Menubar.Item onclick={() => void onPrepareImport()}
              >{m.menu_import_board()}</Menubar.Item
            >
            <Menubar.Separator />
            <Menubar.Item onclick={() => void onExportAllBoards()}
              >{m.menu_backup()}</Menubar.Item
            >
            <Menubar.Item onclick={() => void onImportAllBoards()}
              >{m.menu_restore()}</Menubar.Item
            >
          </Menubar.Content>
        </Menubar.Menu>
        <Menubar.Menu>
          <Menubar.Trigger>{m.common_edit()}</Menubar.Trigger>
          <Menubar.Content>
            <Menubar.Item
              disabled={!board.can_undo || board.undo_in_progress}
              onclick={() => void board.undo()}
            >
              {m.menu_undo()}
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
          <Menubar.Trigger>{m.board_title()}</Menubar.Trigger>
          <Menubar.Content>
            {#if !read_only}
              <Menubar.Item onclick={onAddTask}>
                {m.task_new()}
                <Menubar.Shortcut>
                  <Kbd.Group>
                    <Kbd.Root>Ctrl</Kbd.Root>
                    <Kbd.Root>Shift</Kbd.Root>
                    <Kbd.Root>T</Kbd.Root>
                  </Kbd.Group>
                </Menubar.Shortcut>
              </Menubar.Item>
              <Menubar.Item onclick={onAddColumn}>
                {m.column_new()}
                <Menubar.Shortcut>
                  <Kbd.Group>
                    <Kbd.Root>Ctrl</Kbd.Root>
                    <Kbd.Root>Shift</Kbd.Root>
                    <Kbd.Root>C</Kbd.Root>
                  </Kbd.Group>
                </Menubar.Shortcut>
              </Menubar.Item>
              <Menubar.Separator />
            {/if}
            <Menubar.Item onclick={onOpenIrohShare}
              >{m.board_sharing()}</Menubar.Item
            >
            <Menubar.Item onclick={onOpenBoardShare}
              >{m.board_publish_open()}</Menubar.Item
            >
          </Menubar.Content>
        </Menubar.Menu>
        <Menubar.Menu>
          <Menubar.Trigger>{m.menu_help()}</Menubar.Trigger>
          <Menubar.Content>
            <Menubar.Item onclick={() => (about_open = true)}>
              {m.menu_about()}
            </Menubar.Item>
            <Menubar.Separator />
            <Menubar.Item onclick={() => (diagnostics_open = true)}>
              {m.menu_diagnostics()}
            </Menubar.Item>
          </Menubar.Content>
        </Menubar.Menu>
        <Menubar.Menu>
          <Menubar.Trigger>{m.settings_title()}</Menubar.Trigger>
          <Menubar.Content>
            <Menubar.Item onclick={() => (settings_open = true)}
              >{m.settings_language()}</Menubar.Item
            >
          </Menubar.Content>
        </Menubar.Menu>
      </div>
    </div>

    {#if pending_device_count > 0}
      <Button
        variant="outline"
        size="sm"
        class="shrink-0 gap-1.5 border-primary/40 bg-primary/5 text-primary"
        onclick={onOpenDeviceRequests}
        aria-label={`${pending_device_count} device approval requests`}
        title={m.ui_review_device_approval_requests()}
        ><ShieldCheckIcon class="size-4" />
        <span class="hidden xl:inline">{m.ui_device_requests()}</span><span
          class="rounded-full bg-primary px-1.5 text-xs text-primary-foreground"
          >{pending_device_count}</span
        ></Button
      >
    {/if}
    {#if removed_access_count > 0}
      <Button
        variant="outline"
        size="sm"
        class="shrink-0 gap-1.5 border-destructive/40 bg-destructive/5 text-destructive"
        onclick={onOpenRemovedAccess}
        aria-label={m.share_boards_need_access({
          count: formatNumber(removed_access_count),
        })}
        title={m.ui_request_access_to_shared_boards_again()}
        ><LinkIcon class="size-4" />
        <span>{m.share_request_access_again()}</span><span
          class="rounded-full bg-destructive px-1.5 text-xs text-white"
          >{formatNumber(removed_access_count)}</span
        ></Button
      >
    {/if}
    {#if web_publish_count > 0}
      <Button
        variant="outline"
        size="sm"
        class={cn(
          "shrink-0 gap-1.5",
          web_publish_status === "error" &&
            "border-destructive/50 text-destructive hover:bg-destructive/10 hover:text-destructive",
        )}
        onclick={onOpenBoardShare}
        aria-label={m.share_open_publish({ detail: web_publish_detail })}
        title={web_publish_detail}
      >
        <span class="relative">
          <RadioTowerIcon class="size-4" />
          <span
            class={cn(
              "absolute -right-1 -top-1 size-2 rounded-full border border-background",
              web_publish_status === "error"
                ? "bg-destructive"
                : web_publish_status === "updating"
                  ? "animate-pulse bg-amber-500"
                  : "bg-emerald-500",
            )}
          ></span>
        </span>
        <span class="hidden xl:inline">{m.board_publish()}</span>
        <span class="text-xs tabular-nums text-muted-foreground"
          >{web_publish_count}</span
        >
      </Button>
    {/if}

    <div class="ml-auto flex min-w-0 items-center gap-1.5 border-l pl-2">
      <Button
        variant="secondary"
        size="sm"
        class="shrink-0 gap-1.5"
        onclick={onOpenAllTasks}
        aria-label={m.task_explorer_open()}
        title={m.task_explorer_open()}
      >
        <ListTodoIcon class="size-4" />
        <span class="hidden lg:inline">{m.task_explorer()}</span>
      </Button>

      <InputGroup.Root
        class="min-w-0 w-36 shrink bg-muted/40 transition-colors focus-within:bg-background sm:w-44 lg:w-60"
        title={search_error ? m.board_search_error() : undefined}
      >
        <InputGroup.Input
          aria-label={m.board_search_tasks()}
          aria-busy={search_pending}
          placeholder={m.board_search_tasks()}
          bind:value={search_text}
        />
        <InputGroup.Addon>
          {#if search_pending}
            <LoaderCircleIcon class="animate-spin" />
          {:else}
            <SearchIcon />
          {/if}
        </InputGroup.Addon>
        {#if search_text.length > 0}
          <InputGroup.Addon align="inline-end">
            <InputGroup.Button
              aria-label={m.board_clear_search()}
              title={m.board_clear_search()}
              size="icon-xs"
              onclick={() => (search_text = "")}
            >
              <XIcon />
            </InputGroup.Button>
          </InputGroup.Addon>
        {/if}
        <span class="sr-only" aria-live="polite">
          {#if search_pending}
            {m.board_searching()}
          {:else if search_error}
            {m.board_search_error()}
          {/if}
        </span>
      </InputGroup.Root>
    </div>

    <div class="flex shrink-0 items-center gap-1 border-l pl-2">
      <Button
        onclick={() => {
          note_open = !note_open;
        }}
        variant="outline"
        size="icon-sm"
        class="flex-none"
        aria-pressed={note_open}
        aria-label={note_open ? m.notes_close() : m.notes_open()}
        title={note_open ? m.notes_close() : m.notes_open()}
      >
        <StickyNoteIcon />
      </Button>

      <Button
        onclick={onOpenTaskTemplates}
        variant="outline"
        size="icon-sm"
        class="flex-none"
        aria-label={m.task_template_open()}
        title={m.task_template_open()}
      >
        <LayoutTemplateIcon />
      </Button>

      <Button
        onclick={() => {
          void board.set_notify_enabled(!board.notify_enabled);
        }}
        variant="outline"
        size="icon-sm"
        class="flex-none"
        disabled={board.notification_setting_updating}
        aria-pressed={board.notify_enabled}
        aria-label={board.notify_enabled
          ? m.settings_notify_disable()
          : m.settings_notify_enable()}
        title={board.notify_enabled
          ? m.settings_notify_disable()
          : m.settings_notify_enable()}
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
        class="size-8 flex-none"
        title={selected_view !== "board" ? m.workspace_board_only() : undefined}
      >
        <Button
          onclick={() => {
            task_expand_mode = !task_expand_mode;
          }}
          variant="outline"
          size="icon-sm"
          disabled={selected_view !== "board"}
          aria-pressed={task_expand_mode}
          aria-label={selected_view !== "board"
            ? m.task_details_board()
            : task_expand_mode
              ? m.task_details_hide()
              : m.task_details_show()}
          title={selected_view !== "board"
            ? undefined
            : task_expand_mode
              ? m.task_details_hide()
              : m.task_details_show()}
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
        size="icon-sm"
        class="flex-none"
        aria-label={m.settings_theme_toggle()}
        title={m.settings_theme_toggle()}
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

<HelpDialogs
  bind:about_open
  bind:diagnostics_open
  {update_check_in_progress}
  {onCheckForUpdates}
/>
<LanguageSettings bind:open={settings_open} />
