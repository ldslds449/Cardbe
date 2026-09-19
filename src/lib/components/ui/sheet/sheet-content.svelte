<script lang="ts" module>
	import { tv, type VariantProps } from "tailwind-variants";
	export const sheetVariants = tv({
		base: "bg-background data-[state=open]:animate-in data-[state=closed]:animate-out fixed z-50 flex flex-col gap-4 shadow-lg transition ease-in-out data-[state=closed]:duration-300 data-[state=open]:duration-500",
		variants: {
			side: {
				top: "data-[state=closed]:slide-out-to-top data-[state=open]:slide-in-from-top inset-x-0 top-0 h-auto border-b",
				bottom: "data-[state=closed]:slide-out-to-bottom data-[state=open]:slide-in-from-bottom inset-x-0 bottom-0 h-auto border-t",
				left: "data-[state=closed]:slide-out-to-start data-[state=open]:slide-in-from-start inset-y-0 start-0 h-full border-e",
				right: "data-[state=closed]:slide-out-to-end data-[state=open]:slide-in-from-end inset-y-0 end-0 h-full border-s",
			},
			resizable: {
				true: "",
				false: "",
			},
		},
		compoundVariants: [
			{ side: "left", resizable: false, class: "w-3/4 sm:max-w-sm" },
			{ side: "right", resizable: false, class: "w-3/4 sm:max-w-sm" },
		],
		defaultVariants: {
			side: "right",
			resizable: false,
		},
	});

	export type Side = VariantProps<typeof sheetVariants>["side"];
</script>

<script lang="ts">
	import { Dialog as SheetPrimitive } from "bits-ui";
	import XIcon from "@lucide/svelte/icons/x";
	import GripVerticalIcon from "@lucide/svelte/icons/grip-vertical";
	import { onMount } from "svelte";
	import type { Snippet } from "svelte";
	import SheetPortal from "./sheet-portal.svelte";
	import SheetOverlay from "./sheet-overlay.svelte";
	import { cn, type WithoutChildrenOrChild } from "$lib/utils.js";
	import type { ComponentProps } from "svelte";

	let {
		ref = $bindable(null),
		class: className,
		side = "right",
		portalProps,
		children,
		resizable = false,
		defaultWidth = 384,
		minWidth = 320,
		maxWidth = 960,
		widthStorageKey,
		style,
		...restProps
	}: WithoutChildrenOrChild<SheetPrimitive.ContentProps> & {
		portalProps?: WithoutChildrenOrChild<ComponentProps<typeof SheetPortal>>;
		side?: Side;
		children: Snippet;
		resizable?: boolean;
		defaultWidth?: number;
		minWidth?: number;
		maxWidth?: number;
		widthStorageKey?: string;
	} = $props();

	let panelWidth = $state(0);
	let resizing = $state(false);
	let resizeStartX = 0;
	let resizeStartWidth = 0;
	let contentElement = $state<HTMLElement | null>(null);
	let activeContentElement: HTMLElement | null = null;
	let persistTimer: ReturnType<typeof setTimeout> | null = null;

	const canResize = $derived(resizable && (side === "left" || side === "right"));
	const currentWidth = $derived(panelWidth || defaultWidth);
	const contentStyle = $derived(
		canResize ? `${style ?? ""}; --sheet-width: ${currentWidth}px;` : style,
	);

	$effect(() => {
		ref = contentElement;
		if (contentElement && canResize) {
			contentElement.style.setProperty("--sheet-width", `${currentWidth}px`);
		} else if (!contentElement && resizing) {
			stopResize();
		}
	});

	function clampConfiguredWidth(width: number) {
		return Math.min(maxWidth, Math.max(minWidth, width));
	}

	function clampDragWidth(width: number) {
		const viewportMaximum = Math.max(minWidth, window.innerWidth - 48);
		const effectiveMaximum = Math.min(maxWidth, viewportMaximum);
		return Math.min(effectiveMaximum, Math.max(minWidth, width));
	}

	function persistWidth(width: number) {
		if (!widthStorageKey) return;
		try {
			window.localStorage.setItem(widthStorageKey, String(width));
		} catch {
			// Resizing should still work when storage is unavailable.
		}
	}

	function schedulePersistWidth(width: number) {
		if (persistTimer) clearTimeout(persistTimer);
		persistTimer = setTimeout(() => {
			persistTimer = null;
			persistWidth(width);
		}, 120);
	}

	function flushPersistWidth(width: number) {
		if (persistTimer) {
			clearTimeout(persistTimer);
			persistTimer = null;
		}
		persistWidth(width);
	}

	onMount(() => {
		if (!canResize) return;
		panelWidth = clampConfiguredWidth(defaultWidth);
		if (widthStorageKey) {
			try {
				const storedWidth = Number(window.localStorage.getItem(widthStorageKey));
				if (Number.isFinite(storedWidth) && storedWidth > 0) {
					panelWidth = clampConfiguredWidth(storedWidth);
				}
			} catch {
				// Keep the default width when storage is unavailable.
			}
		}

		const persistBeforePageExit = () => {
			if (panelWidth > 0) flushPersistWidth(panelWidth);
		};
		window.addEventListener("pagehide", persistBeforePageExit);

		return () => {
			removeResizeListeners();
			if (panelWidth > 0) flushPersistWidth(panelWidth);
			activeContentElement = null;
			window.removeEventListener("pagehide", persistBeforePageExit);
			document.documentElement.removeAttribute("data-panel-resizing");
		};
	});

	function removeResizeListeners() {
		window.removeEventListener("mousemove", resize);
		window.removeEventListener("mouseup", stopResize);
		window.removeEventListener("blur", stopResize);
	}

	function startResize(event: MouseEvent) {
		if (!canResize || event.button !== 0) return;
		const handle = event.currentTarget as HTMLElement;
		const content = contentElement ?? handle.parentElement;
		if (!(content instanceof HTMLElement)) return;

		if (resizing) stopResize();
		activeContentElement = content;
		resizeStartX = event.clientX;
		resizeStartWidth = content.getBoundingClientRect().width;
		panelWidth = resizeStartWidth;
		content.style.setProperty("--sheet-width", `${resizeStartWidth}px`);
		resizing = true;
		document.documentElement.setAttribute("data-panel-resizing", "");
		window.addEventListener("mousemove", resize);
		window.addEventListener("mouseup", stopResize);
		window.addEventListener("blur", stopResize);
		event.preventDefault();
		event.stopPropagation();
	}

	function resize(event: MouseEvent) {
		if (!resizing || !activeContentElement) return;
		if ((event.buttons & 1) === 0) {
			stopResize();
			return;
		}
		const delta = side === "right" ? resizeStartX - event.clientX : event.clientX - resizeStartX;
		panelWidth = clampDragWidth(resizeStartWidth + delta);
		activeContentElement.style.setProperty("--sheet-width", `${panelWidth}px`);
		schedulePersistWidth(panelWidth);
	}

	function stopResize() {
		if (!resizing) return;
		resizing = false;
		removeResizeListeners();
		document.documentElement.removeAttribute("data-panel-resizing");
		flushPersistWidth(panelWidth);
		activeContentElement = null;
	}

	function setPanelWidth(width: number) {
		panelWidth = clampDragWidth(width);
		contentElement?.style.setProperty("--sheet-width", `${panelWidth}px`);
		flushPersistWidth(panelWidth);
	}

	function resizeWithKeyboard(event: KeyboardEvent) {
		if (!canResize) return;
		const step = event.shiftKey ? 48 : 16;
		let nextWidth = currentWidth;

		if (event.key === "Home") nextWidth = minWidth;
		else if (event.key === "End") nextWidth = maxWidth;
		else if (event.key === "ArrowLeft") nextWidth += side === "right" ? step : -step;
		else if (event.key === "ArrowRight") nextWidth += side === "right" ? -step : step;
		else return;

		event.preventDefault();
		setPanelWidth(nextWidth);
	}

	function resetWidth() {
		setPanelWidth(defaultWidth);
	}
</script>

<SheetPortal {...portalProps}>
	<SheetOverlay />
	<SheetPrimitive.Content
		bind:ref={contentElement}
		data-slot="sheet-content"
		class={cn(sheetVariants({ side, resizable: canResize }), canResize && "resizable-sheet", className)}
		style={contentStyle}
		{...restProps}
	>
		{#if canResize}
			<button
				type="button"
				class={cn("resize-handle", side === "left" && "resize-handle-end")}
				class:resize-handle-active={resizing}
				aria-label="Resize panel"
				title="Drag to resize. Double-click to reset."
				onmousedown={startResize}
				onkeydown={resizeWithKeyboard}
				ondblclick={resetWidth}
			>
				<GripVerticalIcon class="size-3.5" />
			</button>
		{/if}
		{@render children?.()}
		<SheetPrimitive.Close
			class="ring-offset-background focus-visible:ring-ring absolute end-4 top-4 rounded-xs opacity-70 transition-opacity hover:opacity-100 focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:outline-hidden disabled:pointer-events-none"
		>
			<XIcon class="size-4" />
			<span class="sr-only">Close</span>
		</SheetPrimitive.Close>
	</SheetPrimitive.Content>
</SheetPortal>

<style>
	:global(html[data-panel-resizing]) {
		cursor: ew-resize;
		user-select: none;
	}

	:global([data-slot="sheet-content"].resizable-sheet) {
		width: min(var(--sheet-width), calc(100vw - 3rem));
		max-width: none;
	}

	.resize-handle {
		position: absolute;
		inset-block: 0;
		inset-inline-start: -0.375rem;
		z-index: 10;
		display: flex;
		width: 0.75rem;
		padding: 0;
		border: 0;
		background: transparent;
		cursor: ew-resize;
		align-items: center;
		justify-content: center;
		color: var(--muted-foreground);
		outline: none;
	}

	.resize-handle::before {
		position: absolute;
		inset-block: 0;
		inset-inline-start: 50%;
		width: 1px;
		content: "";
		background: transparent;
		transition: background-color 150ms ease, width 150ms ease;
	}

	.resize-handle :global(svg) {
		position: relative;
		z-index: 1;
		border-radius: 9999px;
		background: var(--background);
		opacity: 0;
		transition: opacity 150ms ease;
	}

	.resize-handle:hover::before,
	.resize-handle:focus-visible::before,
	.resize-handle-active::before {
		width: 2px;
		background: var(--ring);
	}

	.resize-handle:hover :global(svg),
	.resize-handle:focus-visible :global(svg),
	.resize-handle-active :global(svg) {
		opacity: 1;
	}

	.resize-handle-end {
		inset-inline-start: auto;
		inset-inline-end: -0.375rem;
	}

	@media (max-width: 639px) {
		:global([data-slot="sheet-content"].resizable-sheet) {
			width: 100%;
		}

		.resize-handle {
			display: none;
		}
	}
</style>
