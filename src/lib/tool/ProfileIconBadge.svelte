<script lang="ts">
	import type { GpProfileIcon } from '$lib/tool/portal-ipc';
	import WashIcon from '$lib/tool/WashIcon.svelte';
	import { profileBuiltinIconNode, profileIconImageSrc } from '$lib/tool/profile-icons';

	type Props = {
		icon: GpProfileIcon;
		class?: string;
	};

	let { icon, class: className = 'size-4' }: Props = $props();

	const imageSrc = $derived(icon.kind === 'image' ? profileIconImageSrc(icon.imagePath) : null);
</script>

{#if imageSrc}
	<img
		src={imageSrc}
		alt=""
		class="shrink-0 rounded-sm object-cover {className}"
		draggable="false"
	/>
{:else}
	<span
		class="inline-flex shrink-0 items-center justify-center {className}"
		style={icon.color ? `color: ${icon.color}` : undefined}
	>
		<WashIcon icon={profileBuiltinIconNode(icon.builtin)} class="size-full" />
	</span>
{/if}
