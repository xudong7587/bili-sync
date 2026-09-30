<script lang="ts">
	import { onMount } from 'svelte';
	import api from '$lib/api';
	import type { ApiError, MigrationOptions, MigrationStatus } from '$lib/types';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import { toast } from 'svelte-sonner';
	let job = $state<MigrationStatus | null>(null);
	let busy = $state(false);
	let error = $state('');
	let options = $state<MigrationOptions>({
		source_root: '/media',
		min_interval: 15,
		max_interval: 30,
		batch_size: 10,
		batch_cooldown: 120
	});
	const active = $derived(job?.phase === 'running' || job?.phase === 'pausing');
	const unfinished = $derived(!!job && job.completed < job.total);
	const changed = $derived(
		!!job?.options && JSON.stringify(options) !== JSON.stringify(job.options)
	);
	const labels: Record<string, string> = {
		ready: '等待开始',
		running: '迁移中',
		pausing: '等待当前文件完成',
		paused: '已暂停',
		failed: '已暂停，需处理错误',
		completed: '迁移完成'
	};
	function size(n: number) {
		return `${(n / 1024 ** 3).toFixed(2)} GB`;
	}
	async function refresh() {
		try {
			job = (await api.migrationStatus()).data;
			error = '';
		} catch (e) {
			error = (e as ApiError).message;
		}
	}
	async function action(type: 'plan' | 'run' | 'pause') {
		busy = true;
		try {
			if (type === 'plan') job = (await api.migrationPlan(options)).data;
			if (type === 'run') await api.migrationRun();
			if (type === 'pause') await api.migrationPause();
			await refresh();
			if (type === 'plan')
				toast.success(job?.total ? `已找到 ${job.total} 个待迁移视频文件` : '没有找到待迁移视频');
		} catch (e) {
			toast.error((e as ApiError).message);
		} finally {
			busy = false;
		}
	}
	onMount(() => {
		void refresh().then(() => {
			if (job?.options) options = { ...job.options };
		});
		const timer = setInterval(refresh, 3000);
		return () => clearInterval(timer);
	});
</script>

<section class="space-y-4 rounded-xl border p-5">
	<div class="flex flex-wrap items-center justify-between gap-3">
		<div>
			<h3 class="font-semibold">迁移设置与进度</h3>
			<p class="mt-1 text-sm text-muted-foreground">
				将本地已下载的 B 站视频按原目录上传到网盘，元数据保留在本地。
			</p>
		</div>
		{#if job?.phase}<span class="rounded-full bg-muted px-3 py-1 text-xs"
				>{labels[job.phase] || job.phase}</span
			>{/if}
	</div>
	<p class="text-xs leading-relaxed text-muted-foreground">
		先在「视频网盘分流」中保存 CD2 和入库联动配置，再预览迁移清单。清单来自当前配置目录中的
		bili-sync 数据库，只包含下载成功的视频文件。旧版用户可直接复用原配置和媒体目录；默认读取
		/media，也可填写已映射的独立视频根目录，其相对结构应与 /media
		一致。本地原视频会保留，确认入库后可自行清理。
	</p>
	<div class="space-y-2">
		<Label for="migration-root">本地视频根目录（容器内路径）</Label><Input
			id="migration-root"
			bind:value={options.source_root}
			disabled={active || busy}
			placeholder="/media"
		/>
	</div>
	<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
		<div class="space-y-2">
			<Label for="migration-min">最短文件间隔 / 秒</Label><Input
				id="migration-min"
				type="number"
				min={5}
				max={3600}
				bind:value={options.min_interval}
				disabled={active || busy}
			/>
		</div>
		<div class="space-y-2">
			<Label for="migration-max">最长文件间隔 / 秒</Label><Input
				id="migration-max"
				type="number"
				min={5}
				max={3600}
				bind:value={options.max_interval}
				disabled={active || busy}
			/>
		</div>
		<div class="space-y-2">
			<Label for="migration-batch">每批文件数</Label><Input
				id="migration-batch"
				type="number"
				min={1}
				max={100}
				bind:value={options.batch_size}
				disabled={active || busy}
			/>
		</div>
		<div class="space-y-2">
			<Label for="migration-rest">每批额外休息 / 秒</Label><Input
				id="migration-rest"
				type="number"
				min={0}
				max={3600}
				bind:value={options.batch_cooldown}
				disabled={active || busy}
			/>
		</div>
	</div>
	<p class="text-xs text-muted-foreground">
		逐个上传，文件间随机等待，每批额外休息。默认每 10 个文件休息 2
		分钟；降低频率可减少风控风险，但无法保证不触发网盘限制。单个文件处理时与下载、画质操作互斥；文件间休息时可继续追更。遇到正在运行的任务，迁移会等待其完成。
	</p>
	{#if job?.total || job?.phase === 'completed'}
		<div class="grid grid-cols-2 gap-3 rounded-lg bg-muted/40 p-4 sm:grid-cols-4">
			<div>
				<p class="text-xs text-muted-foreground">已确认 / 总文件</p>
				<p class="mt-1 text-lg font-semibold tabular-nums">{job.completed} / {job.total}</p>
			</div>
			<div>
				<p class="text-xs text-muted-foreground">已确认 / 总大小</p>
				<p class="mt-1 text-sm font-medium tabular-nums">
					{size(job.completed_bytes)} / {size(job.total_bytes)}
				</p>
			</div>
			<div>
				<p class="text-xs text-muted-foreground">已在网盘</p>
				<p class="mt-1 text-lg font-semibold tabular-nums">{job.skipped_cloud}</p>
			</div>
			<div>
				<p class="text-xs text-muted-foreground">未完成、缺失或路径不符</p>
				<p class="mt-1 text-lg font-semibold tabular-nums">
					{job.skipped_missing + job.skipped_invalid}
				</p>
			</div>
		</div>
		<p class="break-all text-xs text-muted-foreground">网盘目标：{job.destination}</p>
		{#if job.samples?.length}<details class="rounded-lg border px-3 py-2 text-xs">
				<summary class="cursor-pointer text-muted-foreground">查看前 3 个文件的路径映射</summary>
				<ul class="mt-3 space-y-3">
					{#each job.samples as file (file.source)}<li class="space-y-1 break-all">
							<p>{file.source}</p>
							<p class="text-muted-foreground">→ {file.target}</p>
						</li>{/each}
				</ul>
			</details>{/if}
	{/if}
	{#if job?.current_file}<p class="break-all text-xs">当前文件：{job.current_file}</p>{/if}
	{#if job?.error || error}<p class="rounded-lg bg-destructive/10 p-3 text-sm text-destructive">
			{job?.error || error}
		</p>{/if}
	{#if job?.notification_error}<p class="text-sm text-amber-600">
			视频已保存，入库通知待重试：{job.notification_error}
		</p>{/if}
	<div class="flex flex-wrap gap-2">
		<Button variant="outline" onclick={() => action('plan')} disabled={busy || active}
			>{unfinished ? '重新预览' : '预览待迁移视频'}</Button
		>
		{#if unfinished && !active}<Button onclick={() => action('run')} disabled={busy || changed}
				>{job?.phase === 'ready' ? '开始上传' : '继续迁移'}</Button
			>{/if}
		{#if active}<Button
				variant="outline"
				onclick={() => action('pause')}
				disabled={busy || job?.phase === 'pausing'}>暂停迁移</Button
			>{/if}
	</div>
	{#if changed && unfinished}<p class="text-xs text-muted-foreground">
			设置已修改，请重新预览后继续。
		</p>{/if}
	<p class="text-xs leading-relaxed text-muted-foreground">
		暂停会等待当前文件上传确认，随后保存进度；重启后手动继续。修改频率或根目录后点击「重新预览」，已确认的网盘视频会跳过。每批新迁移的视频确认存好后合并一次完成事件，MediaIndex
		或兼容的 MP 接收器负责生成 STRM。MP 插件需支持此 webhook
		事件；也可使用网盘生活事件或定时增量扫描。
	</p>
</section>
