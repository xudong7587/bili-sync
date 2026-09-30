<script lang="ts">
	import { onMount } from 'svelte';
	import api from '$lib/api';
	import type { ApiError } from '$lib/types';
	import LibraryMigration from '$lib/components/library-migration.svelte';
	import { setBreadcrumb } from '$lib/stores/breadcrumb';
	let destination = $state('');
	let connected = $state(false);
	let webhook = $state(false);
	let loading = $state(true);
	let error = $state('');
	onMount(() => {
		setBreadcrumb([{ label: '存储' }, { label: '存量视频迁移' }]);
		void api
			.getConfig()
			.then(({ data }) => {
				destination = data.cd2_save_path || '';
				connected = !!(data.cd2_url && data.cd2_token && data.cd2_save_path);
				webhook = !!data.media_index_webhook_enabled;
			})
			.catch((e: ApiError) => {
				error = e.message;
			})
			.finally(() => {
				loading = false;
			});
	});
</script>

<svelte:head><title>存量视频迁移 - Bili Sync</title></svelte:head>
<div class="space-y-5 pb-8">
	<div class="flex flex-wrap items-center justify-between gap-3">
		<h2 class="text-xl font-semibold">存量视频迁移</h2>
		<a
			class="rounded-md border bg-background px-3 py-2 text-sm hover:bg-accent"
			href="/settings?section=cloud">配置 CD2 与入库联动 →</a
		>
	</div>
	<div class="flex flex-wrap gap-x-6 gap-y-2 rounded-xl bg-muted/40 px-5 py-4 text-sm">
		<span>CD2：{loading ? '读取配置中…' : connected ? '已配置' : '待配置'}</span>
		<span>入库通知：{loading ? '读取配置中…' : webhook ? '已启用' : '未启用'}</span>
		{#if destination}<span class="break-all text-muted-foreground">网盘根目录：{destination}</span
			>{/if}
	</div>
	{#if error}<p class="text-sm text-destructive">{error}</p>{/if}
	<LibraryMigration />
</div>
