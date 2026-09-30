<script lang="ts">
	import AppearanceSettings from '$lib/components/appearance-settings.svelte';
	import { setBreadcrumb } from '$lib/stores/breadcrumb';
	setBreadcrumb([{ label: '设置' }, { label: '外观与质感' }]);
</script>

<svelte:head><title>外观与质感 - Bili Sync</title></svelte:head>
<AppearanceSettings />
