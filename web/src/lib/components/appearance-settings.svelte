<script lang="ts">
	import { appearance, accents, materials, setAccent, setMaterial } from '$lib/theme';
	import { mode, setMode } from 'mode-watcher';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import CheckIcon from '@lucide/svelte/icons/check';
	import SunIcon from '@lucide/svelte/icons/sun';
	import MoonIcon from '@lucide/svelte/icons/moon';
	import ArrowRightIcon from '@lucide/svelte/icons/arrow-right';
	let previewName = $state('我的视频库');
	const materialName = $derived(materials.find((item) => item.id === $appearance.material)?.name);
	const accentName = $derived(accents.find((item) => item.id === $appearance.accent)?.name);
	const dark = $derived(mode.current === 'dark');
	function swatch(item: (typeof accents)[number], dark: boolean) {
		const primary = dark ? item.dark : item.light;
		return 'secondary' in item
			? `conic-gradient(from -90deg, ${primary}, ${dark ? item.secondaryDark : item.secondary} 33%, ${dark ? item.tertiaryDark : item.tertiary} 67%, ${primary})`
			: primary;
	}
</script>

<div class="appearance-page mx-auto max-w-6xl space-y-6 pb-8">
	<header class="space-y-2">
		<p class="text-primary text-xs font-semibold tracking-widest">SUNNY UI</p>
		<h1 class="text-2xl font-semibold tracking-tight">外观与质感</h1>
		<p class="text-muted-foreground text-sm">
			配色、材质与明暗自由组合。选择后立即生效，自动保存在当前浏览器。
		</p>
	</header>
	<div class="appearance-layout">
		<div class="appearance-surface appearance-content rounded-2xl border">
			<fieldset class="!mt-0">
				<legend>显示模式</legend>
				<div class="appearance-modes">
					<button
						type="button"
						aria-pressed={mode.current === 'light'}
						onclick={() => setMode('light')}
						><SunIcon class="size-4" />浅色{#if mode.current === 'light'}<CheckIcon
								class="size-4"
							/>{/if}</button
					>
					<button
						type="button"
						aria-pressed={mode.current === 'dark'}
						onclick={() => setMode('dark')}
						><MoonIcon class="size-4" />深色{#if mode.current === 'dark'}<CheckIcon
								class="size-4"
							/>{/if}</button
					>
				</div>
			</fieldset>
			{#each [false, true] as multi (multi)}
				<fieldset>
					<legend>{multi ? '多色搭配' : '主题色'}</legend>
					<div class="appearance-swatches">
						{#each accents.filter((item) => 'secondary' in item === multi) as item (item.id)}
							<button
								type="button"
								aria-pressed={$appearance.accent === item.id}
								onclick={() => setAccent(item.id)}
							>
								<span
									class="appearance-color-wheel"
									style:background={swatch(item, dark)}
									aria-hidden="true"
								></span>
								<span>{item.name}</span>
								{#if $appearance.accent === item.id}<CheckIcon class="size-4 shrink-0" />{/if}
							</button>
						{/each}
					</div>
				</fieldset>
			{/each}
			<fieldset>
				<legend>界面质感</legend>
				<div class="appearance-materials">
					{#each materials as item (item.id)}
						<button
							type="button"
							aria-pressed={$appearance.material === item.id}
							onclick={() => setMaterial(item.id)}
						>
							<span class="appearance-mini appearance-mini-{item.id}" aria-hidden="true"
								><i></i><i></i><i></i></span
							>
							<span><strong>{item.name}</strong><small>{item.description}</small></span>
							{#if $appearance.material === item.id}<CheckIcon class="size-4" />{/if}
						</button>
					{/each}
				</div>
			</fieldset>
		</div>
		<aside class="preview-column min-w-0 space-y-4" aria-label="外观实时预览">
			<div class="appearance-surface rounded-2xl border p-5 space-y-5">
				<div class="flex flex-wrap items-center justify-between gap-2">
					<span class="text-muted-foreground text-xs">组合预览</span>
					<span class="text-primary text-xs font-medium">{accentName} · {materialName}</span>
				</div>
				<div class="space-y-2">
					<h2 class="text-lg font-semibold">让视频库更有你的风格</h2>
					<p class="text-muted-foreground text-sm leading-relaxed">
						卡片、按钮与输入框一起变化，文字和状态保持清晰。
					</p>
				</div>
				<div class="appearance-surface-inner rounded-xl border p-4">
					<div class="flex items-center gap-3">
						<div class="preview-cover shrink-0" aria-hidden="true"><span>PLAY</span></div>
						<div class="min-w-0">
							<strong class="block text-sm">收藏的每一帧</strong><span
								class="text-muted-foreground mt-1 block text-xs">本地视频 · 随时回看</span
							>
						</div>
					</div>
					<div class="mt-4 flex flex-wrap gap-2">
						<span
							class="rounded-full bg-emerald-500/10 px-2.5 py-1 text-xs text-emerald-700 dark:text-emerald-300"
							>已完成</span
						><span
							class="rounded-full bg-amber-500/10 px-2.5 py-1 text-xs text-amber-800 dark:text-amber-300"
							>等待中</span
						>
					</div>
				</div>
				<div class="space-y-2">
					<label for="appearance-preview-name" class="text-sm font-medium">视频库名称</label><Input
						id="appearance-preview-name"
						bind:value={previewName}
					/>
				</div>
				<div class="flex flex-wrap gap-2">
					<Button
						onclick={() => {
							previewName = previewName === '我的视频库' ? '收藏的每一帧' : '我的视频库';
						}}>试试按钮<ArrowRightIcon class="size-4" /></Button
					><Button
						variant="outline"
						onclick={() => {
							previewName = '我的视频库';
						}}>次要操作</Button
					>
				</div>
				<div class="flex flex-wrap gap-2">
					<Button variant="destructive" size="sm" disabled>危险操作</Button><Button
						size="sm"
						disabled>不可用</Button
					>
				</div>
			</div>
			<p class="text-muted-foreground px-1 text-xs leading-relaxed">
				这里是外观预览，按钮不会执行下载或迁移。外观偏好仅保存在当前浏览器。
			</p>
			<a
				class="text-muted-foreground inline-block px-1 text-xs underline underline-offset-4 hover:text-primary"
				href="https://github.com/xudong7587/sunny-ui-design-system"
				target="_blank"
				rel="noreferrer">来自 Sunny UI Design System</a
			>
		</aside>
	</div>
</div>

<style>
	.appearance-layout {
		display: grid;
		gap: 24px;
		grid-template-columns: minmax(0, 1.5fr) minmax(280px, 1fr);
		align-items: start;
	}
	.preview-column {
		position: sticky;
		top: 8px;
	}
	.preview-cover {
		display: grid;
		place-items: center;
		width: 64px;
		height: 78px;
		border-radius: 9px;
		background: linear-gradient(140deg, var(--sunny-accent), var(--appearance-secondary));
		color: var(--primary-foreground);
		box-shadow: inset 0 1px 0 #ffffff40;
	}
	.preview-cover span {
		font-size: 10px;
		font-weight: 700;
		letter-spacing: 2px;
	}
	@media (max-width: 1100px) {
		.appearance-layout {
			grid-template-columns: minmax(0, 1fr);
		}
		.preview-column {
			position: static;
		}
	}
	@media (max-width: 480px) {
		.appearance-page :global(.appearance-materials) {
			grid-template-columns: 1fr;
		}
	}
</style>
