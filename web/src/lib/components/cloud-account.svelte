<script lang="ts">
	import { onDestroy } from 'svelte';
	import QRCode from 'qrcode';
	import api from '$lib/api';
	import { Button } from '$lib/components/ui/button';
	import type { ApiError } from '$lib/types';
	let space = $state<{ total: number; used: number; free: number } | null>(null);
	let error = $state('');
	let busy = $state(false);
	let login = $state(false);
	let image = $state('');
	let status = $state('');
	let timer: ReturnType<typeof setTimeout> | undefined;
	let disposed = false;
	onDestroy(() => {
		disposed = true;
		clearTimeout(timer);
	});
	const size = (n: number) => `${(n / 1024 ** 4).toFixed(2)} TB`;
	async function check() {
		busy = true;
		error = '';
		try {
			space = (await api.cloudSpace()).data;
		} catch (e) {
			error = (e as ApiError).message;
		} finally {
			busy = false;
		}
	}
	async function poll() {
		try {
			const state = (await api.cloudLoginState()).data;
			if (disposed) return;
			const qr = [...state.messages]
				.reverse()
				.find((m) => m.message_type === 0 || m.message_type === 1);
			if (qr) {
				if (
					/^data:image\/(png|jpeg|webp);base64,/.test(qr.message) ||
					(qr.message_type === 0 && /^https?:\/\//.test(qr.message))
				)
					image = qr.message;
				else if (/^iVBORw0KGgo/.test(qr.message)) image = `data:image/png;base64,${qr.message}`;
				else image = await QRCode.toDataURL(qr.message, { width: 240, margin: 2 });
			}
			const last = state.messages.at(-1);
			status = last && last.message_type >= 2 ? last.message : '使用 115 扫码并确认授权';
			error = state.error || (last?.message_type === 4 ? last.message : '');
			login = state.running;
			if (state.running) timer = setTimeout(poll, 2000);
			else if (!error) {
				status = '扫码流程已完成，请检查网盘连接';
				await check();
			}
		} catch (e) {
			error = (e as ApiError).message;
			login = false;
		}
	}
	async function start() {
		login = true;
		image = '';
		error = '';
		status = '正在获取二维码…';
		try {
			await api.cloudLoginStart();
			await poll();
		} catch (e) {
			error = (e as ApiError).message;
			login = false;
		}
	}
</script>

<section class="space-y-4 rounded-xl border p-5">
	<div>
		<h3 class="font-semibold">115 账号与连接</h3>
		<p class="text-muted-foreground mt-1 text-sm">
			先保存下方 CD2 配置。扫码通过 CD2 的 115 开放平台登录，令牌需允许管理网盘账号。
		</p>
	</div>
	<div class="flex flex-wrap gap-2">
		<Button variant="outline" disabled={busy} onclick={check}
			>{busy ? '检查中…' : '检查连接与容量'}</Button
		><Button disabled={login} onclick={start}>{login ? '等待扫码…' : '115 扫码登录'}</Button>
	</div>
	{#if error}<p role="alert" class="text-destructive text-sm">{error}</p>{/if}
	{#if image}<img
			src={image}
			alt="115 登录二维码"
			width="240"
			height="240"
			class="rounded-lg bg-white"
			referrerpolicy="no-referrer"
		/>{/if}
	{#if status}<p role="status" class="text-muted-foreground text-sm">{status}</p>{/if}
	{#if space}<div class="grid grid-cols-3 gap-3">
			<div>
				<p class="text-muted-foreground text-xs">总容量</p>
				<p class="font-semibold">{size(space.total)}</p>
			</div>
			<div>
				<p class="text-muted-foreground text-xs">已使用</p>
				<p class="font-semibold">{size(space.used)}</p>
			</div>
			<div>
				<p class="text-muted-foreground text-xs">可用</p>
				<p class="font-semibold">{size(space.free)}</p>
			</div>
		</div>{/if}
</section>
