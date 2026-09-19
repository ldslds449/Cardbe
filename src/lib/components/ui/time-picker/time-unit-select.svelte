<script lang="ts" module>
	import type { Time as TimeType } from '@internationalized/date';
	import type { TimePickerType } from './time-picker-utils';
	import type { WithElementRef } from 'bits-ui';
	import type { HTMLButtonAttributes } from 'svelte/elements';

	export type TimeUnitSelectProps = WithElementRef<HTMLButtonAttributes> & {
		time: TimeType | undefined;
		picker: Extract<TimePickerType, 'hours' | 'minutes'>;
		step?: number;
		setTime?: (time: TimeType) => void;
		onRightFocus?: () => void;
		onLeftFocus?: () => void;
	};
</script>

<script lang="ts">
	import { Time } from '@internationalized/date';
	import * as Select from '$lib/components/ui/select';
	import { getDateByType, setDateByType } from './time-picker-utils';

	let {
		time = $bindable(new Time(0, 0)),
		picker,
		step = 1,
		setTime,
		onLeftFocus,
		onRightFocus,
		ref = $bindable(null)
	}: TimeUnitSelectProps = $props();

	let limit = $derived(picker === 'hours' ? 24 : 60);
	let validStep = $derived(Math.min(Math.max(Math.trunc(step), 1), limit - 1));
	let options = $derived(
		Array.from(
			{ length: Math.ceil(limit / validStep) },
			(_, index) => String(index * validStep).padStart(2, '0')
		)
	);
	let value = $derived(getDateByType(time, picker));

	function handleValueChange(selection: string) {
		const nextTime = setDateByType(time.copy(), selection, picker, undefined, validStep);
		time = nextTime;
		setTime?.(nextTime);
	}

	function handleKeyDown(event: KeyboardEvent) {
		if (event.key === 'ArrowRight') onRightFocus?.();
		if (event.key === 'ArrowLeft') onLeftFocus?.();
	}
</script>

<Select.Root type="single" {value} onValueChange={handleValueChange}>
	<Select.Trigger
		bind:ref
		class="w-[64px] font-mono text-base tabular-nums focus:bg-accent focus:text-accent-foreground"
		onkeydown={handleKeyDown}
		aria-label={picker === 'hours' ? 'Hours' : 'Minutes'}
	>
		{value}
	</Select.Trigger>
	<Select.Content>
		{#each options as option}
			<Select.Item value={option}>{option}</Select.Item>
		{/each}
	</Select.Content>
</Select.Root>
