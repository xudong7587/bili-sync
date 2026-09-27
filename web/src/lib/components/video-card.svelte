<script lang="ts">
	import { Badge } from '$lib/components/ui/badge/index.js';

	import { Button } from '$lib/components/ui/button/index.js';
	import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import { Checkbox } from '$lib/components/ui/checkbox/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import type { VideoInfo } from '$lib/types';
	import BrushCleaningIcon from '@lucide/svelte/icons/brush-cleaning';

	import EllipsisIcon from '@lucide/svelte/icons/ellipsis';
	import FolderIcon from '@lucide/svelte/icons/folder';

	import RotateCcwIcon from '@lucide/svelte/icons/rotate-ccw';
	import SquareArrowOutUpRightIcon from '@lucide/svelte/icons/square-arrow-out-up-right';
	import UserIcon from '@lucide/svelte/icons/user';
	import { goto } from '$app/navigation';

	// 将 bvid 设置为可选属性，但保留 VideoInfo 的其它所有属性

	export let video: Omit<VideoInfo, 'bvid'> & { bvid?: string };
	export let source: { type: string; name: string } | null = null; // 视频源信息
	export let showActions: boolean = true; // 控制是否显示操作按钮
	export let mode: 'default' | 'detail' | 'page' = 'default'; // 卡片模式
	export let customTitle: string = ''; // 自定义标题
	export let customSubtitle: string = ''; // 自定义副标题
	export let taskNames: string[] = []; // 自定义任务名称
	export let showProgress: boolean = true; // 是否显示进度信息
	export let onReset: ((forceReset: boolean) => Promise<void>) | null = null; // 自定义重置函数
	export let onClearAndReset: (() => Promise<void>) | null = null; // 自定义清空重置函数
	export let resetDialogOpen = false; // 导出对话框状态，让父组件可以控制
	export let clearAndResetDialogOpen = false; // 导出清空重置对话框状态
	export let resetting = false;
	export let clearAndResetting = false;

	let forceReset = false;

	function getStatusText(status: number): string {
		if (status === 7) {
			return '已完成';
		} else if (status === 0) {
			return '未开始';
		} else {
			return `失败${status}次`;
		}
	}

	function getOverallStatus(
		downloadStatus: number[],
		shouldDownload: boolean,
		valid: boolean
	): {
		text: string;
		style: string;
	} {
		if (!valid) {
			// 视频属性表明已失效，或由于各种条件判断（充电视频等）判定为无效的情况
			return { text: '失效', style: 'bg-gray-100 text-gray-700' };
		}
		if (!shouldDownload) {
			// 被过滤规则排除，显示为“跳过”
			return { text: '跳过', style: 'bg-gray-100 text-gray-700' };
		}
		const completed = downloadStatus.filter((status) => status === 7).length;
		const total = downloadStatus.length;
		const failed = downloadStatus.filter((status) => status !== 7 && status !== 0).length;

		if (completed === total) {
			// 全部完成，显示为“完成”
			return { text: '完成', style: 'bg-emerald-700 text-emerald-100' };
		} else if (failed > 0) {
			// 出现了失败，显示为“失败”
			return { text: '失败', style: 'bg-rose-700 text-rose-100' };
		} else {
			// 还未开始，显示为“等待”
			return { text: '等待', style: 'bg-yellow-700 text-yellow-100' };
		}
	}

	function getTaskName(index: number): string {
		if (taskNames.length > 0) {
			return taskNames[index] || `任务${index + 1}`;
		}
		const defaultTaskNames = ['视频封面', '视频信息', 'UP主头像', 'UP主信息', '分页下载'];
		return defaultTaskNames[index] || `任务${index + 1}`;
	}

	$: overallStatus = getOverallStatus(video.download_status, video.should_download, video.valid);
	$: completed = video.download_status.filter((status) => status === 7).length;
	$: total = video.download_status.length;

	async function handleReset() {
		resetting = true;
		if (onReset) {
			await onReset(forceReset);
		}
		resetting = false;
		resetDialogOpen = false;
		forceReset = false;
	}

	async function handleClearAndReset() {
		clearAndResetting = true;
		if (onClearAndReset) {
			await onClearAndReset();
		}
		clearAndResetting = false;
		clearAndResetDialogOpen = false;
	}

	function handleViewDetail() {
		goto(`/video/${video.id}`);
	}

	// 根据模式确定显示的标题和副标题
	$: displayTitle = customTitle || video.name;
	$: displaySubtitle = customSubtitle || video.upper_name;
	function date(value?: string) {
		if (!value) return '—';
		const parsed = new Date(value);
		return Number.isNaN(parsed.getTime()) ? '—' : parsed.toLocaleDateString('zh-CN');
	}
</script>

<article
	class="group flex h-full min-w-0 flex-col overflow-hidden rounded-xl border bg-card shadow-sm transition-shadow hover:shadow-md"
>
	{#if mode === 'default'}
		<a
			href={`/video/${video.id}`}
			class="relative block aspect-video overflow-hidden bg-muted"
			aria-label={`查看 ${displayTitle}`}
		>
			{#if video.cover}
				<img
					src={video.cover}
					alt=""
					referrerpolicy="no-referrer"
					loading="lazy"
					class="h-full w-full object-cover"
				/>
			{:else}
				<div class="grid h-full place-content-center text-sm text-muted-foreground">暂无封面</div>
			{/if}
			<div
				class="absolute inset-x-0 bottom-0 h-16 bg-gradient-to-t from-black/65 to-transparent"
			></div>
			<div
				class="absolute inset-x-3 bottom-2.5 flex items-center justify-between gap-2 text-xs text-white"
			>
				<span class="rounded bg-black/45 px-2 py-0.5 backdrop-blur-sm">{overallStatus.text}</span>
				{#if showProgress}<span class="font-semibold tabular-nums" title="下载任务完成度"
						>{total ? Math.round((completed / total) * 100) : 0}%</span
					>{/if}
			</div>
		</a>
	{/if}
	<div class="flex flex-1 flex-col gap-2.5 p-3">
		<h3
			class="line-clamp-2 text-sm font-semibold leading-5"
			class:min-h-10={mode === 'default'}
			title={displayTitle}
		>
			{#if mode === 'default'}<a href={`/video/${video.id}`} class="hover:text-primary"
					>{displayTitle}</a
				>{:else}{displayTitle}{/if}
		</h3>
		{#if displaySubtitle || source}
			<div class="flex min-w-0 items-center justify-between gap-2 text-xs text-muted-foreground">
				<span class="flex min-w-0 items-center gap-1.5"
					><UserIcon class="size-3.5 shrink-0" /><span class="truncate" title={displaySubtitle}
						>{displaySubtitle}</span
					></span
				>
				{#if source}<span class="flex max-w-[45%] min-w-0 items-center gap-1"
						><FolderIcon class="size-3 shrink-0" /><span class="truncate" title={source.name}
							>{source.name}</span
						></span
					>{/if}
			</div>
		{/if}
		{#if mode === 'default'}
			<div class="grid grid-cols-2 gap-x-2 gap-y-1 text-[11px] text-muted-foreground tabular-nums">
				<span title={video.created_at}>入库 {date(video.created_at)}</span>
				<span class="text-right" title={video.favtime}>收藏 {date(video.favtime)}</span>
			</div>
		{:else if showProgress}
			<div class="flex items-center justify-between gap-3 text-xs">
				<Badge variant="secondary" class={overallStatus.style}>{overallStatus.text}</Badge>
				<span class="font-semibold text-primary tabular-nums"
					>{total ? Math.round((completed / total) * 100) : 0}% · {completed}/{total}</span
				>
			</div>
			<div class="flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted-foreground">
				{#each video.download_status as status, i (i)}<span
						>{getTaskName(i)} · {getStatusText(status)}</span
					>{/each}
			</div>
		{/if}
		{#if showActions && mode === 'default'}
			<div class="mt-auto flex items-center justify-between gap-2 border-t pt-2">
				<span class="truncate font-mono text-[10px] text-muted-foreground" title={video.bvid}
					>{video.bvid || '—'}</span
				>
				<div class="flex shrink-0 items-center gap-1">
					<Button size="sm" variant="ghost" class="h-7 px-2 text-xs" onclick={handleViewDetail}
						>详情</Button
					>
					<DropdownMenu.Root>
						<DropdownMenu.Trigger>
							{#snippet child({ props })}
								<Button
									{...props}
									size="icon"
									variant="outline"
									class="size-7 shrink-0"
									aria-label="视频操作"
								>
									<EllipsisIcon class="h-3 w-3" />
								</Button>
							{/snippet}
						</DropdownMenu.Trigger>
						<DropdownMenu.Content align="start" class="w-48">
							<DropdownMenu.Item class="cursor-pointer" onclick={() => (resetDialogOpen = true)}>
								<RotateCcwIcon class="mr-2 h-4 w-4" />
								重置
							</DropdownMenu.Item>
							<DropdownMenu.Item
								class="cursor-pointer"
								onclick={() => (clearAndResetDialogOpen = true)}
							>
								<BrushCleaningIcon class="mr-2 h-4 w-4" />
								清空重置
							</DropdownMenu.Item>
							<DropdownMenu.Item
								class="cursor-pointer"
								onclick={() =>
									window.open(`https://www.bilibili.com/video/${video.bvid}/`, '_blank')}
							>
								<SquareArrowOutUpRightIcon class="mr-2 h-4 w-4" />
								在 B 站打开
							</DropdownMenu.Item>
						</DropdownMenu.Content>
					</DropdownMenu.Root>
				</div>
			</div>
		{/if}
	</div>
</article>

<!-- 重置确认对话框 -->
<AlertDialog.Root bind:open={resetDialogOpen}>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Title>重置视频</AlertDialog.Title>
			<AlertDialog.Description>
				确定要重置视频 <strong>"{displayTitle}"</strong> 的下载状态吗？
				<br />
				此操作会将所有的失败状态重置为未开始，<span class="text-destructive font-medium"
					>无法撤销</span
				>。
			</AlertDialog.Description>
		</AlertDialog.Header>

		<div class="space-y-4 py-4">
			<div class="rounded-lg border border-orange-200 bg-orange-50 p-3">
				<div class="mb-2 flex items-center space-x-2">
					<Checkbox id="force-reset-all" bind:checked={forceReset} />
					<Label for="force-reset-all" class="text-sm font-medium text-orange-700"
						>⚠️ 强制重置</Label
					>
				</div>
				<p class="text-xs leading-relaxed text-orange-700">
					除重置失败状态外还会检查修复任务状态的标识位 <br />
					版本升级引入新任务时勾选该选项进行重置，可以允许旧视频执行新任务
				</p>
			</div>
		</div>

		<AlertDialog.Footer>
			<AlertDialog.Cancel
				onclick={() => {
					forceReset = false;
				}}>取消</AlertDialog.Cancel
			>
			<AlertDialog.Action
				onclick={handleReset}
				disabled={resetting}
				class={forceReset ? 'bg-orange-600 hover:bg-orange-700' : ''}
			>
				{resetting ? '重置中...' : forceReset ? '确认强制重置' : '确认重置'}
			</AlertDialog.Action>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>

<!-- 清空重置确认对话框 -->
<AlertDialog.Root bind:open={clearAndResetDialogOpen}>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Title>清空重置视频</AlertDialog.Title>
			<AlertDialog.Description>
				确定要清空重置视频 <strong>"{displayTitle}"</strong> 吗？
				<br />
				<br />
				此操作会：
				<ul class="mt-2 ml-4 list-disc space-y-1">
					<li>将视频状态重置为未开始</li>
					<li>删除所有分页信息</li>
					<li class="text-destructive font-medium">删除视频对应的文件夹</li>
				</ul>
				<br />
				该功能可在多页视频变更后手动触发全量更新，执行后<span class="text-destructive font-medium"
					>无法撤销</span
				>。
			</AlertDialog.Description>
		</AlertDialog.Header>

		<AlertDialog.Footer>
			<AlertDialog.Cancel>取消</AlertDialog.Cancel>
			<AlertDialog.Action
				onclick={handleClearAndReset}
				disabled={clearAndResetting}
				class="bg-destructive hover:bg-destructive/90"
			>
				{clearAndResetting ? '清空重置中...' : '确认清空重置'}
			</AlertDialog.Action>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>
