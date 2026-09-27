<script lang="ts">
	import { onMount } from 'svelte';
	import api from '$lib/api';
	import type { SysInfo, TaskStatus, ApiError } from '$lib/types';
	import { Button } from '$lib/components/ui/button';
	import VideoWall from '$lib/components/video-wall.svelte';
	import LiveChart from '$lib/components/live-chart.svelte';
	import { setBreadcrumb } from '$lib/stores/breadcrumb';
	import { toast } from 'svelte-sonner';
	let system = $state<SysInfo | null>(null);
	let task = $state<TaskStatus | null>(null);
	let history = $state<{ cpu: number; memory: number; speed: number }[]>([]);
	let folders = $state<{ name: string; bytes: number; count: number }[]>([]);
	let storageError = $state('');
	let triggering = $state(false);
	const colors = ['var(--primary)', '#a78bfa', '#f59e0b', '#fb7185', '#38bdf8', '#94a3b8'];
	const total = $derived(folders.reduce((sum, f) => sum + f.bytes, 0));
	const count = $derived(folders.reduce((sum, f) => sum + f.count, 0));
	const speed = $derived(history.at(-1)?.speed || 0);
	const pie = $derived.by(() => {
		let angle = 0;
		return folders
			.map((f, i) => {
				const start = angle;
				angle += total ? (f.bytes / total) * 360 : 0;
				return `${colors[i % colors.length]} ${start}deg ${angle}deg`;
			})
			.join(',');
	});
	function bytes(n: number) {
		if (!Number.isFinite(n) || n <= 0) return '0 B';
		const i = Math.min(4, Math.floor(Math.log(n) / Math.log(1024)));
		return `${(n / 1024 ** i).toFixed(i ? 1 : 0)} ${['B', 'KB', 'MB', 'GB', 'TB'][i]}`;
	}
	function time(value: string | Date | null) {
		return value
			? new Date(value).toLocaleString('zh-CN', {
					month: '2-digit',
					day: '2-digit',
					hour: '2-digit',
					minute: '2-digit'
				})
			: '—';
	}
	async function storage() {
		try {
			folders = (await api.storageSummary()).data;
			storageError = '';
		} catch (e) {
			storageError = (e as ApiError).message;
		}
	}
	async function trigger() {
		triggering = true;
		try {
			await api.triggerDownloadTask();
			toast.success('已安排检查更新');
		} catch (e) {
			toast.error((e as ApiError).message);
		} finally {
			triggering = false;
		}
	}
	onMount(() => {
		setBreadcrumb([{ label: '仪表盘' }]);
		const stopSystem = api.subscribeToSysInfo((data) => {
			const seconds = system ? (data.timestamp - system.timestamp) / 1000 : 0;
			const currentSpeed =
				system && seconds > 0
					? Math.max(0, data.download_bytes - system.download_bytes) / seconds
					: 0;
			history = [
				...history.slice(-59),
				{
					cpu: data.used_cpu,
					memory: data.total_memory ? (data.used_memory / data.total_memory) * 100 : 0,
					speed: currentSpeed
				}
			];
			system = data;
		});
		const stopTasks = api.subscribeToTasks((data) => {
			task = data;
		});
		void storage();
		const timer = setInterval(storage, 60000);
		return () => {
			stopSystem();
			stopTasks();
			clearInterval(timer);
		};
	});
</script>

<svelte:head><title>仪表盘 - Bili Sync</title></svelte:head>
<div class="space-y-8 pb-8">
	<div class="flex flex-wrap items-end justify-between gap-3">
		<div>
			<p class="text-primary mb-1 text-xs font-semibold tracking-widest">BILI SYNC</p>
			<h1 class="text-3xl font-semibold tracking-tight">收藏，接着看。</h1>
			<p class="text-muted-foreground mt-2 text-sm">下载、云端归档与追更，一处掌握。</p>
		</div>
		<Button variant="outline" onclick={trigger} disabled={triggering || task?.is_running}
			>{triggering ? '正在安排…' : task?.is_running ? '任务运行中' : '立即检查更新'}</Button
		>
	</div>
	<div class="grid gap-4 xl:grid-cols-3">
		<section class="rounded-xl border bg-card p-5">
			<div class="flex items-center justify-between">
				<h2 class="font-semibold">下载状态</h2>
				<span class="text-primary text-xs"
					>{task ? (task.is_running ? '正在处理' : '等待下次更新') : '连接中'}</span
				>
			</div>
			<p class="mt-5 text-4xl font-semibold tracking-tight tabular-nums">
				{bytes(speed)}<span class="text-muted-foreground ml-1 text-base font-normal">/s</span>
			</p>
			<p class="text-muted-foreground mt-1 text-xs">bili-sync 视频下载速度</p>
			<LiveChart
				data={history.map((h) => h.speed / 1024 ** 2)}
				label="最近两分钟视频下载速度"
				unit="MB/s"
			/>
			<div class="text-muted-foreground mt-4 flex flex-wrap justify-between gap-2 text-xs">
				<span>上次完成 {time(task?.last_finish || null)}</span><span
					>下次检查 {time(task?.next_run || null)}</span
				>
			</div>
		</section>
		<section class="rounded-xl border bg-card p-5">
			<h2 class="font-semibold">运行资源</h2>
			<div class="mt-5 flex gap-7">
				<div>
					<p class="text-muted-foreground text-xs">CPU</p>
					<p class="text-primary text-2xl font-semibold tabular-nums">
						{system ? system.used_cpu.toFixed(1) : '—'}%
					</p>
				</div>
				<div>
					<p class="text-muted-foreground text-xs">内存</p>
					<p class="text-2xl font-semibold text-amber-600 tabular-nums">
						{system?.total_memory
							? ((system.used_memory / system.total_memory) * 100).toFixed(1)
							: '—'}%
					</p>
				</div>
			</div>
			<LiveChart
				data={history.map((h) => h.cpu)}
				second={history.map((h) => h.memory)}
				ceiling={100}
				label="CPU 蓝色与内存橙色占用百分比"
				unit="%"
			/>
			<p class="text-muted-foreground mt-4 text-xs">
				内存 {system
					? `${bytes(system.used_memory)} / ${bytes(system.total_memory)}`
					: '等待监控数据'} · 两秒刷新
			</p>
		</section>
		<section class="rounded-xl border bg-card p-5">
			<div class="flex justify-between">
				<h2 class="font-semibold">云端视频分布</h2>
				<span class="text-muted-foreground text-xs">{count} 个文件</span>
			</div>
			<div class="mt-5 flex flex-wrap items-center gap-5">
				<div
					class="relative grid size-36 shrink-0 place-items-center rounded-full"
					style:background={total ? `conic-gradient(${pie})` : 'var(--muted)'}
					role="img"
					aria-label={`已记录云端视频 ${count} 个，总大小 ${bytes(total)}`}
				>
					<div class="grid size-24 place-content-center rounded-full bg-card text-center">
						<strong class="text-lg">{bytes(total)}</strong><span
							class="text-muted-foreground text-[10px]">已记录视频</span
						>
					</div>
				</div>
				<ul class="max-h-40 min-w-0 flex-1 space-y-2 overflow-auto">
					{#each folders as folder, i (folder.name)}<li class="flex items-center gap-2 text-xs">
							<span
								class="size-2 shrink-0 rounded-full"
								style:background={colors[i % colors.length]}
							></span><span class="min-w-0 flex-1 truncate" title={folder.name}>{folder.name}</span
							><span class="text-muted-foreground tabular-nums"
								>{folder.count} 个 · {bytes(folder.bytes)}</span
							>
						</li>{:else}<li class="text-muted-foreground text-xs">上传完成后自动记录</li>{/each}
				</ul>
			</div>
			<p class="text-muted-foreground mt-5 text-xs">
				按本地上传记录汇总。旧视频尚未登记的大小未统计；不扫描网盘目录。
			</p>
			{#if storageError}<p class="text-destructive mt-2 text-xs">{storageError}</p>{/if}
		</section>
	</div>
	<section class="border-t pt-7"><VideoWall embedded /></section>
</div>
