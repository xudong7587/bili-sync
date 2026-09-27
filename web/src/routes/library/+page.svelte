<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { page } from '$app/stores';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Checkbox } from '$lib/components/ui/checkbox';
	import api from '$lib/api';
	import { toast } from 'svelte-sonner';
	import { setBreadcrumb } from '$lib/stores/breadcrumb';
	import type { LibraryRow, LibraryJob, SavedQuality, ApiError } from '$lib/types';
	let rows: LibraryRow[] = [];
	let total = 0;
	let index = 0;
	let query = '';
	let loading = false;
	let selected: number[] = [];
	let storage = 'all';
	let download = 'all';
	let upgradeOnly = false;
	let job: LibraryJob | null = null;
	let busy = false;
	let timer: ReturnType<typeof setTimeout> | undefined;
	let disposed = false;
	$: visible = rows.filter(
		(row) =>
			(storage === 'all' || row.storage === storage) &&
			(download === 'all' || row.downloaded === (download === 'yes')) &&
			(!upgradeOnly || row.comparison?.upgradeable)
	);
	const label = (q: SavedQuality | null | undefined) =>
		q?.height && q?.width
			? `${q.width} × ${q.height}${q.codec ? ` · ${q.codec.toUpperCase()}` : ''}`
			: q?.qn
				? `QN ${q.qn}`
				: '尚无画质记录';
	async function load() {
		loading = true;
		try {
			const result = await api.libraryVideos({
				page: index,
				query,
				favorite: $page.url.searchParams.get('favorite'),
				submission: $page.url.searchParams.get('submission'),
				collection: $page.url.searchParams.get('collection')
			});
			rows = result.data.rows;
			total = result.data.total;
			selected = [];
		} catch (e) {
			toast.error('加载失败', { description: (e as ApiError).message });
		} finally {
			loading = false;
		}
	}
	function select(id: number, checked: boolean) {
		if (checked && selected.length >= 25) {
			toast.info('每批最多选择 25 个分 P');
			return;
		}
		selected = checked
			? [...selected.filter((value) => value !== id), id]
			: selected.filter((value) => value !== id);
	}
	async function poll() {
		try {
			job = (await api.libraryJob()).data;
			if (disposed) return;
			if (job.running) timer = setTimeout(poll, 2000);
			else if (busy) {
				busy = false;
				await load();
			}
		} catch (e) {
			busy = false;
			toast.error('读取任务进度失败', { description: (e as ApiError).message });
		}
	}
	async function run(action: string) {
		busy = true;
		try {
			await api.libraryStart(selected, action);
			await poll();
		} catch (e) {
			busy = false;
			toast.error('任务未启动', { description: (e as ApiError).message });
		}
	}
	onMount(() => {
		setBreadcrumb([{ label: '媒体库' }]);
		void load();
		void poll();
	});
	onDestroy(() => {
		disposed = true;
		clearTimeout(timer);
	});
</script>

<svelte:head><title>媒体库 · Bili Sync</title></svelte:head>
<div class="space-y-6 pb-8">
	<header class="flex flex-wrap items-end justify-between gap-4">
		<div>
			<p class="text-primary mb-2 text-xs font-semibold tracking-widest">LIBRARY</p>
			<h1 class="text-2xl font-semibold tracking-tight">每一份收藏，都有去处</h1>
			<p class="text-muted-foreground mt-2 text-sm">
				先查看本地记录，按需核对 B 站画质。共 {total} 个视频。
			</p>
		</div>
		<Button variant="outline" disabled={loading} onclick={load}>刷新记录</Button>
	</header>
	<div class="space-y-4 rounded-xl border bg-card p-4">
		<form
			class="flex gap-2"
			onsubmit={(event) => {
				event.preventDefault();
				index = 0;
				void load();
			}}
		>
			<Input bind:value={query} placeholder="搜索视频标题" aria-label="搜索视频标题" /><Button
				type="submit"
				disabled={loading}>搜索</Button
			>
		</form>
		<div class="flex flex-wrap items-center gap-3 text-sm">
			<select
				aria-label="保存位置筛选"
				class="rounded-md border bg-background px-3 py-2"
				bind:value={storage}
				><option value="all">所有保存位置</option><option value="115">115 网盘</option><option
					value="local">本地</option
				><option value="unknown">待核实的旧记录</option></select
			>
			<select
				aria-label="下载状态筛选"
				class="rounded-md border bg-background px-3 py-2"
				bind:value={download}
				><option value="all">所有下载状态</option><option value="yes">已下载</option><option
					value="no">未完成</option
				></select
			>
			<label class="flex items-center gap-2"
				><Checkbox bind:checked={upgradeOnly} />仅看可升级</label
			><span class="text-muted-foreground">筛选当前页</span>
		</div>
	</div>
	<div class="flex flex-wrap items-center gap-2">
		<span class="mr-2 text-sm">已选 {selected.length} / 25</span><Button
			variant="outline"
			disabled={!selected.length || busy || job?.running}
			onclick={() => run('check')}>对比 B 站画质</Button
		><Button disabled={!selected.length || busy || job?.running} onclick={() => run('upgrade')}
			>升级所选画质</Button
		><Button
			variant="outline"
			disabled={!selected.length || busy || job?.running}
			onclick={() => run('strm')}>补写 STRM</Button
		>
		<p class="text-muted-foreground text-xs">升级先验证实际文件，保留旧版本。</p>
	</div>
	{#if job && (job.running || job.results.length)}<section
			class="rounded-xl border p-4"
			aria-live="polite"
		>
			<p class="font-medium">
				{job.running ? '任务进行中' : '上次任务'} · {job.completed} / {job.total}
			</p>
			<progress class="mt-2 w-full accent-primary" max={job.total || 1} value={job.completed}
			></progress>
			<details class="mt-2 text-sm">
				<summary class="cursor-pointer">查看逐项结果</summary>
				<ul class="mt-2 space-y-1">
					{#each job.results as result (result.page_id)}<li
							class:text-destructive={!result.success}
						>
							#{result.page_id} · {result.message}
						</li>{/each}
				</ul>
			</details>
		</section>{/if}
	<div class="overflow-x-auto rounded-xl border">
		<table class="w-full text-left text-sm">
			<thead class="bg-muted/50 text-muted-foreground"
				><tr
					><th class="p-4"
						><Checkbox
							aria-label="选择当前页前 25 个已完成分 P"
							checked={selected.length > 0 &&
								visible
									.filter((r) => r.downloaded)
									.slice(0, 25)
									.every((r) => selected.includes(r.id))}
							onCheckedChange={(checked) => {
								selected = checked
									? visible
											.filter((r) => r.downloaded)
											.slice(0, 25)
											.map((r) => r.id)
									: [];
							}}
						/></th
					><th class="p-4">视频 / 收藏时间</th><th class="p-4">下载与保存位置</th><th class="p-4"
						>当前画质</th
					><th class="p-4">画质比较</th></tr
				></thead
			>
			<tbody
				>{#each visible as row (row.id)}<tr class="border-t hover:bg-muted/30"
						><td class="p-4"
							><Checkbox
								aria-label={`选择 ${row.title}`}
								checked={selected.includes(row.id)}
								disabled={!row.downloaded}
								onCheckedChange={(checked) => select(row.id, checked)}
							/></td
						><td class="max-w-80 p-4"
							><a
								href={`https://www.bilibili.com/video/${row.bvid}`}
								target="_blank"
								rel="noreferrer"
								class="font-medium hover:text-primary">{row.title}</a
							>{#if row.part !== row.title}<p class="text-muted-foreground mt-1 text-xs">
									{row.part}
								</p>{/if}
							<p class="text-muted-foreground mt-2 text-xs">{row.favorite_time}</p></td
						><td class="max-w-80 p-4"
							><span class="inline-flex rounded-full bg-muted px-2 py-1 text-xs"
								>{row.downloaded ? '已下载' : '未完成'} · {row.storage === '115'
									? '115'
									: row.storage === 'local'
										? '本地'
										: '待核实'}</span
							>
							<details class="mt-2 text-xs">
								<summary class="cursor-pointer text-muted-foreground">查看路径</summary>
								<p class="mt-2 break-all">元数据：{row.metadata_path || '待生成'}</p>
								<p class="mt-1 break-all">视频：{row.storage_path || '旧记录尚无保存位置回执'}</p>
							</details></td
						><td class="whitespace-nowrap p-4"
							>{label(row.quality || row.comparison?.current)}{#if row.quality?.bitrate}<p
									class="text-muted-foreground mt-1 text-xs"
								>
									{(row.quality.bitrate / 1000000).toFixed(1)} Mbps
								</p>{/if}</td
						><td class="max-w-64 p-4"
							>{#if row.comparison}<span class:text-primary={row.comparison.upgradeable}
									>{row.comparison.upgradeable ? '可升级' : '无需升级 / 待核实'}</span
								>
								<p class="text-muted-foreground mt-1 text-xs">{label(row.comparison.candidate)}</p>
								<p class="text-muted-foreground mt-1 text-xs">
									{row.comparison.message}
								</p>{:else}<span class="text-muted-foreground">尚未对比</span>{/if}</td
						></tr
					>{:else}<tr
						><td colspan="5" class="text-muted-foreground p-12 text-center"
							>{loading ? '正在读取本地记录…' : '当前筛选下没有视频'}</td
						></tr
					>{/each}</tbody
			>
		</table>
	</div>
	<div class="flex items-center justify-between">
		<Button
			variant="outline"
			disabled={index === 0 || loading}
			onclick={() => {
				index--;
				void load();
			}}>上一页</Button
		><span class="text-muted-foreground text-sm"
			>第 {index + 1} / {Math.max(1, Math.ceil(total / 25))} 页</span
		><Button
			variant="outline"
			disabled={(index + 1) * 25 >= total || loading}
			onclick={() => {
				index++;
				void load();
			}}>下一页</Button
		>
	</div>
</div>
