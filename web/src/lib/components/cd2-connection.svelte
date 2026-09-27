<script lang="ts">
	import api from '$lib/api';
	import { Button } from '$lib/components/ui/button';
	import type { ApiError } from '$lib/types';
	let { url, token, path }: { url: string; token: string; path: string } = $props();
	let busy = $state(false);
	let tested = $state('');
	const fingerprint = $derived(JSON.stringify([url, token, path]));
	let message = $state('');
	let error = $state(false);
	async function check() {
		busy = true;
		message = '';
		error = false;
		tested = fingerprint;
		try {
			const s = (await api.cloudSpace({ url, token, path })).data;
			message = `CD2 已连接 · 可用 ${(s.free / 1024 ** 4).toFixed(2)} TB / 共 ${(s.total / 1024 ** 4).toFixed(2)} TB`;
		} catch (e) {
			error = true;
			message = (e as ApiError).message;
		} finally {
			busy = false;
		}
	}
</script>

<div class="space-y-2 rounded-xl border p-4">
	<p class="text-sm font-medium" role="status">
		CD2 状态：{busy
			? '测试中'
			: tested !== fingerprint
				? '尚未测试当前配置'
				: error
					? '连接失败'
					: '已连接'}
	</p>
	<Button
		variant="outline"
		disabled={busy || !url?.trim() || !token?.trim() || !path?.trim()}
		onclick={check}>{busy ? '连接中…' : '测试 CD2 连接'}</Button
	>
	<p class="text-muted-foreground text-xs">
		测试使用上面填写的地址、令牌和目录；测试通过后请保存配置。
	</p>
	{#if message && tested === fingerprint}<p
			class={error ? 'text-destructive text-sm' : 'text-primary text-sm'}
			role="status"
		>
			{message}
		</p>{/if}
</div>
