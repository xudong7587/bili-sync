<script lang="ts">
	import { goto } from '$app/navigation';
	import { toast } from 'svelte-sonner';
	import type { Followed } from '$lib/types';
	interface Props {
		open: boolean;
		item: Followed | null;
		onSuccess: (() => void) | null;
	}
	let { open = $bindable(false), item = null }: Props = $props();
	// Quick subscriptions use the same complete form as manual subscriptions.
	$effect(() => {
		if (!open || !item) return;
		const type =
			item.type === 'upper'
				? 'submissions'
				: item.type === 'favorite'
					? 'favorites'
					: 'collections';
		const params = new URLSearchParams({
			add: type,
			name: item.type === 'upper' ? item.uname : item.title
		});
		if (item.type === 'upper') params.set('mid', String(item.mid));
		else if (item.type === 'favorite') params.set('fid', String(item.fid));
		else {
			params.set('sid', String(item.sid));
			params.set('mid', String(item.mid));
		}
		open = false;
		void goto(`/video-sources?${params}`).catch(() => toast.error('无法打开订阅配置，请重试'));
	});
</script>
