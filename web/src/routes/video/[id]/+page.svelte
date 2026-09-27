<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';
	import { Button } from '$lib/components/ui/button/index.js';
	import api from '$lib/api';
	import SquareArrowOutUpRightIcon from '@lucide/svelte/icons/square-arrow-out-up-right';
	import type { ApiError, VideoResponse, UpdateVideoStatusRequest } from '$lib/types';
	import BrushCleaningIcon from '@lucide/svelte/icons/brush-cleaning';
	import RotateCcwIcon from '@lucide/svelte/icons/rotate-ccw';
	import SquarePenIcon from '@lucide/svelte/icons/square-pen';
	import { setBreadcrumb } from '$lib/stores/breadcrumb';
	import { appStateStore, ToQuery } from '$lib/stores/filter';
	import VideoCard from '$lib/components/video-card.svelte';
	import StatusEditor from '$lib/components/status-editor.svelte';
	import { toast } from 'svelte-sonner';

	let videoData: VideoResponse | null = null;
	let loading = false;
	let error: string | null = null;
	let resetDialogOpen = false;
	let resetting = false;
	let clearAndResetDialogOpen = false;
	let clearAndResetting = false;
	let statusEditorOpen = false;
	let statusEditorLoading = false;

	async function loadVideoDetail() {
		const videoId = parseInt($page.params.id!);
		if (isNaN(videoId)) {
			error = '无效的视频 ID';
			toast.error('无效的视频 ID');
			return;
		}
		loading = true;
		error = null;
		try {
			const result = await api.getVideo(videoId);
			videoData = result.data;
		} catch (cause) {
			error = (cause as ApiError).message || '加载视频详情失败';
			console.error('加载视频详情失败：', cause);
			toast.error('加载视频详情失败', {
				description: error
			});
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		setBreadcrumb([
			{
				label: '视频',
				href: `/${ToQuery($appStateStore)}`
			},
			{ label: '视频详情' }
		]);
	});

	// 监听路由参数变化
	$: if ($page.params.id) {
		loadVideoDetail();
	}

	async function handleStatusEditorSubmit(request: UpdateVideoStatusRequest) {
		if (!videoData) return;

		statusEditorLoading = true;
		try {
			const result = await api.updateVideoStatus(videoData.video.id, request);
			const data = result.data;

			if (data.success) {
				// 更新本地数据
				videoData = {
					...videoData,
					video: data.video,
					pages: data.pages
				};
				statusEditorOpen = false;
				toast.success('状态更新成功');
			} else {
				toast.error('状态更新失败');
			}
		} catch (error) {
			console.error('状态更新失败：', error);
			toast.error('状态更新失败', {
				description: (error as ApiError).message
			});
		} finally {
			statusEditorLoading = false;
		}
	}

	async function handleReset(forceReset: boolean) {
		if (!videoData) return;
		try {
			const result = await api.resetVideoStatus(videoData.video.id, { force: forceReset });
			const data = result.data;
			if (data.resetted) {
				videoData = {
					...videoData,
					video: data.video,
					pages: data.pages
				};
				toast.success('重置成功');
			} else {
				toast.info('重置无效', {
					description: `视频「${data.video.name}」没有失败的状态，无需重置`
				});
			}
		} catch (error) {
			console.error('重置失败:', error);
			toast.error('重置失败', {
				description: (error as ApiError).message
			});
		}
	}

	async function handleClearAndReset() {
		if (!videoData) return;
		try {
			const result = await api.clearAndResetVideoStatus(videoData.video.id);
			const data = result.data;
			videoData = {
				...videoData,
				video: data.video,
				pages: []
			};
			if (data.warning) {
				toast.warning('清空重置成功', {
					description: data.warning
				});
			} else {
				toast.success('清空重置成功', {
					description: `视频「${data.video.name}」已清空重置`
				});
			}
		} catch (error) {
			console.error('清空重置失败：', error);
			toast.error('清空重置失败', {
				description: (error as ApiError).message
			});
		}
	}
</script>

<svelte:head>
	<title>{videoData?.video.name || '视频详情'} - Bili Sync</title>
</svelte:head>

{#if loading}
	<div class="flex items-center justify-center py-12">
		<div class="text-muted-foreground">加载中...</div>
	</div>
{:else if error}
	<div class="flex items-center justify-center py-12">
		<div class="space-y-2 text-center">
			<p class="text-destructive">{error}</p>
			<button
				class="text-muted-foreground hover:text-foreground text-sm transition-colors"
				onclick={() => goto('/')}
			>
				返回首页
			</button>
		</div>
	</div>
{:else if videoData}
	<section class="relative isolate mb-6 overflow-hidden rounded-2xl border bg-card">
		{#if videoData.video.cover}
			<img
				src={videoData.video.cover}
				alt=""
				referrerpolicy="no-referrer"
				class="pointer-events-none absolute inset-0 -z-10 h-full w-full scale-110 object-cover opacity-15 blur-2xl"
			/>
		{/if}
		<div class="grid gap-5 p-5 md:grid-cols-[minmax(220px,34%)_1fr] md:p-6">
			<div class="overflow-hidden rounded-xl bg-muted self-start">
				{#if videoData.video.cover}<img
						src={videoData.video.cover}
						alt={videoData.video.name}
						referrerpolicy="no-referrer"
						class="aspect-video w-full object-cover"
					/>{:else}<div class="grid aspect-video place-items-center text-muted-foreground">
						暂无封面
					</div>{/if}
			</div>
			<div class="min-w-0 space-y-4">
				<div>
					<p class="mb-2 text-xs font-mono text-muted-foreground">
						{videoData.video.bvid} · {videoData.pages.length} 个分 P
					</p>
					<h1 class="text-xl font-semibold leading-snug md:text-2xl">{videoData.video.name}</h1>
				</div>
				<div class="flex items-center gap-2.5">
					{#if videoData.metadata?.upper_face}<img
							src={videoData.metadata.upper_face}
							alt=""
							referrerpolicy="no-referrer"
							class="size-9 rounded-full object-cover ring-2 ring-background"
						/>{/if}
					<div>
						<p class="text-sm font-medium">{videoData.video.upper_name}</p>
						<p class="text-xs text-muted-foreground">
							UP 主{videoData.metadata?.upper_id ? ` · UID ${videoData.metadata.upper_id}` : ''}
						</p>
					</div>
				</div>
				<dl class="grid grid-cols-2 gap-x-4 gap-y-2 text-xs sm:grid-cols-3">
					{#each [['发布时间', videoData.metadata?.pubtime], ['收藏时间', videoData.video.favtime], ['入库时间', videoData.video.created_at]] as [label, value] (label)}
						<div>
							<dt class="text-muted-foreground">{label}</dt>
							<dd class="mt-1 tabular-nums">
								{value ? new Date(value).toLocaleString('zh-CN') : '—'}
							</dd>
						</div>
					{/each}
				</dl>
				{#if videoData.metadata?.tags?.length}<div class="flex flex-wrap gap-1.5">
						{#each videoData.metadata.tags as tag, i (i)}<span
								class="rounded-md bg-primary/10 px-2 py-1 text-[11px] text-primary">{tag}</span
							>{/each}
					</div>{/if}
			</div>
		</div>
		{#if videoData.metadata?.intro || videoData.metadata?.path}<div
				class="space-y-3 border-t bg-background/50 px-5 py-4 md:px-6"
			>
				{#if videoData.metadata.intro}<p
						class="max-h-48 overflow-auto whitespace-pre-wrap text-sm leading-relaxed text-muted-foreground"
					>
						{videoData.metadata.intro}
					</p>{/if}
				{#if videoData.metadata.path}<p class="break-all text-xs text-muted-foreground">
						元数据目录 · <span class="font-mono">{videoData.metadata.path}</span>
					</p>{/if}
			</div>{/if}
	</section>
	<!-- 视频任务区域 -->
	<section>
		<div class="mb-4 flex flex-wrap items-center justify-between gap-3">
			<h2 class="text-xl font-semibold">下载任务</h2>
			<div class="flex flex-wrap gap-2">
				<Button
					size="sm"
					variant="outline"
					class="shrink-0 cursor-pointer "
					onclick={() => (statusEditorOpen = true)}
					disabled={statusEditorLoading}
				>
					<SquarePenIcon class="mr-2 h-4 w-4" />
					编辑状态
				</Button>
				<Button
					size="sm"
					variant="outline"
					class="shrink-0 cursor-pointer "
					onclick={() => (resetDialogOpen = true)}
					disabled={resetting || clearAndResetting}
				>
					<RotateCcwIcon class="mr-2 h-4 w-4 {resetting ? 'animate-spin' : ''}" />
					重置
				</Button>
				<Button
					size="sm"
					variant="outline"
					class="shrink-0 cursor-pointer "
					onclick={() => (clearAndResetDialogOpen = true)}
					disabled={resetting || clearAndResetting}
				>
					<BrushCleaningIcon class="mr-2 h-4 w-4 {clearAndResetting ? 'animate-spin' : ''}" />
					清空重置
				</Button>
				<Button
					size="sm"
					variant="outline"
					class="shrink-0 cursor-pointer "
					onclick={() =>
						window.open(`https://www.bilibili.com/video/${videoData?.video.bvid}/`, '_blank')}
					disabled={statusEditorLoading}
				>
					<SquareArrowOutUpRightIcon class="mr-2 h-4 w-4" />
					在 B 站打开
				</Button>
			</div>
		</div>

		<div style="margin-bottom: 1rem;">
			<VideoCard
				video={videoData.video}
				mode="detail"
				showActions={false}
				taskNames={['视频封面', '视频信息', 'UP 主头像', 'UP 主信息', '分页下载']}
				bind:resetDialogOpen
				bind:resetting
				bind:clearAndResetDialogOpen
				bind:clearAndResetting
				onReset={handleReset}
				onClearAndReset={handleClearAndReset}
			/>
		</div>
	</section>

	<section>
		{#if videoData.pages && videoData.pages.length > 0}
			<div>
				<div class="mb-4 flex flex-wrap items-center justify-between gap-3">
					<h2 class="text-xl font-semibold">分页列表</h2>
					<div class="text-muted-foreground text-sm">
						共 {videoData.pages.length} 个分页
					</div>
				</div>

				<div
					class="grid gap-4"
					style="grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));"
				>
					{#each videoData.pages as pageInfo (pageInfo.id)}
						<VideoCard
							video={{
								id: pageInfo.id,
								name: `P${pageInfo.pid}: ${pageInfo.name}`,
								upper_name: '',
								download_status: pageInfo.download_status,
								should_download: videoData.video.should_download,
								valid: videoData.video.valid
							}}
							mode="page"
							showActions={false}
							customTitle="P{pageInfo.pid}: {pageInfo.name}"
							customSubtitle=""
							taskNames={['视频封面', '视频内容', '视频信息', '视频弹幕', '视频字幕']}
						/>
					{/each}
				</div>
			</div>
		{:else}
			<div class="py-12 text-center">
				<div class="space-y-2">
					<p class="text-muted-foreground">暂无分 P 数据</p>
				</div>
			</div>
		{/if}
	</section>

	<!-- 状态编辑器 -->
	{#if videoData}
		<StatusEditor
			bind:open={statusEditorOpen}
			video={videoData.video}
			pages={videoData.pages}
			loading={statusEditorLoading}
			onsubmit={handleStatusEditorSubmit}
		/>
	{/if}
{/if}
