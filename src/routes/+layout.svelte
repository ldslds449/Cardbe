<script lang="ts">
import { dev } from "$app/environment";
import "../app.css";
import Sonner from "$lib/components/ui/sonner/sonner.svelte";
import { onMount } from "svelte";

let { children } = $props();

onMount(() => {
  const dismiss_startup = () =>
    document.getElementById("app-startup")?.classList.add("is-ready");
  const waits_for_workspace = window.location.pathname === "/";
  if (waits_for_workspace) {
    window.addEventListener("cardbe:workspace-ready", dismiss_startup, {
      once: true,
    });
  } else {
    dismiss_startup();
  }

  if (dev) {
    return () =>
      window.removeEventListener("cardbe:workspace-ready", dismiss_startup);
  }

  const preventBrowserContextMenu = (event: MouseEvent) =>
    event.preventDefault();
  document.addEventListener("contextmenu", preventBrowserContextMenu);

  return () => {
    window.removeEventListener("cardbe:workspace-ready", dismiss_startup);
    document.removeEventListener("contextmenu", preventBrowserContextMenu);
  };
});
</script>

{@render children()}
<Sonner expand closeButton closeButtonAriaLabel="Close notification" />
