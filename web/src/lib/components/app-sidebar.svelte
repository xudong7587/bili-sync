<script lang="ts">
	import { page } from '$app/state';
	import CloudIcon from '@lucide/svelte/icons/cloud';
	import BotIcon from '@lucide/svelte/icons/bot';
	import ChartPieIcon from '@lucide/svelte/icons/chart-pie';
	import DatabaseIcon from '@lucide/svelte/icons/database';
	import FilePlayIcon from '@lucide/svelte/icons/file-play';
	import FoldersIcon from '@lucide/svelte/icons/folders';
	import HeartIcon from '@lucide/svelte/icons/heart';
	import PaletteIcon from '@lucide/svelte/icons/palette';
	import Settings2Icon from '@lucide/svelte/icons/settings-2';
	import SquareTerminalIcon from '@lucide/svelte/icons/square-terminal';
	import UserIcon from '@lucide/svelte/icons/user';
	import * as Sidebar from '$lib/components/ui/sidebar/index.js';
	import { mode, toggleMode } from 'mode-watcher';
	import type { ComponentProps } from 'svelte';

	let sidebar = Sidebar.useSidebar();

	let { ref = $bindable(null), ...restProps }: ComponentProps<typeof Sidebar.Root> = $props();

	const data = {
		header: {
			title: 'Bili Sync',
			subtitle: '收藏、追更与媒体库',
			icon: BotIcon,
			href: '/'
		},
		navMain: [
			{
				category: '总览',
				items: [
					{
						title: '仪表盘',
						icon: ChartPieIcon,
						href: '/'
					},
					{
						title: '日志',
						icon: SquareTerminalIcon,
						href: '/logs'
					}
				]
			},
			{
				category: '内容管理',
				items: [
					{
						title: '视频',
						icon: FilePlayIcon,
						href: '/videos'
					},
					{
						title: '视频源',
						icon: DatabaseIcon,
						href: '/video-sources'
					}
				]
			},
			{
				category: '快捷订阅',
				items: [
					{
						title: '我创建的收藏夹',
						icon: HeartIcon,
						href: '/me/favorites'
					},
					{
						title: '我追的合集 / 收藏夹',
						icon: FoldersIcon,
						href: '/me/collections'
					},
					{
						title: '我关注的 UP 主',
						icon: UserIcon,
						href: '/me/uppers'
					}
				]
			},
			{
				category: '网盘分流',
				items: [{ title: 'CloudDrive2 · 115', icon: CloudIcon, href: '/settings?section=cloud' }]
			},
			{
				category: '设置',
				items: [
					{ title: '通用与调度', icon: Settings2Icon, href: '/settings?section=basic' },
					{ title: 'B 站账号', icon: UserIcon, href: '/settings?section=auth' },
					{ title: '画质与视频处理', icon: FilePlayIcon, href: '/settings?section=filter' },
					{ title: '弹幕', icon: SquareTerminalIcon, href: '/settings?section=danmaku' },
					{ title: '通知与 Webhook', icon: BotIcon, href: '/settings?section=notifiers' },
					{ title: '外观', icon: PaletteIcon, href: '/settings?section=appearance' },
					{ title: '高级设置', icon: Settings2Icon, href: '/settings?section=advanced' }
				]
			}
		]
	};

	const closeMobileSidebar = () => {
		if (sidebar.isMobile) {
			sidebar.setOpenMobile(false);
		}
	};
</script>

<Sidebar.Root bind:ref variant="inset" {...restProps}>
	<Sidebar.Header>
		<Sidebar.Menu>
			<Sidebar.MenuItem>
				<Sidebar.MenuButton size="lg">
					{#snippet child({ props })}
						<a href={data.header.href} {...props} onclick={closeMobileSidebar}>
							<div
								class="bg-sidebar-primary text-sidebar-primary-foreground flex aspect-square size-8 items-center justify-center rounded-lg"
							>
								<data.header.icon class="size-4" />
							</div>
							<div class="grid flex-1 text-left text-sm leading-tight">
								<span class="truncate font-medium">{data.header.title}</span>
								<span class="truncate text-xs">{data.header.subtitle}</span>
							</div>
						</a>
					{/snippet}
				</Sidebar.MenuButton>
			</Sidebar.MenuItem>
		</Sidebar.Menu>
	</Sidebar.Header>
	<Sidebar.Content>
		<Sidebar.Group>
			{#each data.navMain as group (group.category)}
				<Sidebar.GroupLabel class="h-10">{group.category}</Sidebar.GroupLabel>
				<Sidebar.Menu>
					{#each group.items as item (item.title)}
						<Sidebar.MenuItem>
							<Sidebar.MenuButton
								class="h-9"
								isActive={page.url.pathname + page.url.search === item.href}
							>
								{#snippet child({ props })}
									<a href={item.href} {...props} onclick={closeMobileSidebar}>
										<item.icon class="size-4" />
										<span class="text-sm">{item.title}</span>
									</a>
								{/snippet}
							</Sidebar.MenuButton>
						</Sidebar.MenuItem>
					{/each}
				</Sidebar.Menu>
			{/each}
		</Sidebar.Group>
	</Sidebar.Content>
	<Sidebar.Footer>
		<Sidebar.Separator />
		<Sidebar.Menu>
			<Sidebar.MenuItem>
				<Sidebar.MenuButton class="h-8 cursor-pointer">
					{#snippet child({ props })}
						<button
							{...props}
							onclick={() => {
								toggleMode();
								closeMobileSidebar();
							}}
						>
							<PaletteIcon class="size-4" />
							<span class="text-sm">{mode.current === 'light' ? '亮色' : '暗色'}</span>
						</button>
					{/snippet}
				</Sidebar.MenuButton>
			</Sidebar.MenuItem>
		</Sidebar.Menu>
	</Sidebar.Footer>
</Sidebar.Root>
