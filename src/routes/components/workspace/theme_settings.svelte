<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import { m } from "$lib/paraglide/messages";
  import { themes } from "$lib/theme/themes";
  import { selectTheme, themeState } from "$lib/theme/theme-manager.svelte";
  import CheckIcon from "@lucide/svelte/icons/check";
</script>

<fieldset
  class="space-y-3"
  disabled={!themeState.ready}
  aria-busy={themeState.saving}
>
  <legend class="text-sm font-medium">{m.theme_title()}</legend>
  <p class="text-sm text-muted-foreground">{m.theme_description()}</p>
  <Button
    variant="outline"
    aria-pressed={themeState.preference === "system"}
    onclick={() => void selectTheme("system")}
    class="w-full justify-between"
  >
    {m.theme_system()}
    {#if themeState.preference === "system"}<CheckIcon
        aria-hidden="true"
      />{/if}
  </Button>
  <div class="grid grid-cols-2 gap-2">
    {#each themes as theme (theme.id)}
      <Button
        variant="outline"
        class="h-auto flex-col items-stretch gap-2 p-2"
        aria-pressed={themeState.preference === theme.id}
        onclick={() => void selectTheme(theme.id)}
      >
        <span
          class="theme-preview flex h-8 items-center gap-1 rounded p-1"
          data-theme={theme.id}
          style="background:var(--background)"
          aria-hidden="true"
        >
          <span class="h-full flex-1 rounded-sm" style="background:var(--card)"
          ></span>
          <span class="h-full w-5 rounded-sm" style="background:var(--primary)"
          ></span>
        </span>
        <span class="flex items-center justify-between gap-1 text-xs">
          {theme.name()}
          {#if themeState.preference === theme.id}<CheckIcon
              class="size-3"
              aria-hidden="true"
            />{/if}
        </span>
      </Button>
    {/each}
  </div>
</fieldset>
