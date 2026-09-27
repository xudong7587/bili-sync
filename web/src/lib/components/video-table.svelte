<script lang="ts">
	import { Checkbox } from '$lib/components/ui/checkbox';
	import { Button } from '$lib/components/ui/button';
	import type { VideoInfo } from '$lib/types';
	import ArrowUpDown from '@lucide/svelte/icons/arrow-up-down';
	import ImageIcon from '@lucide/svelte/icons/image';
	let {
		videos,
		sort,
		onSort,
		source,
		onResetSelected,
		busy = false
	}: {
		videos: VideoInfo[];
		sort: string;
		onSort: (value: string) => void;
		source: (video: VideoInfo) => { name: string } | null;
		onResetSelected: (ids: number[]) => Promise<void>;
		busy?: boolean;
	} = $props();
	let selected = $state<number[]>([]);
	$effect(() => {
		void videos;
		selected = [];
	});
	function toggle(id: number, checked: boolean) {
		selected = checked ? [...selected, id] : selected.filter((v) => v !== id);
	}
	function date(value?: string) {
		return value ? value.replace('T', ' ').slice(0, 16) : '—';
	}
	function status(v: VideoInfo) {
		if (!v.valid) return '失效';
		if (!v.should_download) return '跳过';
		if (v.download_status.length && v.download_status.every((s) => s === 7)) return '已完成';
		if (v.download_status.some((s) => s !== 0 && s !== 7)) return '失败';
		return '等待';
	}
	function percentage(v: VideoInfo) {
		return v.download_status.length
			? Math.round(
					(v.download_status.filter((s) => s === 7).length / v.download_status.length) * 100
				)
			: 0;
	}
	function direction(asc: string, desc: string): 'ascending' | 'descending' | 'none' {
		return sort === asc ? 'ascending' : sort === desc ? 'descending' : 'none';
	}
</script>

<div class="mb-8 overflow-hidden rounded-xl border bg-card">
	{#if selected.length}
		<div class="flex items-center gap-3 border-b bg-primary/5 px-4 py-2 text-sm">
			<span>已选 {selected.length} 个视频（当前页）</span>
			<Button
				variant="outline"
				size="sm"
				disabled={busy}
				onclick={async () => {
					await onResetSelected([...selected]);
					selected = [];
				}}>重试所选失败任务</Button
			>
			<Button variant="ghost" size="sm" disabled={busy} onclick={() => (selected = [])}
				>取消选择</Button
			>
		</div>
	{/if}
	<div class="overflow-x-auto">
		<table class="w-full min-w-[980px] table-fixed text-left text-sm">
			<caption class="sr-only">视频列表，可按标题、入库时间和 UP 主排序</caption>
			<colgroup
				><col class="w-12" /><col class="w-24" /><col class="w-[27%]" /><col class="w-36" /><col
					class="w-40"
				/><col class="w-[14%]" /><col class="w-28" /><col class="w-16" /></colgroup
			>
			<thead class="border-b bg-muted/50 text-xs text-muted-foreground">
				<tr class="h-11">
					<th class="pl-4"
						><Checkbox
							aria-label="选择当前页所有视频"
							checked={selected.length === videos.length && videos.length > 0}
							indeterminate={selected.length > 0 && selected.length < videos.length}
							disabled={busy}
							onCheckedChange={(checked) => (selected = checked ? videos.map((v) => v.id) : [])}
						/></th
					>
					<th class="px-2 font-medium">封面</th>
					<th class="px-3" aria-sort={direction('title', 'title_desc')}
						><button
							class="flex items-center gap-1 py-2"
							onclick={() => onSort(sort === 'title' ? 'title_desc' : 'title')}
							>标题 <ArrowUpDown class="size-3" /></button
						></th
					>
					<th class="px-3" aria-sort={direction('added_asc', 'added_desc')}
						><button
							class="flex items-center gap-1 py-2"
							onclick={() => onSort(sort === 'added_desc' ? 'added_asc' : 'added_desc')}
							>入库时间 <ArrowUpDown class="size-3" /></button
						></th
					>
					<th class="px-3 font-medium">视频数据</th>
					<th class="px-3" aria-sort={direction('upper', 'upper_desc')}
						><button
							class="flex items-center gap-1 py-2"
							onclick={() => onSort(sort === 'upper' ? 'upper_desc' : 'upper')}
							>UP 主 <ArrowUpDown class="size-3" /></button
						></th
					>
					<th class="px-3 font-medium">任务状态</th><th class="px-3 font-medium">操作</th>
				</tr>
			</thead>
			<tbody class="divide-y">
				{#each videos as video (video.id)}
					<tr
						class={`h-[72px] transition-colors hover:bg-muted/40 ${selected.includes(video.id) ? 'bg-primary/5' : ''}`}
					>
						<td class="pl-4"
							><Checkbox
								aria-label={`选择 ${video.name}`}
								checked={selected.includes(video.id)}
								disabled={busy}
								onCheckedChange={(checked) => toggle(video.id, checked)}
							/></td
						>
						<td class="px-2"
							><a
								href={`/video/${video.id}`}
								aria-label={`查看 ${video.name}`}
								class="flex h-10 w-20 items-center justify-center overflow-hidden rounded bg-muted"
								>{#if video.cover}<img
										src={video.cover}
										alt=""
										referrerpolicy="no-referrer"
										loading="lazy"
										class="h-full w-full object-cover"
									/>{:else}<ImageIcon class="size-4 text-muted-foreground" />{/if}</a
							></td
						>
						<td class="px-3"
							><a
								class="block truncate font-medium hover:text-primary"
								title={video.name}
								href={`/video/${video.id}`}>{video.name}</a
							><span
								class="mt-1 block truncate text-xs text-muted-foreground"
								title={source(video)?.name}>{source(video)?.name || '—'}</span
							></td
						>
						<td class="px-3 text-xs tabular-nums" title="首次加入 bili-sync 数据库的时间"
							>{date(video.created_at)}</td
						>
						<td class="px-3"
							><a
								class="text-xs hover:text-primary"
								href={`https://www.bilibili.com/video/${video.bvid}`}
								target="_blank"
								rel="noreferrer">{video.bvid}</a
							><span class="mt-1 block text-[11px] text-muted-foreground"
								>收藏 {date(video.favtime)}</span
							></td
						>
						<td class="truncate px-3" title={video.upper_name}>{video.upper_name}</td>
						<td class="px-3"
							><span class:text-destructive={status(video) === '失败'} class="text-xs"
								>{status(video)}</span
							><span
								class="ml-2 text-xs tabular-nums text-muted-foreground"
								title="处理步骤完成比例">{percentage(video)}%</span
							></td
						>
						<td class="px-3"
							><a class="text-xs text-primary hover:underline" href={`/video/${video.id}`}>详情</a
							></td
						>
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
</div>
