<script lang="ts">
    import ShapesIcon from "@lucide/svelte/icons/shapes";
    import UserRoundIcon from "@lucide/svelte/icons/user-round";
    import CircleDotDashedIcon from "@lucide/svelte/icons/circle-dot-dashed";
    import GaugeIcon from "@lucide/svelte/icons/gauge";
    import WeightIcon from "@lucide/svelte/icons/weight";

    import { Badge } from "$lib/components/ui/badge/index.js";
    import { cn } from "$lib/utils.js";
    import { parse_label } from "../utils/label";

    let {
        label,
        class: className,
    }: {
        label: string;
        class?: string;
    } = $props();

    let parsed = $derived(parse_label(label));
    let kind_name = $derived(
        parsed.kind === "category"
            ? "Category"
            : parsed.kind === "general"
              ? null
              : parsed.kind.charAt(0).toUpperCase() + parsed.kind.slice(1),
    );
    let accessible_label = $derived(
        kind_name ? `${kind_name}: ${parsed.value}` : parsed.value,
    );
</script>

<Badge
    variant="secondary"
    class={cn(
        "max-w-full",
        className,
    )}
    title={accessible_label}
    aria-label={accessible_label}
>
    {#if parsed.kind === "owner"}
        <UserRoundIcon />
    {:else if parsed.kind === "category"}
        <ShapesIcon />
    {:else if parsed.kind === "priority"}
        <GaugeIcon />
    {:else if parsed.kind === "status"}
        <CircleDotDashedIcon />
    {:else if parsed.kind === "effort"}
        <WeightIcon />
    {/if}
    <span class="overflow-hidden text-ellipsis">{parsed.value}</span>
</Badge>
