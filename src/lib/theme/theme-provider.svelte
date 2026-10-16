<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { ModeWatcher, mode, setTheme } from "mode-watcher";
  import { initializeTheme, themeState } from "./theme-manager.svelte";
  import { resolveTheme } from "./themes";
  import { syncWindowChrome } from "./window-chrome";

  onMount(initializeTheme);
  $effect(() => {
    const preference = themeState.preference;
    const theme = resolveTheme(preference, mode.current ?? "light");
    // The persistence setter reads its own state; keep it out of this effect.
    untrack(() => {
      setTheme(theme);
      syncWindowChrome(preference);
    });
  });
</script>

<ModeWatcher disableHeadScriptInjection synchronousModeChanges />
