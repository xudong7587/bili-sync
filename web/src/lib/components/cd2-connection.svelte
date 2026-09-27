<script lang="ts">
	import api from '$lib/api';
	import { Button } from '$lib/components/ui/button';
	import type { ApiError } from '$lib/types';
	let busy = $state(false);
	let message = $state('');
	let error = $state(false);
	async function check() {
		busy = true;
		message = '';
		error = false;
		try {
			const s = (await api.cloudSpace()).data;
			message = `CD2 已连接 · 可用 ${(s.free / 1024 ** 4).toFixed(2)} TB / 共 ${(s.total / 1024 ** 4).toFixed(2)} TB`;
		} catch (e) {
			error = true;
			message = (e as ApiError).message;
		} finally {
			busy = false;
		}
	}
</script>

<div class="space-y-2">
	<Button variant="outline" disabled={busy} onclick={check}
		>{busy ? '连接中…' : '检查 CD2 连接'}</Button
	>
	<p class="text-muted-foreground text-xs">请先保存上面的地址、API 令牌和目录，再检查连接。</p>
	{#if message}<p class={error ? 'text-destructive text-sm' : 'text-primary text-sm'} role="status">
			{message}
		</p>{/if}
</div>
