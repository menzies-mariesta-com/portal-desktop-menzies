<script lang="ts">
	import { washRecipes } from '@menzies-mariesta-com/menzies-design-wash-ui/core';
	import { m } from '$lib/paraglide/messages.js';
	import {
		gpProfileIconBuiltinIds,
		type GpProfileIcon,
		type GpProfileIconBuiltinId
	} from '$lib/tool/portal-ipc';
	import ProfileIconBadge from '$lib/tool/ProfileIconBadge.svelte';
	import WashIcon from '$lib/tool/WashIcon.svelte';
	import { PROFILE_ICON_COLORS, defaultProfileIcon } from '$lib/tool/profile-icons';
	import { washIcons } from '$lib/tool/wash-icons';

	type Props = {
		open?: boolean;
		value?: GpProfileIcon;
		onSave: (icon: GpProfileIcon) => void;
		onCancel: () => void;
		onPickImage: () => Promise<string | null>;
	};

	let {
		open = $bindable(false),
		value = defaultProfileIcon(),
		onSave,
		onCancel,
		onPickImage
	}: Props = $props();

	let draft = $state<GpProfileIcon>(defaultProfileIcon());
	let picking = $state(false);

	$effect(() => {
		if (open) {
			draft = {
				kind: value.kind,
				builtin: value.builtin,
				color: value.color ?? '',
				imagePath: value.imagePath ?? null
			};
		}
	});

	function selectBuiltin(id: GpProfileIconBuiltinId) {
		draft = {
			...draft,
			kind: 'builtin',
			builtin: id,
			imagePath: null
		};
	}

	function selectColor(color: string) {
		draft = { ...draft, color };
	}

	async function browseImage() {
		picking = true;
		try {
			const path = await onPickImage();
			if (!path) return;
			draft = {
				...draft,
				kind: 'image',
				imagePath: path
			};
		} finally {
			picking = false;
		}
	}

	function clearImage() {
		draft = {
			...draft,
			kind: 'builtin',
			imagePath: null
		};
	}

	function save() {
		onSave({ ...draft });
		open = false;
	}

	function cancel() {
		onCancel();
		open = false;
	}
</script>

{#if open}
	<dialog class="modal modal-open">
		<div class="modal-box flex max-h-[min(90vh,36rem)] max-w-lg flex-col gap-4 overflow-hidden">
			<h2 class="card-title text-primary shrink-0 font-bold">{m.profile_icon_picker_title()}</h2>

			<div class="flex shrink-0 items-center gap-3">
				<div
					class="bg-base-200 rounded-box border-base-300 flex size-12 items-center justify-center border"
				>
					<ProfileIconBadge icon={draft} class="size-7" />
				</div>
				<p class="text-ink-muted text-sm">{m.profile_icon_preview_hint()}</p>
			</div>

			<div class="min-h-0 flex-1 space-y-4 overflow-y-auto overscroll-contain pr-1">
				<section class="space-y-2">
					<p class="font-display text-sm font-semibold">{m.profile_icon_builtin()}</p>
					<div class="grid grid-cols-6 gap-1 sm:grid-cols-8">
						{#each gpProfileIconBuiltinIds as id (id)}
							<div class={washRecipes.tooltipIcon('secondary', 'top')} data-tip={id}>
								<button
									type="button"
									class="btn btn-ghost btn-square btn-sm cursor-pointer"
									class:btn-active={draft.kind === 'builtin' && draft.builtin === id}
									aria-label={id}
									aria-pressed={draft.kind === 'builtin' && draft.builtin === id}
									onclick={() => selectBuiltin(id)}
								>
									<WashIcon icon={washIcons[id]} class="size-4" />
								</button>
							</div>
						{/each}
					</div>
				</section>

				<section class="space-y-2">
					<p class="font-display text-sm font-semibold">{m.profile_icon_color()}</p>
					<div class="flex flex-wrap gap-2">
						{#each PROFILE_ICON_COLORS as swatch (swatch.id)}
							<button
								type="button"
								class="btn btn-sm cursor-pointer gap-2"
								class:btn-primary={draft.color === swatch.value}
								class:btn-ghost={draft.color !== swatch.value}
								disabled={draft.kind === 'image'}
								onclick={() => selectColor(swatch.value)}
							>
								<span
									class="border-base-300 size-3.5 rounded-full border"
									style={swatch.value ? `background-color: ${swatch.value}` : undefined}
									class:bg-base-content={!swatch.value}
								></span>
								{swatch.id === 'default' ? m.profile_icon_color_default() : swatch.label}
							</button>
						{/each}
					</div>
				</section>

				<section class="space-y-2">
					<p class="font-display text-sm font-semibold">{m.profile_icon_image()}</p>
					<div class="flex flex-wrap gap-2">
						<button
							type="button"
							class="btn btn-secondary btn-sm cursor-pointer"
							class:loading={picking}
							disabled={picking}
							aria-busy={picking}
							onclick={browseImage}
						>
							{#if !picking}
								<WashIcon icon={washIcons['folder-open']} class="size-4" />
							{/if}
							{m.profile_icon_browse_image()}
						</button>
						{#if draft.kind === 'image'}
							<button type="button" class="btn btn-ghost btn-sm cursor-pointer" onclick={clearImage}
								>{m.profile_icon_clear_image()}</button
							>
						{/if}
					</div>
					{#if draft.imagePath}
						<p class="text-ink-muted truncate font-mono text-xs">{draft.imagePath}</p>
					{/if}
				</section>
			</div>

			<div class="modal-action shrink-0">
				<button type="button" class="btn cursor-pointer" onclick={cancel}>{m.cancel()}</button>
				<button type="button" class="btn btn-primary cursor-pointer" onclick={save}
					>{m.save()}</button
				>
			</div>
		</div>
		<form method="dialog" class="modal-backdrop">
			<button type="button" class="cursor-pointer" onclick={cancel}>close</button>
		</form>
	</dialog>
{/if}
