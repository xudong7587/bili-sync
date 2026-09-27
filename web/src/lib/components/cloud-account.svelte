<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import QRCode from 'qrcode';
	import api from '$lib/api';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import type { ApiError } from '$lib/types';
	let clientId = $state('100195125');
	const channels = [
		['alipaymini', '支付宝小程序', '支付宝'],
		['wechatmini', '微信小程序', '微信'],
		['web', '网页版', '115 App'],
		['android', 'Android 版', '115 App'],
		['ios', 'iOS 版', '115 App'],
		['tv', 'TV 版', '115 App'],
		['qandroid', 'Android TV 版', '115 App'],
		['windows', 'Windows 版', '115 App'],
		['mac', 'macOS 版', '115 App'],
		['linux', 'Linux 版', '115 App'],
		['open', '开放平台应用', '115 App']
	];
	let channel = $state('alipaymini');
	let savedChannel = $state('');
	let authorized = $state(false);
	let error = $state('');
	let busy = $state(false);
	let verified = $state(false);
	let login = $state(false);
	let image = $state('');
	let status = $state('');
	let timer: ReturnType<typeof setTimeout> | undefined;
	let disposed = false;
	onMount(() => {
		void poll();
	});
	onDestroy(() => {
		disposed = true;
		clearTimeout(timer);
	});
	async function check() {
		busy = true;
		error = '';
		try {
			await api.cloudAccountCheck();
			authorized = true;
			verified = true;
			status = '115 授权有效';
		} catch (e) {
			verified = false;
			error = (e as ApiError).message;
		} finally {
			busy = false;
		}
	}
	async function poll() {
		try {
			const state = (await api.cloudLoginState()).data;
			if (disposed) return;
			authorized = state.authorized;
			savedChannel = state.channel;
			if (state.running && state.channel) channel = state.channel;
			login = state.running;
			status = state.status;
			error = state.error || '';
			image = state.qrcode ? await QRCode.toDataURL(state.qrcode, { width: 240, margin: 2 }) : '';
			if (state.running) timer = setTimeout(poll, 2000);
		} catch (e) {
			if (!disposed) {
				error = (e as ApiError).message;
				login = false;
			}
		}
	}
	async function start() {
		clearTimeout(timer);
		login = true;
		image = '';
		error = '';
		status = '正在获取 115 二维码…';
		try {
			await api.cloudLoginStart(clientId, channel);
			await poll();
		} catch (e) {
			error = (e as ApiError).message;
			login = false;
		}
	}
</script>

<section class="space-y-4 rounded-xl border p-5">
	<div class="flex items-center justify-between gap-3">
		<h3 class="font-semibold">115 账号</h3>
		<span class="text-muted-foreground text-sm"
			>{authorized
				? `${verified ? '连接正常' : '已保存登录'} · ${channels.find((c) => c[0] === savedChannel)?.[1] || '115'}`
				: '未登录'}</span
		>
	</div>
	<p class="text-muted-foreground text-sm">
		使用 115 扫码，授权独立保存在 bili-sync，用于读取已上传视频。请登录与 CD2 上传目标相同的 115
		账号。
	</p>
	<div class="space-y-2">
		<Label for="p115-channel">登录渠道</Label><select
			id="p115-channel"
			class="h-10 w-full rounded-md border bg-background px-3 text-sm"
			bind:value={channel}
			disabled={login}
			>{#each channels as c (c[0])}<option value={c[0]}>{c[1]}</option>{/each}</select
		>
		<p class="text-muted-foreground text-xs">
			请选择其他服务未使用的渠道。同一渠道的旧会话可能被顶下线；默认支付宝小程序，与 MediaIndex
			的登录方式一致。
		</p>
	</div>

	<div class="flex flex-wrap gap-2">
		<Button disabled={login} onclick={start}>{login ? '等待扫码确认…' : '115 扫码登录'}</Button
		><Button variant="outline" disabled={busy || !authorized} onclick={check}
			>{busy ? '检查中…' : '测试 115 连接'}</Button
		>
	</div>
	{#if channel === 'open'}<details open class="text-sm">
			<summary class="text-muted-foreground cursor-pointer">开放平台应用</summary>
			<div class="mt-3 space-y-2">
				<Label for="p115-app">115 应用 ID</Label><Input
					id="p115-app"
					bind:value={clientId}
					disabled={login}
				/>
				<p class="text-muted-foreground">
					可使用已有的 115 开放平台应用 ID。扫码前请核对手机上显示的应用名称。
				</p>
			</div>
		</details>{/if}
	{#if error}<p role="alert" class="text-destructive text-sm">{error}</p>{/if}
	{#if image}<img
			src={image}
			alt="115 授权二维码"
			width="240"
			height="240"
			class="rounded-lg bg-white"
		/>{/if}
	{#if status}<p role="status" class="text-muted-foreground text-sm">{status}</p>{/if}
</section>
