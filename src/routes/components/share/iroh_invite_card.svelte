<script lang="ts">
  import { formatNumber, getLocale } from "$lib/i18n";
  import * as m from "$lib/paraglide/messages.js";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import ShieldCheckIcon from "@lucide/svelte/icons/shield-check";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as Card from "$lib/components/ui/card/index.js";
  import * as Collapsible from "$lib/components/ui/collapsible/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import * as Select from "$lib/components/ui/select/index.js";
  import { Switch } from "$lib/components/ui/switch/index.js";
  import {
    IrohDeviceStatus,
    type IrohConnectionDetails,
    type IrohInvite,
  } from "../../type/iroh-share";
  import IrohConnectionInfo from "./iroh_connection_info.svelte";

  let {
    invite,
    updating,
    loading_access,
    qr_visible,
    qr_svg,
    devices_open,
    device_connections,
    onupdate,
    oncopy,
    ontoggleqr,
    ondelete,
    ondevicesopenchange,
    onreview,
    onrevoke,
  }: {
    invite: IrohInvite;
    updating: boolean;
    loading_access: boolean;
    qr_visible: boolean;
    qr_svg: string | undefined;
    devices_open: boolean | undefined;
    device_connections: IrohConnectionDetails["devices"];
    onupdate: (change: {
      permission?: "viewer" | "editor";
      enabled?: boolean;
    }) => void;
    oncopy: () => void;
    ontoggleqr: () => void;
    ondelete: () => void;
    ondevicesopenchange: (open: boolean) => void;
    onreview: (node_id: string) => void;
    onrevoke: (node_id: string) => void;
  } = $props();
  const sync_time = $derived(
    new Intl.DateTimeFormat(getLocale(), {
      dateStyle: "medium",
      timeStyle: "short",
    }),
  );
  function invitation_date(value: string) {
    // SQLite datetime('now') is UTC but omits the timezone.
    const date = new Date(
      /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/.test(value)
        ? `${value.replace(" ", "T")}Z`
        : value,
    );
    return Number.isNaN(date.getTime())
      ? m.share_creation_date_unavailable()
      : m.share_created_date({ date: sync_time.format(date) });
  }
</script>

<Card.Root class="gap-3 p-4 shadow-none">
  <div class="flex flex-wrap items-center justify-between gap-2">
    <div>
      <Card.Title class="text-base">{invite.board_name}</Card.Title
      ><Card.Description>{invitation_date(invite.created_at)}</Card.Description>
    </div>
    <Badge variant={invite.enabled ? "secondary" : "outline"}
      >{invite.enabled ? m.explorer_active() : m.ui_disabled()}</Badge
    >
  </div>
  <Collapsible.Root class="rounded-md border">
    <Collapsible.Trigger
      class="group flex w-full items-center justify-between gap-2 px-3 py-2 text-sm hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      <span
        >{m.share_invitation_settings({
          permission:
            invite.permission === "viewer"
              ? m.board_read_only()
              : m.board_can_edit(),
        })}</span
      >
      <ChevronDownIcon
        class="size-4 text-muted-foreground transition-transform group-data-[state=open]:rotate-180"
      />
    </Collapsible.Trigger>
    <Collapsible.Content class="space-y-2 border-t p-3">
      <div class="flex flex-wrap items-center gap-3">
        <Select.Root
          type="single"
          disabled={updating}
          value={invite.permission}
          onValueChange={(v) =>
            onupdate({
              permission: v as "viewer" | "editor",
            })}
        >
          <Select.Trigger
            class="w-36"
            aria-label={m.share_permission_for({
              name: invite.board_name,
            })}
            >{invite.permission === "viewer"
              ? m.board_read_only()
              : m.board_can_edit()}</Select.Trigger
          >
          <Select.Content
            ><Select.Item value="viewer">{m.board_read_only()}</Select.Item
            ><Select.Item value="editor">{m.board_can_edit()}</Select.Item
            ></Select.Content
          >
        </Select.Root>
        <div class="flex items-center gap-2 px-1">
          <Switch
            id={`invite-active-${invite.invite_id}`}
            disabled={updating}
            checked={invite.enabled}
            onCheckedChange={(checked) => onupdate({ enabled: checked })}
          />
          <Label for={`invite-active-${invite.invite_id}`}
            >{m.share_enabled()}</Label
          >
        </div>
      </div>
      <p class="text-xs text-muted-foreground">
        {m.share_settings_help()}
      </p>
    </Collapsible.Content>
  </Collapsible.Root>
  <div class="flex flex-wrap gap-2">
    <Button
      variant="outline"
      disabled={!invite.enabled || loading_access}
      onclick={oncopy}>{m.ui_copy_link()}</Button
    >
    <Button
      variant="outline"
      disabled={!invite.enabled || loading_access}
      onclick={ontoggleqr}
      >{qr_visible ? m.ui_hide_qr() : m.ui_show_qr()}</Button
    >
    <Button class="ml-auto" variant="destructive" onclick={ondelete}
      >{m.common_delete()}</Button
    >
  </div>
  {#if invite.devices.length > 0}
    <Collapsible.Root
      open={devices_open ??
        invite.devices.some(
          (device) => device.status === IrohDeviceStatus.Pending,
        )}
      onOpenChange={ondevicesopenchange}
      class="rounded-lg border bg-background"
    >
      <Collapsible.Trigger
        class="group flex w-full items-center gap-2 rounded-lg px-3 py-2.5 text-left text-sm font-medium transition-colors hover:bg-accent/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        <ShieldCheckIcon class="size-4 text-muted-foreground" />
        <span class="flex-1"
          >{m.ui_devices()}
          <span class="text-muted-foreground"
            >({formatNumber(invite.devices.length)})</span
          ></span
        >
        {#if invite.devices.some((device) => device.status === IrohDeviceStatus.Pending)}
          <Badge variant="secondary"
            >{m.share_pending_count({
              count: invite.devices.filter(
                (device) => device.status === IrohDeviceStatus.Pending,
              ).length,
            })}</Badge
          >
        {/if}
        <ChevronDownIcon
          class="size-4 text-muted-foreground transition-transform group-data-[state=open]:rotate-180"
        />
      </Collapsible.Trigger>
      <Collapsible.Content class="border-t">
        <div class="space-y-2 p-2">
          {#each invite.devices as device}
            <div class="rounded-lg border bg-muted/20 p-3 text-sm">
              <div class="flex items-center gap-3">
                <span
                  class="flex size-9 shrink-0 items-center justify-center rounded-full bg-background text-primary"
                  ><ShieldCheckIcon class="size-4" /></span
                >
                <div class="min-w-0 flex-1">
                  <div class="font-medium">
                    {device.status === IrohDeviceStatus.Pending
                      ? m.ui_approval_requested()
                      : device.status === IrohDeviceStatus.Approved
                        ? m.ui_approved_device()
                        : m.share_access_revoked()}
                  </div>
                  <div class="font-mono text-xs text-muted-foreground">
                    {device.node_id.slice(0, 8)}…{device.node_id.slice(-8)}
                  </div>
                </div>
                {#if device.status === IrohDeviceStatus.Pending}
                  <Button
                    size="sm"
                    onclick={() => onreview(device.node_id)}
                    aria-haspopup="dialog">{m.ui_review()}</Button
                  >
                {:else if device.status === IrohDeviceStatus.Approved}
                  <Button
                    variant="outline"
                    size="sm"
                    onclick={() => onrevoke(device.node_id)}
                    >{m.ui_revoke()}</Button
                  >
                {/if}
              </div>
              <div class="mt-2">
                <IrohConnectionInfo info={device_connections[device.node_id]} />
              </div>
            </div>
          {/each}
        </div>
      </Collapsible.Content>
    </Collapsible.Root>
  {/if}
  {#if qr_svg}
    <div
      class="qr mx-auto aspect-square w-full max-w-56 overflow-hidden rounded-md bg-white p-2"
      aria-label={m.ui_board_invitation_qr_code()}
    >
      {@html qr_svg}
    </div>
  {/if}
</Card.Root>
