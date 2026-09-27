<script lang="ts">
	let {
		data,
		second = [],
		ceiling,
		label,
		unit = ''
	}: {
		data: number[];
		second?: number[];
		ceiling?: number;
		label: string;
		unit?: string;
	} = $props();
	const max = $derived(ceiling || Math.max(1, ...data, ...second));
	const points = (values: number[]) =>
		values
			.map(
				(v, i) =>
					`${(i * 600) / Math.max(1, values.length - 1)},${110 - (Math.min(max, v) / max) * 100}`
			)
			.join(' ');
</script>

<div class="relative pt-5">
	<span class="text-muted-foreground absolute right-0 top-0 text-[10px] tabular-nums"
		>{max.toFixed(1)} {unit}</span
	>
	<svg
		viewBox="0 0 600 120"
		preserveAspectRatio="none"
		class="h-32 w-full"
		role="img"
		aria-label={label}
	>
		<path
			d="M0 10H600 M0 60H600 M0 110H600"
			fill="none"
			stroke="currentColor"
			class="text-border"
			stroke-dasharray="3 4"
		/>
		{#if data.length > 1}<polyline
				points={points(data)}
				fill="none"
				stroke="var(--primary)"
				stroke-width="2.5"
				vector-effect="non-scaling-stroke"
			/>{/if}
		{#if second.length > 1}<polyline
				points={points(second)}
				fill="none"
				stroke="#f59e0b"
				stroke-width="2"
				vector-effect="non-scaling-stroke"
			/>{/if}
	</svg>
	<div class="text-muted-foreground flex justify-between text-[10px]">
		<span>最近 {Math.max(0, data.length - 1) * 2} 秒</span><span>现在</span>
	</div>
</div>
