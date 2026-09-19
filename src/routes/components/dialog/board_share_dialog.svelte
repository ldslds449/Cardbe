<script lang="ts">
  import { toast } from "svelte-sonner";
  import PlusIcon from "@lucide/svelte/icons/plus";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import CalendarDaysIcon from "@lucide/svelte/icons/calendar-days";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import RefreshCwIcon from "@lucide/svelte/icons/refresh-cw";
  import SearchIcon from "@lucide/svelte/icons/search";
  import Trash2Icon from "@lucide/svelte/icons/trash-2";
  import PowerIcon from "@lucide/svelte/icons/power";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import CircleCheckIcon from "@lucide/svelte/icons/circle-check";
  import { CalendarDate, getLocalTimeZone, today } from "@internationalized/date";
  import { Button } from "$lib/components/ui/button/index.js";
  import Calendar from "$lib/components/ui/calendar/calendar.svelte";
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import * as AlertDialog from "$lib/components/ui/alert-dialog/index.js";
  import * as ContextMenu from "$lib/components/ui/context-menu/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { TagInput } from "$lib/components/ui/tag-input/index.js";
  import * as Popover from "$lib/components/ui/popover/index.js";
  import type { Column } from "../../type/column.svelte";
  import {
    build_share_snapshot,
    forget_managed_share,
    is_managed_share_enabled,
    load_managed_shares,
    publish_share,
    revoke_share,
    resolve_share_selection,
    save_managed_share,
    share_content_signature,
    type ManagedShare,
  } from "../../share";

  let {
    open = $bindable(),
    columns,
    default_title,
  }: {
    open: boolean;
    columns: Column[];
    default_title: string;
  } = $props();

  let share = $state<ManagedShare | null>(null);
  let title = $state("");
  let selected_column_ids = $state<string[]>([]);
  let selected_task_ids = $state<string[]>([]);
  let selected_labels = $state<string[]>([]);
  let task_search = $state("");
  let reuse_link_open = $state(false);
  let requested_link = $state("");
  let expiration_enabled = $state(true);
  let expiration_date = $state(today(getLocalTimeZone()).add({ days: 30 }));
  let expiration_open = $state(false);
  let context_menu_share_id = $state<string | null>(null);
  let publishing = $state(false);
  let revoking = $state(false);
  let delete_confirm_open = $state(false);
  let editing = $state(false);
  let initialized_for_open = false;
  const managed_shares = $derived(load_managed_shares());
  const share_date_formatter = new Intl.DateTimeFormat("en-US", {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
  const share_datetime_formatter = new Intl.DateTimeFormat("en-US", {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });

  function load_share(candidate: ManagedShare | null, editing_state = candidate === null) {
    share = candidate;
    editing = editing_state;
    title = candidate?.title || `${default_title || "Cardbe"} board`;
    const available_ids = new Set(columns.map((column) => column.id));
    const candidate_column_ids = Array.isArray(candidate?.selected_column_ids)
      ? candidate.selected_column_ids
      : [];
    selected_column_ids = candidate
      ? candidate_column_ids.filter((id) => available_ids.has(id))
      : [];
    const available_task_ids = new Set(
      columns.flatMap((column) => column.tasks.map((task) => task.id)),
    );
    const candidate_task_ids = Array.isArray(candidate?.selected_task_ids)
      ? candidate.selected_task_ids
      : [];
    selected_task_ids = candidate
      ? candidate_task_ids.filter((id) => available_task_ids.has(id))
      : [];
    selected_labels = [...(candidate?.selected_labels ?? [])];
    if (candidate?.expires_at) {
      const expires = new Date(candidate.expires_at);
      expiration_enabled = true;
      expiration_date = new CalendarDate(expires.getFullYear(), expires.getMonth() + 1, expires.getDate());
    } else if (candidate) {
      expiration_enabled = false;
    } else {
      expiration_enabled = true;
      expiration_date = today(getLocalTimeZone()).add({ days: 30 });
    }
    task_search = "";
    reuse_link_open = false;
    requested_link = "";
    context_menu_share_id = null;
  }

  function cancel_editing() {
    if (!share) return;
    load_share(share);
  }

  $effect(() => {
    if (open && !initialized_for_open) {
      load_share(managed_shares[0] ?? null);
      initialized_for_open = true;
    } else if (!open) {
      initialized_for_open = false;
    }
  });

  function set_column_selected(column_id: string, checked: boolean) {
    const task_ids = columns.find((column) => column.id === column_id)?.tasks.map((task) => task.id) ?? [];
    selected_column_ids = checked
      ? Array.from(new Set([...selected_column_ids, column_id]))
      : selected_column_ids.filter((id) => id !== column_id);
    selected_task_ids = checked
      ? Array.from(new Set([...selected_task_ids, ...task_ids]))
      : selected_task_ids.filter((id) => !task_ids.includes(id));
  }

  function set_task_selected(column_id: string, task_id: string, checked: boolean) {
    if (checked) {
      selected_column_ids = Array.from(new Set([...selected_column_ids, column_id]));
      selected_task_ids = Array.from(new Set([...selected_task_ids, task_id]));
    } else {
      selected_task_ids = selected_task_ids.filter((id) => id !== task_id);
    }
  }

  function expiration_value(): Date | null {
    if (!expiration_enabled) return null;
    return new Date(
      expiration_date.year,
      expiration_date.month - 1,
      expiration_date.day,
      23,
      59,
      59,
      999,
    );
  }

  const visible_columns = $derived.by(() => {
    const needle = task_search.trim().toLocaleLowerCase();
    if (!needle) return columns;
    return columns
      .map((column) => {
        if (column.name.toLocaleLowerCase().includes(needle)) return column;
        return {
          ...column,
          tasks: column.tasks.filter((task) =>
            [task.title, task.description, ...task.labels]
              .join(" ")
              .toLocaleLowerCase()
              .includes(needle)),
        };
      })
      .filter((column) => column.tasks.length > 0);
  });
  const available_labels = $derived(
    Array.from(new Set(columns.flatMap((column) => column.tasks.flatMap((task) => task.labels)))).sort(),
  );
  const effective_selection = $derived(
    resolve_share_selection(columns, selected_column_ids, selected_task_ids, selected_labels),
  );
  const visible_task_ids = $derived(
    visible_columns.flatMap((column) => column.tasks.map((task) => task.id)),
  );
  const all_visible_tasks_selected = $derived(
    visible_task_ids.length > 0 && visible_task_ids.every((id) => selected_task_ids.includes(id)),
  );
  const no_visible_tasks_selected = $derived(
    visible_task_ids.every((id) => !selected_task_ids.includes(id)),
  );

  function set_visible_tasks_selected(include: boolean) {
    const task_ids = new Set(visible_task_ids);
    const column_ids = new Set(
      visible_columns
        .filter((column) => column.tasks.length > 0)
        .map((column) => column.id),
    );
    if (include) {
      selected_task_ids = Array.from(new Set([...selected_task_ids, ...task_ids]));
      selected_column_ids = Array.from(new Set([...selected_column_ids, ...column_ids]));
      return;
    }

    selected_task_ids = selected_task_ids.filter((id) => !task_ids.has(id));
    selected_column_ids = selected_column_ids.filter((column_id) => {
      if (!column_ids.has(column_id)) return true;
      return columns
        .find((column) => column.id === column_id)
        ?.tasks.some((task) => selected_task_ids.includes(task.id)) ?? false;
    });
  }

  async function publish() {
    if (publishing) return;
    publishing = true;
    try {
      const was_update = share !== null;
      const was_reused = !was_update && requested_link.trim() !== "";
      const snapshot = build_share_snapshot(
        columns,
        effective_selection.selected_column_ids,
        effective_selection.selected_task_ids,
        title,
      );
      const updated_share = await publish_share(
        snapshot,
        selected_column_ids,
        selected_task_ids,
        expiration_value(),
        // Updating an existing share keeps its URL; new shares receive an
        // independent capability ID and remain active alongside existing ones.
        share,
        selected_labels,
        requested_link,
        // Saving from this dialog is an explicit user action, so it may recover
        // an ID that was retired by a previous disable or an in-flight revoke.
        was_update,
      );
      share = updated_share;
      requested_link = "";
      save_managed_share(
        updated_share,
        share_content_signature(
          columns,
          effective_selection.selected_column_ids,
          effective_selection.selected_task_ids,
          title,
        ),
      );
      load_share(updated_share, false);
      toast.success(
        was_update
          ? "Share link updated"
          : was_reused
            ? "Previous share link reused"
            : "Share link published",
      );
    } catch (error) {
      console.error("Couldn't publish board share", error);
      toast.error(error instanceof Error ? error.message : "Couldn't publish the share link");
    } finally {
      publishing = false;
    }
  }

  async function copy_link(target: ManagedShare | null = share) {
    if (!target) return;
    try {
      await navigator.clipboard.writeText(target.url);
      toast.success("Share link copied");
    } catch {
      toast.error("Couldn't copy the link");
    }
  }

  async function disable(target: ManagedShare | null = share) {
    if (!target || revoking) return;
    revoking = true;
    try {
      await revoke_share(target);
      const disabled_share = { ...target, enabled: false };
      const selection = resolve_share_selection(
        columns,
        target.selected_column_ids,
        target.selected_task_ids,
        target.selected_labels,
      );
      save_managed_share(
        disabled_share,
        share_content_signature(
          columns,
          selection.selected_column_ids,
          selection.selected_task_ids,
          target.title,
        ),
      );
      if (share?.id === target.id) load_share(disabled_share);
      toast.success("Share link disabled. You can enable it again later.");
    } catch (error) {
      console.error("Couldn't disable board share", error);
      toast.error(error instanceof Error ? error.message : "Couldn't disable the share link");
    } finally {
      revoking = false;
    }
  }

  async function enable(target: ManagedShare | null = share) {
    if (!target || publishing || is_managed_share_enabled(target)) return;
    publishing = true;
    try {
      const selection = resolve_share_selection(
        columns,
        target.selected_column_ids,
        target.selected_task_ids,
        target.selected_labels,
      );
      const snapshot = build_share_snapshot(
        columns,
        selection.selected_column_ids,
        selection.selected_task_ids,
        target.title,
      );
      const enabled_share = await publish_share(
        snapshot,
        target.selected_column_ids,
        target.selected_task_ids,
        target.expires_at ? new Date(target.expires_at) : null,
        target,
        target.selected_labels,
        null,
        true,
      );
      save_managed_share(
        enabled_share,
        share_content_signature(
          columns,
          selection.selected_column_ids,
          selection.selected_task_ids,
          target.title,
        ),
      );
      if (share?.id === target.id) load_share(enabled_share);
      toast.success("Share link enabled");
    } catch (error) {
      console.error("Couldn't enable board share", error);
      toast.error(error instanceof Error ? error.message : "Couldn't enable the share link");
    } finally {
      publishing = false;
    }
  }

  async function delete_share() {
    if (!share || revoking) return;
    revoking = true;
    const deleted_id = share.id;
    try {
      await revoke_share(share);
      forget_managed_share(deleted_id);
      const remaining = load_managed_shares().filter((candidate) => candidate.id !== deleted_id);
      load_share(remaining[0] ?? null);
      delete_confirm_open = false;
      toast.success("Share link deleted");
    } catch (error) {
      console.error("Couldn't delete board share", error);
      toast.error(error instanceof Error ? error.message : "Couldn't delete the share link");
    } finally {
      revoking = false;
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="flex h-[min(90vh,48rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-5xl">
    <Dialog.Header class="shrink-0 px-6 pb-4 pt-6">
      <Dialog.Title>Share board</Dialog.Title>
      <Dialog.Description>
        Create and manage read-only links for this board. Links resume at the same address when Cardbe starts again.
      </Dialog.Description>
    </Dialog.Header>

    <div class="grid min-h-0 flex-1 grid-rows-[auto_minmax(0,1fr)] overflow-hidden md:grid-cols-[16rem_minmax(0,1fr)] md:grid-rows-1">
      <aside class="flex max-h-52 min-h-0 flex-col gap-3 overflow-y-auto border-b bg-muted/20 p-4 md:max-h-none md:border-b-0 md:border-r" aria-label="Shared board links">
        <section class="grid gap-2.5" aria-labelledby="existing-shares-heading">
          <div class="flex items-center gap-2">
            <h3 id="existing-shares-heading" class="text-sm font-semibold">Your share links</h3>
            <span class="rounded-full bg-muted px-2 py-0.5 text-xs font-medium text-muted-foreground">
              {managed_shares.length}
            </span>
          </div>
          {#if managed_shares.length > 0}
            <div class="grid gap-1">
              {#each managed_shares as candidate (candidate.id)}
                {@const candidate_enabled = is_managed_share_enabled(candidate)}
                <ContextMenu.Root
                  open={context_menu_share_id === candidate.id}
                  onOpenChange={(menu_open) => {
                    if (menu_open) {
                      context_menu_share_id = candidate.id;
                    } else if (context_menu_share_id === candidate.id) {
                      context_menu_share_id = null;
                    }
                  }}
                >
                  <ContextMenu.Trigger>
                    {#snippet child({ props })}
                      <button
                        {...props}
                        type="button"
                        class={`grid w-full min-w-0 gap-1 rounded-md border px-3 py-2.5 text-left transition-colors ${
                          share?.id === candidate.id
                            ? "border-primary bg-primary/10 ring-1 ring-primary/30"
                            : "bg-card hover:border-primary/50 hover:bg-muted/40"
                        }`}
                        disabled={publishing || revoking}
                        onpointerdown={(event) => event.stopPropagation()}
                        onclick={(event) => {
                          event.stopPropagation();
                          load_share(candidate);
                        }}
                        aria-pressed={share?.id === candidate.id}
                        title={`${candidate.title} — Right-click for actions`}
                      >
                        <span class="flex min-w-0 items-center gap-2">
                          <span class="truncate text-sm font-medium">{candidate.title}</span>
                          <span
                            class={`size-1.5 shrink-0 rounded-full ${candidate_enabled ? "bg-success" : "bg-muted-foreground/50"}`}
                            aria-label={candidate_enabled ? "Active" : "Disabled"}
                          ></span>
                        </span>
                        <span class="text-xs text-muted-foreground">
                          <span class={candidate_enabled ? "text-success" : undefined}>
                            {candidate_enabled ? "Active" : "Disabled"}
                          </span>
                          {" · "}
                          {candidate.expires_at ? `Expires ${share_date_formatter.format(new Date(candidate.expires_at))}` : "No expiration"}
                        </span>
                      </button>
                    {/snippet}
                  </ContextMenu.Trigger>
                  <ContextMenu.Content class="w-56 rounded-lg p-1.5 shadow-lg">
                    <ContextMenu.Item
                      class="h-9 gap-2.5 rounded-md px-2.5"
                      disabled={publishing || revoking}
                      onclick={() => load_share(candidate, true)}
                    >
                      <PencilIcon class="size-4 text-muted-foreground" />
                      Edit settings
                    </ContextMenu.Item>
                    <ContextMenu.Item
                      class="h-9 gap-2.5 rounded-md px-2.5"
                      onclick={() => void copy_link(candidate)}
                    >
                      <CopyIcon class="size-4 text-muted-foreground" />
                      Copy address
                    </ContextMenu.Item>
                    <ContextMenu.Separator class="my-1" />
                    <ContextMenu.Item
                      class="h-9 gap-2.5 rounded-md px-2.5"
                      disabled={publishing || revoking}
                      onclick={() => candidate_enabled ? void disable(candidate) : void enable(candidate)}
                    >
                      <PowerIcon
                        class={candidate_enabled ? "size-4 text-success" : "size-4 text-muted-foreground"}
                      />
                      {candidate_enabled ? "Disable link" : "Enable link"}
                    </ContextMenu.Item>
                    <ContextMenu.Separator class="my-1" />
                    <ContextMenu.Item
                      variant="destructive"
                      class="h-9 gap-2.5 rounded-md px-2.5"
                      disabled={publishing || revoking}
                      onclick={() => {
                        load_share(candidate);
                        delete_confirm_open = true;
                      }}
                    >
                      <Trash2Icon />
                      Delete link
                    </ContextMenu.Item>
                  </ContextMenu.Content>
                </ContextMenu.Root>
              {/each}
            </div>
          {:else}
            <div class="px-2 py-3 text-center">
              <p class="text-xs text-muted-foreground">No links yet. Create one to share this board.</p>
            </div>
          {/if}
        </section>

        <section aria-labelledby="new-share-heading">
          <Button
            class={`w-full justify-start ${!share ? "ring-2 ring-primary/30" : ""}`}
            variant={share ? "outline" : "default"}
            disabled={publishing || revoking || !share}
            onclick={() => load_share(null)}
            aria-current={!share ? "page" : undefined}
          >
            <PlusIcon />
            <span id="new-share-heading">{share ? "New share link" : "Creating new link"}</span>
          </Button>
        </section>
      </aside>

      <div class="grid min-h-0 gap-5 overflow-y-auto px-5 py-5 sm:px-6">
        {#if share}
          <section class={`grid min-h-[12.5rem] content-start gap-4 rounded-xl border p-4 ${is_managed_share_enabled(share) ? "border-success/50 bg-success/5" : "border-border bg-muted/30"}`}>
            <div class="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
              <div class="flex min-w-0 items-start gap-3">
                <div class={`flex size-9 shrink-0 items-center justify-center rounded-full ${is_managed_share_enabled(share) ? "bg-success/15 text-success" : "bg-muted text-muted-foreground"}`}>
                  <PowerIcon class="size-4" />
                </div>
                <div class="min-w-0">
                  <div class="flex flex-wrap items-center gap-2">
                    <h3 class="font-semibold">{editing ? "Edit share link" : share.title}</h3>
                    <span class={`inline-flex min-w-16 justify-center rounded-full px-2 py-0.5 text-xs font-medium ${is_managed_share_enabled(share) ? "bg-success/15 text-success" : "border border-border bg-muted text-muted-foreground"}`}>
                      {is_managed_share_enabled(share) ? "Active" : "Disabled"}
                    </span>
                  </div>
                  <p class="mt-1 min-h-10 text-sm text-muted-foreground">
                    {is_managed_share_enabled(share)
                      ? "Anyone on this local network with the link can view the selected content."
                      : "This address and its settings are saved, but nobody can open it."}
                  </p>
                </div>
              </div>
              {#if !editing}
                <div class="flex shrink-0 flex-wrap gap-2">
                  {#if is_managed_share_enabled(share)}
                    <Button class="w-24" variant="outline" size="sm" disabled={publishing || revoking} onclick={() => void disable()}>
                      <PowerIcon />
                      {revoking ? "Disabling..." : "Disable"}
                    </Button>
                  {:else}
                    <Button class="w-24" size="sm" disabled={publishing || revoking} onclick={() => void enable()}>
                      <PowerIcon />
                      {publishing ? "Enabling..." : "Enable"}
                    </Button>
                  {/if}
                  <Button variant="outline" size="sm" disabled={publishing || revoking} onclick={() => load_share(share, true)}>
                    <PencilIcon />
                    Edit
                  </Button>
                </div>
              {/if}
            </div>
            <div class="grid gap-2">
              <span class="text-sm font-medium">Share address</span>
              <div class="flex gap-2">
                <Input value={share.url} readonly aria-label="Share address" class={!is_managed_share_enabled(share) ? "text-muted-foreground" : ""} />
                <Button variant="outline" size="icon" onclick={() => void copy_link()} aria-label="Copy share address" title="Copy share address">
                  <CopyIcon />
                </Button>
              </div>
              <p class="text-xs text-muted-foreground">
                Last published {share_datetime_formatter.format(new Date(share.updated_at))}
                {share.expires_at ? ` · Expires ${share_date_formatter.format(new Date(share.expires_at))}` : " · No expiration"}
              </p>
            </div>
          </section>
        {/if}

        {#if share && !editing}
          <section class="grid gap-3" aria-label="Shared content settings">
            <div>
              <h3 class="text-sm font-semibold">Share settings</h3>
              <p class="text-xs text-muted-foreground">These settings control what visitors can see.</p>
            </div>
            <div class="grid gap-4 rounded-lg border p-4 sm:grid-cols-2">
              <div>
                <div class="text-xs font-medium text-muted-foreground">Board name</div>
                <p class="mt-1 text-sm font-medium">{share.title}</p>
              </div>
              <div>
                <div class="text-xs font-medium text-muted-foreground">Expiration</div>
                <p class="mt-1 text-sm font-medium">{share.expires_at ? share_date_formatter.format(new Date(share.expires_at)) : "No expiration"}</p>
              </div>
              <div>
                <div class="text-xs font-medium text-muted-foreground">Shared content</div>
                <p class="mt-1 text-sm font-medium">{effective_selection.selected_column_ids.length} columns · {effective_selection.selected_task_ids.length} tasks</p>
              </div>
              <div>
                <div class="text-xs font-medium text-muted-foreground">Automatic labels</div>
                {#if selected_labels.length > 0}
                  <div class="mt-1 flex flex-wrap gap-1.5">
                    {#each selected_labels as label (label)}
                      <span class="rounded-md bg-muted px-2 py-0.5 text-xs">{label}</span>
                    {/each}
                  </div>
                {:else}
                  <p class="mt-1 text-sm text-muted-foreground">None</p>
                {/if}
              </div>
            </div>
          </section>
        {/if}

        {#if !share || editing}
        <label class="grid gap-1.5 text-sm font-medium">
        Shared board name
        <Input bind:value={title} maxlength={120} placeholder="Team roadmap" />
        </label>

      {#if !share}
        <div class="grid gap-2">
          <div>
            <div class="text-sm font-medium">Share address</div>
            <p class="text-xs text-muted-foreground">Create a new address, or restore one you shared before.</p>
          </div>
          <div class="grid gap-2 sm:grid-cols-2">
            <button
              type="button"
              class={`flex items-start gap-3 rounded-lg border p-3 text-left transition-colors ${!reuse_link_open ? "border-primary bg-primary/5 ring-1 ring-primary/20" : "hover:bg-muted/40"}`}
              onclick={() => {
                reuse_link_open = false;
                requested_link = "";
              }}
            >
              <CircleCheckIcon class={`mt-0.5 size-4 shrink-0 ${!reuse_link_open ? "text-primary" : "text-muted-foreground"}`} />
              <span>
                <span class="block text-sm font-medium">New address</span>
                <span class="mt-0.5 block text-xs text-muted-foreground">Best for a new audience</span>
              </span>
            </button>
            <button
              type="button"
              class={`flex items-start gap-3 rounded-lg border p-3 text-left transition-colors ${reuse_link_open ? "border-primary bg-primary/5 ring-1 ring-primary/20" : "hover:bg-muted/40"}`}
              onclick={() => reuse_link_open = true}
            >
              <RefreshCwIcon class={`mt-0.5 size-4 shrink-0 ${reuse_link_open ? "text-primary" : "text-muted-foreground"}`} />
              <span>
                <span class="block text-sm font-medium">Reuse previous address</span>
                <span class="mt-0.5 block text-xs text-muted-foreground">Reconnect an old shared URL</span>
              </span>
            </button>
          </div>
          {#if reuse_link_open}
            <div id="reuse-share-link-options" class="grid gap-2 rounded-lg border bg-muted/20 p-3">
              <label for="previous-share-link" class="text-sm font-medium">Previous address or share ID</label>
              <Input
                id="previous-share-link"
                bind:value={requested_link}
                autocomplete="off"
                spellcheck={false}
                placeholder="Paste the link you shared before"
              />
              <p class="text-xs text-muted-foreground">
                Paste the complete old address when possible. Cardbe will restore its share ID and try to use the same port.
              </p>
            </div>
          {/if}
        </div>
      {/if}

      <div class="grid gap-2">
        <div class="flex items-center gap-2">
          <Checkbox
            id="share-expiration-enabled"
            checked={expiration_enabled}
            onCheckedChange={(checked) => {
              expiration_enabled = checked === true;
            }}
          />
          <label for="share-expiration-enabled" class="text-sm font-medium">Set link expiration</label>
        </div>
        {#if expiration_enabled}
          <Popover.Root bind:open={expiration_open}>
            <Popover.Trigger>
              {#snippet child({ props })}
                <Button {...props} variant="outline" class="w-full justify-between font-normal">
                  <span class="flex items-center gap-2">
                    <CalendarDaysIcon class="size-4" />
                    {share_date_formatter.format(expiration_date.toDate(getLocalTimeZone()))}
                  </span>
                  <ChevronDownIcon />
                </Button>
              {/snippet}
            </Popover.Trigger>
            <Popover.Content class="w-auto overflow-hidden p-0" align="start">
              <Calendar
                type="single"
                bind:value={expiration_date}
                minValue={today(getLocalTimeZone())}
                captionLayout="dropdown"
                onValueChange={() => {
                  expiration_open = false;
                }}
              />
            </Popover.Content>
          </Popover.Root>
        {:else}
          <p class="text-xs text-muted-foreground">The link remains active until you disable it.</p>
        {/if}
      </div>

      <div class="grid gap-1.5">
        <div class="text-sm font-medium">Automatically include labels</div>
        <TagInput
          bind:tags={selected_labels}
          suggestions={available_labels}
          placeholder="Add label"
        />
        <p class="text-xs text-muted-foreground">
          Cards with any of these labels are included automatically, including cards created later.
        </p>
      </div>

      <div class="grid gap-2">
        <div class="flex items-center justify-between">
          <div>
            <div class="text-sm font-medium">Content to include</div>
            <div class="text-xs text-muted-foreground">
              {effective_selection.selected_column_ids.length} columns · {effective_selection.selected_task_ids.length} tasks selected
            </div>
          </div>
        </div>
        <div class="relative">
          <SearchIcon class="pointer-events-none absolute left-3 top-2.5 size-4 text-muted-foreground" />
          <Input class="pl-9" bind:value={task_search} placeholder="Search columns, tasks, descriptions, or labels" aria-label="Search shareable tasks" />
        </div>
        <div class="flex items-center justify-between gap-3">
          <span class="text-xs tabular-nums text-muted-foreground">
            {visible_task_ids.length} {task_search.trim() ? "matching" : "available"} tasks
          </span>
          <div class="flex gap-1">
            <Button
              variant="ghost"
              size="sm"
              disabled={visible_task_ids.length === 0 || all_visible_tasks_selected}
              onclick={() => set_visible_tasks_selected(true)}
            >
              Select all
            </Button>
            <Button
              variant="ghost"
              size="sm"
              disabled={visible_task_ids.length === 0 || no_visible_tasks_selected}
              onclick={() => set_visible_tasks_selected(false)}
            >
              Clear all
            </Button>
          </div>
        </div>
        <div class="max-h-72 space-y-2 overflow-y-auto rounded-md border p-2">
          {#each visible_columns as column (column.id)}
            {@const all_column_task_ids = columns.find((candidate) => candidate.id === column.id)?.tasks.map((task) => task.id) ?? []}
            {@const selected_in_column = all_column_task_ids.filter((id) => selected_task_ids.includes(id)).length}
            <div class="rounded-md border bg-muted/20">
              <label class="flex cursor-pointer items-center gap-3 rounded-t-md px-3 py-2 hover:bg-muted/60">
                <Checkbox
                  checked={selected_column_ids.includes(column.id) && selected_in_column === all_column_task_ids.length}
                  indeterminate={selected_column_ids.includes(column.id) && all_column_task_ids.length > 0 && selected_in_column < all_column_task_ids.length}
                  onCheckedChange={(checked) => set_column_selected(column.id, checked === true)}
                />
                <span class="min-w-0 flex-1 truncate text-sm font-medium">{column.name}</span>
                <span class="text-xs tabular-nums text-muted-foreground">
                  {selected_in_column}/{all_column_task_ids.length}
                </span>
              </label>
              {#if column.tasks.length > 0}
                <div class="border-t px-2 py-1">
                  {#each column.tasks as task (task.id)}
                    <label class="flex cursor-pointer items-start gap-3 rounded-md px-2 py-2 hover:bg-muted/60">
                      <Checkbox
                        class="mt-0.5"
                        checked={selected_task_ids.includes(task.id)}
                        onCheckedChange={(checked) => set_task_selected(column.id, task.id, checked === true)}
                      />
                      <span class="min-w-0 flex-1">
                        <span class="block truncate text-sm">{task.title || "Untitled task"}</span>
                        {#if task.labels.length > 0}
                          <span class="block truncate text-xs text-muted-foreground">{task.labels.join(" · ")}</span>
                        {/if}
                      </span>
                    </label>
                  {/each}
                </div>
              {/if}
            </div>
          {:else}
            <p class="p-5 text-center text-sm text-muted-foreground">No matching columns or tasks</p>
          {/each}
        </div>
      </div>

        {/if}

    </div>
    </div>

    <Dialog.Footer class="shrink-0 border-t bg-background px-6 py-4 sm:justify-end">
      <div class="flex flex-wrap justify-end gap-2">
        {#if share}
          <Button
            variant="ghost"
            class="text-destructive hover:bg-destructive/10 hover:text-destructive"
            disabled={publishing || revoking}
            onclick={() => delete_confirm_open = true}
          >
            <Trash2Icon />
            Delete link
          </Button>
        {/if}
        {#if share && editing}
          <Button variant="outline" disabled={publishing || revoking} onclick={cancel_editing}>Cancel</Button>
          <Button disabled={publishing || revoking || effective_selection.selected_column_ids.length === 0} onclick={() => void publish()}>
            <RefreshCwIcon />
            {publishing ? "Saving..." : "Save changes"}
          </Button>
        {:else if !share}
          <Button disabled={publishing || revoking || effective_selection.selected_column_ids.length === 0} onclick={() => void publish()}>
            {publishing ? "Publishing..." : requested_link.trim() ? "Reuse address" : "Create share link"}
          </Button>
        {/if}
      </div>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<AlertDialog.Root bind:open={delete_confirm_open}>
  <AlertDialog.Content class="sm:max-w-md">
    <AlertDialog.Header>
      <AlertDialog.Title>Delete this share link?</AlertDialog.Title>
      <AlertDialog.Description>
        This permanently removes the saved link and disables its address. This action cannot be undone. Use Disable instead if you may want to share it again.
      </AlertDialog.Description>
    </AlertDialog.Header>
    <AlertDialog.Footer>
      <AlertDialog.Cancel disabled={revoking}>Keep link</AlertDialog.Cancel>
      <Button variant="destructive" disabled={revoking} onclick={() => void delete_share()}>
        <Trash2Icon />
        {revoking ? "Deleting..." : "Delete link"}
      </Button>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
