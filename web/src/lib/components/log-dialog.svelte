<script lang="ts">
	import { onMount } from 'svelte';
	import api from '$lib/api';
	import { Button } from '$lib/components/ui/button';
	import * as Dialog from '$lib/components/ui/dialog';
	import ScrollText from '@lucide/svelte/icons/scroll-text';
	let open = $state(false);
	let paused = $state(false);
	let filter = $state('all');
	let logs = $state<{ timestamp: string; level: string; message: string; id: number }[]>([]);
	let sequence = 0;
	onMount(() =>
		api.subscribeToLogs((data: string) => {
			if (paused) return;
			try {
				const log = JSON.parse(data);
				logs = [...logs.slice(-499), { ...log, id: sequence++ }];
			} catch {
				/* Ignore malformed events. */
			}
		})
	);
	const visible = $derived(
		logs.filter((log) => filter === 'all' || log.level === filter).toReversed()
	);
</script>

<Dialog.Root bind:open>
	<Dialog.Trigger
		class="inline-flex h-9 items-center gap-2 rounded-md border px-3 text-sm hover:bg-accent"
		><ScrollText class="size-4" />日志</Dialog.Trigger
	>
	<Dialog.Content class="flex max-h-[85vh] flex-col sm:max-w-5xl">
		<Dialog.Header
			><Dialog.Title>运行日志</Dialog.Title><Dialog.Description
				>最近 500 条 · 最新在前</Dialog.Description
			></Dialog.Header
		>
		<div class="flex gap-2">
			<select
				aria-label="日志级别"
				bind:value={filter}
				class="rounded border bg-background px-2 text-sm"
				><option value="all">全部级别</option><option value="ERROR">错误</option><option
					value="WARN">警告</option
				><option value="INFO">信息</option></select
			><Button size="sm" variant="outline" onclick={() => (paused = !paused)}
				>{paused ? '继续接收' : '暂停接收'}</Button
			>
		</div>
		<div
			class="min-h-48 overflow-auto rounded-lg bg-muted/40 p-3 font-mono text-xs"
			aria-label="日志内容"
		>
			{#each visible as log (log.id)}<div class="border-border/50 border-b py-2">
					<span class="text-muted-foreground">{log.timestamp}</span>
					<strong
						class={log.level === 'ERROR'
							? 'text-destructive'
							: log.level === 'WARN'
								? 'text-amber-600'
								: 'text-primary'}>{log.level}</strong
					>
					<p class="mt-1 whitespace-pre-wrap break-all">{log.message}</p>
				</div>{:else}<p class="text-muted-foreground py-10 text-center">
					暂无符合条件的日志
				</p>{/each}
		</div>
	</Dialog.Content>
</Dialog.Root>
