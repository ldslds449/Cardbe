<script lang="ts">
  import * as m from "$lib/paraglide/messages.js";
  import { cn } from "$lib/utils.js";
  import Loader2Icon from "@lucide/svelte/icons/loader-2";
  import type { ComponentProps } from "svelte";

  type Props = ComponentProps<typeof Loader2Icon>;

  let { class: className, ...restProps }: Props = $props();
</script>

<Loader2Icon
  role="status"
  aria-label={m.ui_loading()}
  class={cn("size-4 animate-spin", className)}
  {...restProps}
/>
