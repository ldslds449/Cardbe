<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu/index.js";
  import * as Kbd from "$lib/components/ui/kbd/index.js";
  import { Switch } from "$lib/components/ui/switch/index.js";
  import { m } from "$lib/paraglide/messages";
  import {
    parseCommandError,
    translateCommandError,
    type CommandError,
  } from "$lib/command-errors";
  import { logger } from "$lib/logger";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import BanIcon from "@lucide/svelte/icons/ban";
  import EllipsisIcon from "@lucide/svelte/icons/ellipsis";
  import RotateCcwIcon from "@lucide/svelte/icons/rotate-ccw";
  import {
    display_shortcut,
    GLOBAL_SHORTCUTS_CHANGED_EVENT,
    held_modifiers,
    shortcut_from_keydown,
    type GlobalShortcutOverview,
    type ShortcutAction,
  } from "$lib/global-shortcuts";

  type Feedback =
    | { kind: "error"; error: CommandError }
    | { kind: "modifier" }
    | { kind: "duplicate"; owner: ShortcutAction; shortcut: string };

  let overview = $state<GlobalShortcutOverview | null>(null);
  let recording = $state<ShortcutAction | null>(null);
  let held = $state<string[]>([]);
  let saving = $state(false);
  let feedback = $state<{ action: ShortcutAction; value: Feedback } | null>(
    null,
  );
  let load_error = $state<CommandError | null>(null);

  function action_label(action: ShortcutAction) {
    switch (action) {
      case "quick_task":
        return m.task_new();
      case "quick_note":
        return m.ui_new_note();
      case "floating_tasks":
        return m.tray_toggle_floating_tasks();
      case "show_cardbe":
        return m.tray_show();
    }
  }

  function command_error(caught: unknown): CommandError {
    return parseCommandError(caught) ?? { code: "INTERNAL_ERROR" };
  }

  async function refresh() {
    try {
      overview = await invoke<GlobalShortcutOverview>("get_global_shortcuts");
      load_error = null;
    } catch (caught) {
      logger.warn("settings.global_shortcuts_load.failed", caught);
      load_error = command_error(caught);
    }
  }

  onMount(() => {
    void refresh();
    if (!isTauri()) {
      return;
    }
    // The tray menu can also turn the group on or off.
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen(GLOBAL_SHORTCUTS_CHANGED_EVENT, () => {
      if (!saving) {
        void refresh();
      }
    })
      .then((stop) => {
        if (disposed) {
          stop();
        } else {
          unlisten = stop;
        }
      })
      .catch((caught) =>
        logger.warn("settings.global_shortcuts_listen.failed", caught),
      );
    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  function stop_recording() {
    recording = null;
    held = [];
  }

  // Saving never disables the controls: a disabled control loses focus, and
  // the dialog then moves focus (and the scroll position) back to the top.
  async function set_enabled(enabled: boolean) {
    if (saving) {
      return;
    }
    saving = true;
    load_error = null;
    try {
      overview = await invoke<GlobalShortcutOverview>(
        "set_global_shortcuts_enabled",
        { enabled },
      );
    } catch (caught) {
      logger.warn("settings.global_shortcuts_toggle.failed", caught);
      load_error = command_error(caught);
    } finally {
      saving = false;
    }
  }

  async function save(
    action: ShortcutAction,
    shortcut: string,
    take_over = false,
  ) {
    if (saving) {
      return;
    }
    stop_recording();
    feedback = null;
    const owner = shortcut
      ? overview?.shortcuts.find(
          (status) => status.action !== action && status.shortcut === shortcut,
        )
      : undefined;
    if (owner && !take_over) {
      feedback = {
        action,
        value: { kind: "duplicate", owner: owner.action, shortcut },
      };
      return;
    }
    saving = true;
    try {
      overview = await invoke<GlobalShortcutOverview>("set_global_shortcut", {
        action,
        shortcut,
        takeOver: take_over,
      });
    } catch (caught) {
      logger.warn("settings.global_shortcut_save.failed", caught);
      feedback = {
        action,
        value: { kind: "error", error: command_error(caught) },
      };
    } finally {
      saving = false;
    }
  }

  function handle_keydown(event: KeyboardEvent, action: ShortcutAction) {
    if (recording !== action) {
      return;
    }
    // Keep Escape and other keys from reaching the dialog while recording.
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape") {
      stop_recording();
      return;
    }
    const result = shortcut_from_keydown(event);
    if (result.kind === "incomplete") {
      held = held_modifiers(event);
      return;
    }
    if (result.kind === "modifier") {
      held = [];
      feedback = { action, value: { kind: "modifier" } };
      return;
    }
    void save(action, result.shortcut);
  }

  function handle_keyup(event: KeyboardEvent, action: ShortcutAction) {
    if (recording === action) {
      held = held_modifiers(event);
    }
  }
</script>

<fieldset
  class="relative space-y-3"
  disabled={overview === null}
  aria-busy={saving}
>
  <legend class="text-sm font-medium">{m.settings_shortcuts_title()}</legend>
  <Switch
    class="absolute end-0 top-0"
    checked={overview?.enabled ?? false}
    aria-label={m.tray_enable_shortcuts()}
    aria-describedby="global-shortcuts-description"
    onCheckedChange={(enabled) => void set_enabled(enabled)}
  />
  <p
    id="global-shortcuts-description"
    class="pe-12 text-sm text-muted-foreground"
  >
    {overview && !overview.enabled
      ? m.settings_shortcuts_paused()
      : m.settings_shortcuts_description()}
  </p>
  {#if load_error}
    <p role="alert" class="text-xs text-destructive">
      {translateCommandError(load_error)}
    </p>
  {/if}
  <div
    class={[
      "-mx-2 transition-opacity",
      overview && !overview.enabled && "opacity-50",
    ]}
  >
    {#each overview?.shortcuts ?? [] as status (status.action)}
      {@const label = action_label(status.action)}
      {@const message =
        feedback?.action === status.action ? feedback.value : null}
      {@const is_recording = recording === status.action}
      {@const off = status.shortcut === null}
      {@const unavailable = overview?.enabled && !off && !status.active}
      <div class="group rounded-md px-2 py-1 hover:bg-muted/50">
        <div class="flex items-center gap-1">
          <span
            id="global-shortcut-label-{status.action}"
            class={[
              "min-w-0 flex-1 truncate text-sm",
              off && "text-muted-foreground",
            ]}
            title={label}>{label}</span
          >
          <Button
            id="global-shortcut-button-{status.action}"
            variant="ghost"
            size="sm"
            class={[
              "h-7 px-1.5 font-normal",
              off &&
                "border border-dashed text-muted-foreground hover:text-foreground",
              is_recording && "ring-[3px] ring-ring/50",
            ]}
            aria-labelledby="global-shortcut-label-{status.action} global-shortcut-button-{status.action}"
            aria-describedby="global-shortcuts-description global-shortcut-hint-{status.action}"
            aria-pressed={is_recording}
            onclick={() => {
              feedback = null;
              held = [];
              recording = status.action;
            }}
            onkeydown={(event) => handle_keydown(event, status.action)}
            onkeyup={(event) => handle_keyup(event, status.action)}
            onblur={() => {
              if (is_recording) {
                stop_recording();
              }
            }}
          >
            {#if is_recording && held.length > 0}
              <Kbd.Group>
                {#each held as key (key)}
                  <Kbd.Root>{key}</Kbd.Root>
                {/each}
                <span class="text-muted-foreground">…</span>
              </Kbd.Group>
            {:else if is_recording}
              <span class="text-xs text-muted-foreground"
                >{m.settings_shortcut_recording()}</span
              >
            {:else if status.shortcut}
              <Kbd.Group>
                {#each status.shortcut.split("+") as key (key)}
                  <Kbd.Root class={unavailable ? "text-warning" : undefined}
                    >{key}</Kbd.Root
                  >
                {/each}
              </Kbd.Group>
            {:else}
              <BanIcon class="size-3.5" aria-hidden="true" />
              <span class="text-xs">{m.settings_shortcut_none()}</span>
            {/if}
          </Button>
          <DropdownMenu.Root>
            <DropdownMenu.Trigger>
              {#snippet child({ props })}
                <Button
                  {...props}
                  id="global-shortcut-options-{status.action}"
                  variant="ghost"
                  size="icon-sm"
                  class="size-7 text-muted-foreground opacity-0 group-focus-within:opacity-100 group-hover:opacity-100 data-[state=open]:opacity-100"
                  aria-label={m.settings_shortcut_options_for({
                    action: label,
                  })}
                  title={m.settings_shortcut_options_for({ action: label })}
                >
                  <EllipsisIcon />
                </Button>
              {/snippet}
            </DropdownMenu.Trigger>
            <DropdownMenu.Content
              align="end"
              class="min-w-44"
              onCloseAutoFocus={(event) => {
                // A pointer-opened menu never focuses its trigger, and the dialog
                // ignores focus moved while the menu is open. If the pointer then
                // focuses the closing menu, the dialog restores an older element,
                // often at its top. Focus the trigger once the dialog resumes so
                // it becomes the element the dialog restores.
                event.preventDefault();
                const trigger = document.getElementById(
                  `global-shortcut-options-${status.action}`,
                );
                queueMicrotask(() => trigger?.focus({ preventScroll: true }));
              }}
            >
              <DropdownMenu.Item
                disabled={status.shortcut === status.default_shortcut}
                onclick={() =>
                  void save(status.action, status.default_shortcut)}
              >
                <RotateCcwIcon />
                {m.settings_shortcut_reset()}
                <DropdownMenu.Shortcut class="tracking-normal"
                  >{display_shortcut(
                    status.default_shortcut,
                  )}</DropdownMenu.Shortcut
                >
              </DropdownMenu.Item>
              <DropdownMenu.Item
                disabled={off}
                onclick={() => void save(status.action, "")}
              >
                <BanIcon />
                {m.settings_shortcut_disable()}
              </DropdownMenu.Item>
            </DropdownMenu.Content>
          </DropdownMenu.Root>
        </div>
        <div
          id="global-shortcut-hint-{status.action}"
          class="pb-1 text-xs empty:hidden"
          aria-live="polite"
        >
          {#if message?.kind === "error"}
            <p class="text-destructive">
              {translateCommandError(message.error)}
            </p>
          {:else if message?.kind === "modifier"}
            <p class="text-destructive">
              {m.settings_shortcut_modifier_required()}
            </p>
          {:else if message?.kind === "duplicate"}
            <p class="flex flex-wrap items-center gap-x-2 text-destructive">
              {m.settings_shortcut_assigned_to({
                shortcut: display_shortcut(message.shortcut),
                action: action_label(message.owner),
              })}
              <Button
                variant="link"
                size="sm"
                class="h-auto p-0 text-xs"
                onclick={() => void save(status.action, message.shortcut, true)}
                >{m.settings_shortcut_take_over()}</Button
              >
            </p>
          {:else if unavailable}
            <p class="text-warning">{m.settings_shortcut_inactive()}</p>
          {/if}
        </div>
      </div>
    {/each}
  </div>
</fieldset>
