<script lang="ts">
  import { LinkPreview as HoverCard } from "bits-ui";
  import { isTauri } from "@tauri-apps/api/core";
  import type { Snippet } from "svelte";
  import type { HTMLAnchorAttributes } from "svelte/elements";
  import GlobeIcon from "@lucide/svelte/icons/globe";
  import {
    cachedPreview,
    fetchPreview,
    fetchPreviewImage,
    type LinkPreview,
  } from "$lib/link-preview";
  import {
    parseCommandError,
    translateCommandError,
    type CommandError,
  } from "$lib/command-errors";
  import * as m from "$lib/paraglide/messages";

  let {
    href,
    children,
    ...rest
  }: HTMLAnchorAttributes & { children?: Snippet } = $props();
  let open = $state(false);
  let preview = $state<LinkPreview>();
  let error = $state<CommandError | null>(null);
  let failed = $state(false);
  let image = $state<string | null>(null);
  let favicon = $state<string | null>(null);
  const desktop = isTauri();

  $effect(() => {
    const url = href;
    if (!open || !url || !desktop) {
      return;
    }
    let active = true;
    preview = cachedPreview(url);
    error = null;
    failed = false;
    image = null;
    favicon = null;
    void fetchPreview(url)
      .then(async (result) => {
        if (!active) {
          return;
        }
        preview = result;
        await Promise.all([
          result.image_url
            ? fetchPreviewImage(result.image_url).then((value) => {
                if (active) {
                  image = value;
                }
              })
            : undefined,
          result.favicon_url
            ? fetchPreviewImage(result.favicon_url).then((value) => {
                if (active) {
                  favicon = value;
                }
              })
            : undefined,
        ]);
      })
      .catch((value: unknown) => {
        if (active) {
          error = parseCommandError(value);
          failed = true;
        }
      });
    return () => {
      active = false;
    };
  });
</script>

<HoverCard.Root
  bind:open
  openDelay={300}
  closeDelay={100}
  disabled={!desktop || !href}
>
  <HoverCard.Trigger>
    {#snippet child({ props })}
      <a {...props} {...rest} {href} role={undefined}>{@render children?.()}</a>
    {/snippet}
  </HoverCard.Trigger>
  <HoverCard.Portal>
    <HoverCard.Content
      sideOffset={6}
      collisionPadding={12}
      class="bg-popover text-popover-foreground z-[100] w-80 max-w-[calc(100vw-2rem)] overflow-hidden rounded-md border shadow-md outline-hidden"
    >
      {#if image}
        <img
          src={image}
          alt=""
          class="h-36 w-full object-cover"
          onerror={() => {
            image = null;
          }}
        />
      {/if}
      <div class="space-y-2 p-4 text-sm">
        {#if failed}
          <p role="status">{translateCommandError(error)}</p>
        {:else if preview}
          <div class="flex items-center gap-2">
            {#if favicon}
              <img
                src={favicon}
                alt=""
                class="size-4 shrink-0 object-contain"
                onerror={() => {
                  favicon = null;
                }}
              />
            {:else}
              <GlobeIcon class="size-4 shrink-0 text-muted-foreground" />
            {/if}
            <p class="line-clamp-2 min-w-0 font-semibold">
              {preview.title ?? preview.domain}
            </p>
          </div>
          {#if preview.status === "unavailable"}
            <p class="text-muted-foreground">{m.link_preview_unavailable()}</p>
          {:else if preview.description}
            <p class="line-clamp-3 text-muted-foreground">
              {preview.description}
            </p>
          {/if}
          <p class="truncate text-xs text-muted-foreground">{preview.domain}</p>
          <p class="line-clamp-2 break-all text-xs text-muted-foreground">
            {href}
          </p>
        {:else}
          <div
            role="status"
            aria-label={m.link_preview_loading()}
            class="space-y-2 animate-pulse"
          >
            <div class="h-4 w-3/4 rounded bg-muted"></div>
            <div class="h-3 rounded bg-muted"></div>
            <div class="h-3 w-1/2 rounded bg-muted"></div>
          </div>
        {/if}
      </div>
    </HoverCard.Content>
  </HoverCard.Portal>
</HoverCard.Root>
