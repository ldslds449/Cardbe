<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { ModeWatcher, mode, setTheme } from "mode-watcher";
  import { initializeTheme, themeState } from "./theme-manager.svelte";
  import { resolveTheme } from "./themes";

  onMount(initializeTheme);
  $effect(() => {
    const theme = resolveTheme(themeState.preference, mode.current ?? "light");
    // The persistence setter reads its own state; keep it out of this effect.
    untrack(() => setTheme(theme));
  });
</script>

<ModeWatcher disableHeadScriptInjection synchronousModeChanges />
