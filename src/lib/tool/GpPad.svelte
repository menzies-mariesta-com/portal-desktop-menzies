<script lang="ts">
	import { onMount } from 'svelte';
	import { listen } from '@tauri-apps/api/event';
	import { washRecipes } from '@menzies-mariesta-com/menzies-design-wash-ui/core';
	import { m } from '$lib/paraglide/messages.js';
	import WashIcon from '$lib/tool/WashIcon.svelte';
	import WashSelect from '$lib/tool/WashSelect.svelte';
	import ProfileIconBadge from '$lib/tool/ProfileIconBadge.svelte';
	import ProfileIconPicker from '$lib/tool/ProfileIconPicker.svelte';
	import { defaultProfileIcon } from '$lib/tool/profile-icons';
	import { washIcons } from '$lib/tool/wash-icons';
	import {
		clearGpCredentials,
		connectGp,
		deleteGpProfile,
		disconnectGp,
		coerceGpProfileStrings,
		emptyGpProfile,
		gpLogs,
		hasGpStoredCredentials,
		isTauri,
		listGpProfiles,
		MAX_CONNECTION_LOG_BLOCKS,
		pickGpPath,
		renameGpProfile,
		respondGpAuth,
		saveGpProfile,
		type ConnStatus,
		type GpProfile,
		type GpProfileIcon
	} from '$lib/tool/portal-ipc';
	import { parsePortalLogLine } from '$lib/tool/portal-log-line';

	type Props = {
		gpStatus: ConnStatus;
		credentialStore: 'keyring' | 'session';
		onToast: (tone: 'success' | 'error' | 'info' | 'warning', text: string) => void;
		focusProfileId?: string | null;
	};

	let {
		gpStatus = $bindable(),
		credentialStore = 'keyring',
		onToast,
		focusProfileId = null
	}: Props = $props();

	let profiles = $state<GpProfile[]>([]);
	let selectedId = $state<string | null>(null);
	let query = $state('');
	let busy = $state(false);
	let draft = $state<GpProfile | null>(null);
	let advancedOpen = $state(false);
	let detailTab = $state<'performance' | 'log'>('performance');
	let logs = $state<string[]>([]);
	let authOpen = $state(false);
	let authPassword = $state('');
	let authKeyPass = $state('');
	let authRemember = $state(true);
	/** Prefer session when the OS keyring / Secret Service is unreliable (e.g. COSMIC). */
	let authStore = $state<'keyring' | 'session'>('session');
	let challengeOpen = $state(false);
	let challengeText = $state('');
	let challengePassword = $state('');
	let renameValue = $state('');
	let iconPickerOpen = $state(false);
	let iconPickerProfileId = $state<string | null>(null);
	let iconPickerValue = $state<GpProfileIcon>(defaultProfileIcon());

	const selected = $derived(profiles.find((p) => p.id === selectedId) ?? null);
	const filtered = $derived(
		profiles.filter((p) => p.name.toLowerCase().includes(query.trim().toLowerCase()))
	);

	const ownsSession = $derived(
		!!selected && !!gpStatus.profileId && gpStatus.profileId === selected.id
	);
	const isConnected = $derived(ownsSession && gpStatus.phase === 'connected');
	const isConnecting = $derived(
		ownsSession && (gpStatus.phase === 'connecting' || gpStatus.phase === 'reconnecting')
	);
	const isError = $derived(ownsSession && gpStatus.phase === 'error');
	const showDuration = $derived(isConnected || isConnecting);
	const statusLabel = $derived(
		isConnected
			? m.status_connected()
			: isConnecting
				? m.status_connecting()
				: isError
					? m.status_error()
					: m.status_not_connected()
	);
	const statusClass = $derived(
		isConnected
			? 'text-success font-medium'
			: isConnecting
				? 'text-warning font-medium'
				: isError
					? 'text-error font-medium'
					: 'text-ink-muted'
	);

	function formatDuration(startedAtMs: number | null | undefined): string {
		if (!startedAtMs) return '0s';
		const sec = Math.max(0, Math.floor((Date.now() - startedAtMs) / 1000));
		const h = Math.floor(sec / 3600);
		const m_ = Math.floor((sec % 3600) / 60);
		const s = sec % 60;
		if (h > 0) return `${h}h ${m_}m`;
		if (m_ > 0) return `${m_}m ${s}s`;
		return `${s}s`;
	}

	async function refresh() {
		profiles = await listGpProfiles();
		if (focusProfileId && profiles.some((p) => p.id === focusProfileId)) {
			selectedId = focusProfileId;
		} else if (!selectedId && profiles[0]) {
			selectedId = profiles[0].id;
		}
		syncDraft();
		if (detailTab === 'log') {
			logs = await gpLogs();
		}
	}

	/** Plain deep copy. structuredClone fails on Svelte 5 $state proxies. */
	function cloneProfile(profile: GpProfile): GpProfile {
		return coerceGpProfileStrings(JSON.parse(JSON.stringify(profile)) as GpProfile);
	}

	function syncDraft() {
		const p = profiles.find((x) => x.id === selectedId);
		draft = p ? cloneProfile(p) : null;
		renameValue = p?.name ?? '';
	}

	async function onCreate() {
		busy = true;
		try {
			const created = await saveGpProfile(
				emptyGpProfile({ name: m.gp_new_profile(), portal: '', gateway: '', username: '' })
			);
			onToast('success', m.profile_saved());
			profiles = await listGpProfiles();
			selectedId = created.id;
			query = '';
			// Prefer the saved payload so the form opens even if list lag or parse drift.
			const fromList = profiles.find((p) => p.id === created.id);
			draft = cloneProfile(fromList ?? created);
			renameValue = draft.name;
		} catch (err) {
			onToast('error', err instanceof Error ? err.message : String(err));
		} finally {
			busy = false;
		}
	}

	async function onSaveDraft() {
		if (!draft) return;
		busy = true;
		try {
			const saved = await saveGpProfile(draft);
			onToast('success', m.profile_saved());
			await refresh();
			selectedId = saved.id;
			syncDraft();
		} catch (err) {
			onToast('error', err instanceof Error ? err.message : String(err));
		} finally {
			busy = false;
		}
	}

	async function onRename() {
		if (!selected || !renameValue.trim()) return;
		busy = true;
		try {
			await renameGpProfile(selected.id, renameValue.trim());
			onToast('success', m.profile_renamed());
			await refresh();
		} catch (err) {
			onToast('error', err instanceof Error ? err.message : String(err));
		} finally {
			busy = false;
		}
	}

	async function onDelete() {
		if (!selected) return;
		await onDeleteProfile(selected.id);
	}

	async function onDeleteProfile(id: string) {
		if (!window.confirm(m.confirm_delete_gp_profile())) return;
		busy = true;
		try {
			await deleteGpProfile(id);
			if (selectedId === id) {
				selectedId = null;
				draft = null;
			}
			onToast('success', m.profile_deleted());
			await refresh();
		} catch (err) {
			onToast('error', err instanceof Error ? err.message : String(err));
		} finally {
			busy = false;
		}
	}

	function openIconPicker(profile: GpProfile) {
		iconPickerProfileId = profile.id;
		iconPickerValue = {
			kind: profile.icon?.kind ?? 'builtin',
			builtin: profile.icon?.builtin ?? 'globe',
			color: profile.icon?.color ?? '',
			imagePath: profile.icon?.imagePath ?? null
		};
		iconPickerOpen = true;
	}

	async function onSaveProfileIcon(icon: GpProfileIcon) {
		const id = iconPickerProfileId;
		if (!id) return;
		const profile = profiles.find((p) => p.id === id);
		if (!profile) return;
		busy = true;
		try {
			const saved = await saveGpProfile({ ...cloneProfile(profile), icon });
			onToast('success', m.profile_icon_saved());
			await refresh();
			if (selectedId === saved.id) {
				selectedId = saved.id;
				syncDraft();
			}
		} catch (err) {
			onToast('error', err instanceof Error ? err.message : String(err));
		} finally {
			busy = false;
			iconPickerOpen = false;
			iconPickerProfileId = null;
		}
	}

	async function onClearCreds() {
		if (!selected) return;
		busy = true;
		try {
			await clearGpCredentials(selected.id);
			onToast('success', m.credentials_cleared());
		} catch (err) {
			onToast('error', err instanceof Error ? err.message : String(err));
		} finally {
			busy = false;
		}
	}

	async function pickPath(
		category: 'certificate' | 'sslKey' | 'hipScript' | 'cookieCache' | 'iconImage'
	) {
		if (!draft) return;
		const path = await pickGpPath(category);
		if (!path) return;
		if (category === 'certificate') {
			draft.paths.certificate = path;
			draft.certificatePath = path;
		} else if (category === 'sslKey') {
			draft.paths.sslKey = path;
			draft.sslKeyPath = path;
		} else if (category === 'hipScript') {
			draft.paths.hipScript = path;
			draft.hip.scriptPath = path;
		} else {
			draft.paths.cookieCache = path;
			draft.advanced.cookieCachePath = path;
		}
		draft = { ...draft };
	}

	async function beginConnect() {
		if (!draft) return;
		await onSaveDraft();
		const needsPass =
			draft.authMode === 'password' || draft.authMode === 'mixed' || draft.authMode === 'cert';
		const hasCreds = await hasGpStoredCredentials(draft.id);
		if (needsPass && !hasCreds && draft.authMode !== 'browser' && draft.authMode !== 'webview') {
			authPassword = '';
			authKeyPass = '';
			// Session avoids COSMIC / Secret Service DBus timeouts when remembering.
			authStore = credentialStore === 'keyring' ? 'session' : credentialStore;
			authOpen = true;
			return;
		}
		await doConnect();
	}

	async function doConnect(username?: string, password?: string, keyPassphrase?: string) {
		if (!draft) return;
		busy = true;
		try {
			await connectGp({
				profileId: draft.id,
				username: username ?? draft.username ?? undefined,
				password,
				keyPassphrase,
				remember: authRemember,
				store: authRemember ? authStore : credentialStore
			});
			onToast('info', m.connecting());
			authOpen = false;
		} catch (err) {
			onToast('error', err instanceof Error ? err.message : String(err));
		} finally {
			busy = false;
		}
	}

	async function onDisconnect() {
		busy = true;
		try {
			// Backend flips Idle immediately; cap wait so a stuck pkexec cannot freeze the button.
			await Promise.race([
				disconnectGp(),
				new Promise<void>((resolve) => {
					setTimeout(resolve, 5000);
				})
			]);
			onToast('info', m.disconnected());
		} catch (err) {
			onToast('error', err instanceof Error ? err.message : String(err));
		} finally {
			busy = false;
		}
	}

	export function focusProfile(id: string) {
		selectedId = id;
		query = '';
		syncDraft();
		detailTab = 'performance';
	}

	export async function reload() {
		await refresh();
	}

	onMount(() => {
		void refresh();
		let unlistenChallenge: (() => void) | undefined;
		let unlistenLog: (() => void) | undefined;
		if (isTauri()) {
			void listen<{ prompt: string; passwordType: string }>('portal://gp-auth-challenge', (e) => {
				challengeText = e.payload.prompt;
				challengeOpen = true;
			}).then((fn) => {
				unlistenChallenge = fn;
			});
			void listen<string>('portal://gp-log', (event) => {
				if (detailTab !== 'log') return;
				logs = [...logs, event.payload].slice(-MAX_CONNECTION_LOG_BLOCKS);
			}).then((fn) => {
				unlistenLog = fn;
			});
		}
		return () => {
			unlistenChallenge?.();
			unlistenLog?.();
		};
	});

	$effect(() => {
		const id = focusProfileId;
		if (id && id !== selectedId) {
			selectedId = id;
			const p = profiles.find((x) => x.id === id);
			draft = p ? cloneProfile(p) : null;
			renameValue = p?.name ?? '';
		}
	});
</script>

<div class="flex h-full min-h-0 w-full flex-1 flex-col overflow-hidden">
	<div class="flex min-h-0 flex-1 flex-col gap-0 overflow-hidden md:flex-row">
		<aside
			class="border-ink-border/15 bg-base-100/80 flex min-h-0 w-full min-w-0 shrink-0 flex-col overflow-hidden border-b backdrop-blur-sm md:h-full md:w-72 md:border-r md:border-b-0"
		>
			<div class="flex shrink-0 flex-wrap items-center gap-1 p-2">
				<div class={washRecipes.tooltipIcon('primary', 'bottom')} data-tip={m.new_profile()}>
					<button
						type="button"
						class="btn btn-ghost btn-square btn-sm btn-primary cursor-pointer"
						class:loading={busy}
						disabled={busy}
						aria-label={m.new_profile()}
						onclick={onCreate}
					>
						{#if !busy}
							<WashIcon icon={washIcons.plus} class="size-4" />
						{/if}
					</button>
				</div>
				<label class="input input-sm input-bordered ml-auto flex min-w-0 flex-1 items-center gap-1">
					<WashIcon icon={washIcons.search} class="size-3.5 opacity-50" />
					<input
						class="min-w-0 grow cursor-text"
						type="search"
						placeholder={m.search_profiles()}
						bind:value={query}
					/>
				</label>
			</div>
			<ul
				class="menu menu-sm flex min-h-0 w-full min-w-0 flex-1 flex-col flex-nowrap overflow-x-hidden overflow-y-auto overscroll-contain p-1"
			>
				{#if filtered.length === 0}
					<li class="text-base-content/60 w-full px-2 py-4 text-sm">{m.no_gp_profiles()}</li>
				{:else}
					{#each filtered as profile (profile.id)}
						<li class="group/row w-full min-w-0">
							<div
								class="hover:bg-base-200/70 flex w-full min-w-0 items-center gap-0.5 rounded-lg px-1 py-0.5"
								class:bg-base-200={selectedId === profile.id}
							>
								<div
									class={washRecipes.tooltipIcon('secondary', 'right')}
									data-tip={m.profile_change_icon()}
								>
									<button
										type="button"
										class="btn btn-ghost btn-square btn-xs cursor-pointer"
										aria-label={m.profile_change_icon()}
										onclick={(e) => {
											e.stopPropagation();
											openIconPicker(profile);
										}}
									>
										<ProfileIconBadge icon={profile.icon ?? defaultProfileIcon()} class="size-4" />
									</button>
								</div>
								<button
									type="button"
									class="flex min-w-0 flex-1 cursor-pointer items-center overflow-hidden px-1 py-1 text-start"
									onclick={() => {
										selectedId = profile.id;
										syncDraft();
									}}
								>
									<span class="min-w-0 flex-1 truncate">{profile.name}</span>
								</button>
								<div
									class="{washRecipes.tooltipIcon(
										'error',
										'left'
									)} opacity-100 transition-opacity md:opacity-0 md:group-hover/row:opacity-100 md:focus-within:opacity-100"
									data-tip={m.delete()}
								>
									<button
										type="button"
										class="btn btn-ghost btn-square btn-xs btn-error cursor-pointer"
										class:loading={busy}
										disabled={busy}
										aria-label={m.delete()}
										onclick={(e) => {
											e.stopPropagation();
											void onDeleteProfile(profile.id);
										}}
									>
										{#if !busy}
											<WashIcon icon={washIcons['trash-2']} class="size-3.5" />
										{/if}
									</button>
								</div>
							</div>
						</li>
					{/each}
				{/if}
			</ul>
		</aside>

		<section class="bg-base-200/40 flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
			{#if !draft}
				<div class="flex flex-1 flex-col items-center justify-center gap-3 p-8 text-center">
					<div
						class="bg-base-100 rounded-box border-base-300 flex size-14 items-center justify-center border shadow-sm"
					>
						<WashIcon icon={washIcons.globe} class="text-primary size-7" />
					</div>
					<p class="font-display text-lg font-semibold">{m.gp_empty_title()}</p>
					<p class="text-ink-muted max-w-sm text-sm">{m.gp_empty_body()}</p>
					<button
						type="button"
						class="btn btn-primary btn-sm cursor-pointer"
						class:loading={busy}
						disabled={busy}
						onclick={onCreate}
					>
						{#if !busy}
							<WashIcon icon={washIcons.plus} class="size-4" />
						{/if}
						{m.new_profile()}
					</button>
				</div>
			{:else}
				<div
					class="border-ink-border/15 bg-base-100/90 flex shrink-0 flex-wrap items-center gap-2 border-b px-4 py-3 backdrop-blur-sm"
				>
					<div
						class={washRecipes.tooltipIcon('secondary', 'bottom')}
						data-tip={m.profile_change_icon()}
					>
						<button
							type="button"
							class="btn btn-ghost btn-square btn-sm cursor-pointer"
							aria-label={m.profile_change_icon()}
							onclick={() => {
								if (draft) openIconPicker(draft);
							}}
						>
							<ProfileIconBadge icon={draft.icon ?? defaultProfileIcon()} class="size-5" />
						</button>
					</div>
					<div class="min-w-0 flex-1">
						<p class="font-display truncate text-lg font-semibold">{draft.name}</p>
						<p class="text-ink-muted truncate font-mono text-xs">
							{draft.portal.trim() ? draft.portal : m.gp_portal()}
						</p>
					</div>
					{#if isConnected || isConnecting}
						<button
							type="button"
							class="btn btn-error btn-sm cursor-pointer"
							class:loading={busy}
							disabled={busy}
							onclick={onDisconnect}
						>
							{#if !busy}
								<WashIcon icon={washIcons.unplug} class="size-4" />
							{/if}
							{m.disconnect()}
						</button>
					{:else}
						<button
							type="button"
							class="btn btn-primary btn-sm cursor-pointer"
							class:loading={busy}
							disabled={busy || !draft.portal.trim()}
							onclick={beginConnect}
						>
							{#if !busy}
								<WashIcon icon={washIcons.plug} class="size-4" />
							{/if}
							{m.connect()}
						</button>
					{/if}
				</div>

				<div class="min-h-0 flex-1 overflow-y-auto overscroll-contain p-4 sm:p-6">
					<div
						class="mx-auto grid max-w-5xl gap-6 lg:grid-cols-[minmax(0,1fr)_minmax(14rem,18rem)]"
					>
						<form
							class="card border-base-300 bg-base-100 border shadow-sm"
							onsubmit={(e) => {
								e.preventDefault();
								void onSaveDraft();
							}}
						>
							<div class="card-body gap-8">
								<section class="space-y-4">
									<div>
										<h2 class="card-title text-primary font-bold">{m.gp_section_portal()}</h2>
										<p class="text-ink-muted text-sm">{m.gp_section_portal_hint()}</p>
									</div>
									<div class="grid gap-4 sm:grid-cols-2">
										<fieldset class="fieldset sm:col-span-2">
											<label class="label" for="gp-name">
												<span class="label-text"
													>{m.rename_profile()}<span
														class="text-error align-top text-sm leading-none"
														aria-hidden="true">*</span
													></span
												>
											</label>
											<input
												id="gp-name"
												class="input w-full cursor-text"
												bind:value={draft.name}
												required
											/>
										</fieldset>
										<fieldset class="fieldset sm:col-span-2">
											<label class="label" for="gp-portal">
												<span class="label-text"
													>{m.gp_portal()}<span
														class="text-error align-top text-sm leading-none"
														aria-hidden="true">*</span
													></span
												>
											</label>
											<input
												id="gp-portal"
												class="input w-full cursor-text font-mono"
												placeholder="vpn.example.com"
												bind:value={draft.portal}
												required
											/>
										</fieldset>
										<fieldset class="fieldset sm:col-span-2">
											<label class="label" for="gp-gateway">
												<span class="label-text">{m.gp_gateway()}</span>
											</label>
											<input
												id="gp-gateway"
												class="input w-full cursor-text font-mono"
												bind:value={draft.gateway}
												disabled={draft.autoGateway}
											/>
										</fieldset>
										<label
											class="rounded-box border-base-300 bg-base-100 flex cursor-pointer items-center justify-between gap-3 border p-3"
										>
											<span class="text-sm">{m.gp_auto_gateway()}</span>
											<input
												type="checkbox"
												class="toggle toggle-primary toggle-sm"
												checked={draft.autoGateway}
												onchange={(e) => {
													if (!draft) return;
													const on = (e.currentTarget as HTMLInputElement).checked;
													draft.autoGateway = on;
													if (on) draft.asGateway = false;
												}}
											/>
										</label>
										<label
											class="rounded-box border-base-300 bg-base-100 flex cursor-pointer items-center justify-between gap-3 border p-3"
										>
											<span class="text-sm">{m.gp_as_gateway()}</span>
											<input
												type="checkbox"
												class="toggle toggle-primary toggle-sm"
												checked={draft.asGateway}
												onchange={(e) => {
													if (!draft) return;
													const on = (e.currentTarget as HTMLInputElement).checked;
													draft.asGateway = on;
													if (on) draft.autoGateway = false;
												}}
											/>
										</label>
									</div>
								</section>

								<section class="space-y-4">
									<div>
										<h2 class="font-display text-base font-semibold">{m.gp_section_auth()}</h2>
										<p class="text-ink-muted text-sm">{m.gp_section_auth_hint()}</p>
										<p class="text-ink-muted text-sm">{m.gp_auth_microsoft_hint()}</p>
									</div>
									<fieldset class="fieldset">
										<label class="label" for="gp-user">
											<span class="label-text">{m.username()}</span>
										</label>
										<input
											id="gp-user"
											class="input w-full cursor-text"
											bind:value={draft.username}
											autocomplete="username"
										/>
									</fieldset>
									<div class="space-y-2" role="radiogroup" aria-label={m.gp_auth_mode()}>
										{#each [{ value: 'password' as const, label: m.gp_auth_password() }, { value: 'browser' as const, label: m.gp_auth_browser() }, { value: 'webview' as const, label: m.gp_auth_webview() }, { value: 'cert' as const, label: m.gp_auth_cert() }, { value: 'mixed' as const, label: m.gp_auth_mixed() }] as mode (mode.value)}
											<label
												class="rounded-box border-base-300 bg-base-100 has-checked:border-primary has-checked:bg-primary/5 flex cursor-pointer items-center gap-3 border p-3"
											>
												<input
													type="radio"
													name="gp-auth-mode"
													class="radio radio-primary"
													value={mode.value}
													checked={draft.authMode === mode.value}
													onchange={() => {
														if (draft) draft.authMode = mode.value;
													}}
												/>
												<span class="flex-1 text-sm font-medium">{mode.label}</span>
											</label>
										{/each}
									</div>
									{#if draft.authMode === 'browser' || draft.authMode === 'mixed'}
										<fieldset class="fieldset">
											<label class="label" for="gp-browser">
												<span class="label-text">{m.gp_browser()}</span>
											</label>
											<input
												id="gp-browser"
												class="input w-full cursor-text"
												placeholder="auto"
												bind:value={draft.browser}
											/>
										</fieldset>
									{/if}
								</section>

								<section class="space-y-4">
									<div>
										<h2 class="font-display text-base font-semibold">{m.gp_section_certs()}</h2>
										<p class="text-ink-muted text-sm">{m.gp_section_certs_hint()}</p>
										<p class="text-ink-muted text-sm">{m.gp_certs_skip_hint()}</p>
									</div>
									<div class="flex flex-wrap items-end gap-2">
										<fieldset class="fieldset min-w-0 flex-1">
											<label class="label" for="gp-cert">
												<span class="label-text">{m.gp_certificate()}</span>
											</label>
											<input
												id="gp-cert"
												class="input w-full cursor-text font-mono"
												bind:value={draft.paths.certificate}
											/>
										</fieldset>
										<button
											type="button"
											class="btn btn-ghost btn-sm cursor-pointer"
											onclick={() => pickPath('certificate')}>{m.gp_pick_path()}</button
										>
									</div>
									<div class="flex flex-wrap items-end gap-2">
										<fieldset class="fieldset min-w-0 flex-1">
											<label class="label" for="gp-key">
												<span class="label-text">{m.gp_ssl_key()}</span>
											</label>
											<input
												id="gp-key"
												class="input w-full cursor-text font-mono"
												bind:value={draft.paths.sslKey}
											/>
										</fieldset>
										<button
											type="button"
											class="btn btn-ghost btn-sm cursor-pointer"
											onclick={() => pickPath('sslKey')}>{m.gp_pick_path()}</button
										>
									</div>
								</section>

								<section class="rounded-box border-base-300 bg-base-200/30 space-y-3 border p-4">
									<p class="font-display text-sm font-semibold">{m.gp_hip()}</p>
									<label class="flex cursor-pointer items-center justify-between gap-3 text-sm">
										<span>{m.gp_hip_enabled()}</span>
										<input
											type="checkbox"
											class="toggle toggle-primary toggle-sm"
											bind:checked={draft.hip.enabled}
										/>
									</label>
									{#if draft.hip.enabled}
										<div class="flex flex-wrap items-end gap-2">
											<fieldset class="fieldset min-w-0 flex-1">
												<label class="label" for="gp-hip-script">
													<span class="label-text">{m.gp_hip_script()}</span>
												</label>
												<input
													id="gp-hip-script"
													class="input w-full cursor-text font-mono"
													bind:value={draft.hip.scriptPath}
												/>
											</fieldset>
											<button
												type="button"
												class="btn btn-ghost btn-sm cursor-pointer"
												onclick={() => pickPath('hipScript')}>{m.gp_pick_path()}</button
											>
										</div>
										<fieldset class="fieldset">
											<label class="label" for="gp-hip-user">
												<span class="label-text">{m.gp_hip_user()}</span>
											</label>
											<input
												id="gp-hip-user"
												class="input w-full cursor-text"
												bind:value={draft.hip.hipUser}
											/>
										</fieldset>
									{/if}
								</section>

								<details
									class="rounded-box border-base-300 bg-base-100 border"
									bind:open={advancedOpen}
								>
									<summary
										class="font-display cursor-pointer list-none px-4 py-3 text-sm font-semibold marker:content-none"
									>
										<span class="flex items-center gap-2">
											<WashIcon icon={washIcons.settings} class="size-4 opacity-70" />
											{m.gp_advanced()}
										</span>
									</summary>
									<div class="border-base-300 space-y-3 border-t px-4 py-4">
										<fieldset class="fieldset">
											<label class="label" for="gp-mtu"
												><span class="label-text">{m.gp_mtu()}</span></label
											>
											<input
												id="gp-mtu"
												type="number"
												class="input w-full cursor-text"
												value={draft.advanced.mtu ?? ''}
												oninput={(e) => {
													const v = (e.currentTarget as HTMLInputElement).value;
													if (draft) draft.advanced.mtu = v ? Number(v) : null;
												}}
											/>
										</fieldset>
										<label class="flex cursor-pointer items-center justify-between gap-2 text-sm">
											<span>{m.gp_no_dtls()}</span>
											<input
												type="checkbox"
												class="toggle toggle-sm"
												bind:checked={draft.advanced.noDtls}
											/>
										</label>
										<label class="flex cursor-pointer items-center justify-between gap-2 text-sm">
											<span>{m.gp_disable_ipv6()}</span>
											<input
												type="checkbox"
												class="toggle toggle-sm"
												bind:checked={draft.advanced.disableIpv6}
											/>
										</label>
										<fieldset class="fieldset">
											<label class="label" for="gp-os"
												><span class="label-text">{m.gp_os()}</span></label
											>
											<input
												id="gp-os"
												class="input w-full cursor-text"
												bind:value={draft.advanced.os}
											/>
										</fieldset>
										<fieldset class="fieldset">
											<label class="label" for="gp-cv"
												><span class="label-text">{m.gp_client_version()}</span></label
											>
											<input
												id="gp-cv"
												class="input w-full cursor-text"
												bind:value={draft.advanced.clientVersion}
											/>
										</fieldset>
										<fieldset class="fieldset">
											<label class="label" for="gp-iface"
												><span class="label-text">{m.gp_interface()}</span></label
											>
											<input
												id="gp-iface"
												class="input w-full cursor-text font-mono"
												bind:value={draft.advanced.interface}
											/>
										</fieldset>
										<fieldset class="fieldset">
											<label class="label" for="gp-rt"
												><span class="label-text">{m.gp_reconnect_timeout()}</span></label
											>
											<input
												id="gp-rt"
												type="number"
												class="input w-full cursor-text"
												bind:value={draft.advanced.reconnectTimeout}
											/>
										</fieldset>
										<label class="flex cursor-pointer items-center justify-between gap-2 text-sm">
											<span>{m.gp_no_xmlpost()}</span>
											<input
												type="checkbox"
												class="toggle toggle-sm"
												bind:checked={draft.advanced.noXmlpost}
											/>
										</label>
										<div>
											<span class="label-text mb-1 block">{m.gp_csc()}</span>
											<WashSelect
												value={draft.advanced.csc}
												options={[
													{ value: 'auto', label: 'auto' },
													{ value: 'yes', label: 'yes' },
													{ value: 'no', label: 'no' }
												]}
												onchange={(v) => {
													if (draft) draft.advanced.csc = v as 'auto' | 'yes' | 'no';
												}}
											/>
										</div>
										<fieldset class="fieldset">
											<label class="label" for="gp-lh"
												><span class="label-text">{m.gp_local_hostname()}</span></label
											>
											<input
												id="gp-lh"
												class="input w-full cursor-text"
												bind:value={draft.advanced.localHostname}
											/>
										</fieldset>
										<fieldset class="fieldset">
											<label class="label" for="gp-dpd"
												><span class="label-text">{m.gp_dpd()}</span></label
											>
											<input
												id="gp-dpd"
												type="number"
												class="input w-full cursor-text"
												value={draft.advanced.dpdInterval ?? ''}
												oninput={(e) => {
													const v = (e.currentTarget as HTMLInputElement).value;
													if (draft) draft.advanced.dpdInterval = v ? Number(v) : null;
												}}
											/>
										</fieldset>
										<label class="flex cursor-pointer items-center justify-between gap-2 text-sm">
											<span>{m.gp_clean_cookie()}</span>
											<input
												type="checkbox"
												class="toggle toggle-sm"
												bind:checked={draft.advanced.cleanCookie}
											/>
										</label>
										<div class="flex flex-wrap items-end gap-2">
											<fieldset class="fieldset min-w-0 flex-1">
												<label class="label" for="gp-ccp"
													><span class="label-text">{m.gp_cookie_cache_path()}</span></label
												>
												<input
													id="gp-ccp"
													class="input w-full cursor-text font-mono"
													bind:value={draft.advanced.cookieCachePath}
												/>
											</fieldset>
											<button
												type="button"
												class="btn btn-ghost btn-sm cursor-pointer"
												onclick={() => pickPath('cookieCache')}>{m.gp_pick_path()}</button
											>
										</div>
										<fieldset class="fieldset">
											<label class="label" for="gp-script"
												><span class="label-text">{m.gp_script()}</span></label
											>
											<input
												id="gp-script"
												class="input w-full cursor-text font-mono"
												bind:value={draft.advanced.script}
											/>
										</fieldset>
										<label class="flex cursor-pointer items-center justify-between gap-2 text-sm">
											<span>{m.gp_script_tun()}</span>
											<input
												type="checkbox"
												class="toggle toggle-sm"
												bind:checked={draft.advanced.scriptTun}
											/>
										</label>
									</div>
								</details>

								<div class="border-base-300 flex flex-wrap gap-2 border-t pt-4">
									<button
										type="submit"
										class="btn btn-secondary cursor-pointer"
										class:loading={busy}
										disabled={busy}
										aria-busy={busy}>{m.save()}</button
									>
									<button
										type="button"
										class="btn cursor-pointer"
										disabled={busy}
										onclick={() => {
											renameValue = draft?.name ?? '';
											void onRename();
										}}>{m.rename()}</button
									>
									<button
										type="button"
										class="btn btn-error btn-outline cursor-pointer"
										disabled={busy}
										onclick={onDelete}>{m.delete()}</button
									>
									<button
										type="button"
										class="btn btn-ghost cursor-pointer"
										disabled={busy}
										onclick={onClearCreds}>{m.clear_credentials()}</button
									>
								</div>
							</div>
						</form>

						<aside class="flex h-fit flex-col gap-4 lg:sticky lg:top-0">
							<div class="card border-base-300 bg-base-100 border shadow-sm">
								<div class="card-body gap-4">
									<div class="flex items-center gap-2">
										<WashIcon icon={washIcons.gauge} class="text-primary size-5" />
										<h2 class="card-title text-secondary font-bold">{m.gp_section_session()}</h2>
									</div>
									<div class="space-y-3 text-sm">
										<div class="bg-base-200/50 rounded-box p-3">
											<p class="text-ink-muted text-xs">{m.status_label()}</p>
											<p class={statusClass}>{statusLabel}</p>
											{#if ownsSession && gpStatus.message.trim()}
												<p class="text-ink-muted mt-1 text-xs whitespace-pre-wrap">
													{gpStatus.message}
												</p>
											{/if}
										</div>
										<div class="bg-base-200/50 rounded-box p-3">
											<p class="text-ink-muted text-xs">{m.duration_label()}</p>
											<p class="font-mono">
												{showDuration ? formatDuration(gpStatus.startedAtMs) : '0s'}
											</p>
										</div>
										<div class="bg-base-200/50 rounded-box p-3">
											<p class="text-ink-muted text-xs">{m.vpn_ip_label()}</p>
											<p class="font-mono text-xs break-all">
												{isConnected ? (gpStatus.vpnIp ?? m.not_available()) : m.not_available()}
											</p>
										</div>
									</div>
									{#if isConnected || isConnecting}
										<button
											type="button"
											class="btn btn-error w-full cursor-pointer"
											class:loading={busy}
											disabled={busy}
											onclick={onDisconnect}
										>
											{#if !busy}
												<WashIcon icon={washIcons.unplug} class="size-4" />
											{/if}
											{m.disconnect()}
										</button>
									{:else}
										<button
											type="button"
											class="btn btn-primary w-full cursor-pointer"
											class:loading={busy}
											disabled={busy || !draft.portal.trim()}
											onclick={beginConnect}
										>
											{#if !busy}
												<WashIcon icon={washIcons.plug} class="size-4" />
											{/if}
											{m.connect()}
										</button>
									{/if}
								</div>
							</div>

							<div class="card border-base-300 bg-base-100 border shadow-sm">
								<div class="card-body gap-3">
									<div class="tabs tabs-boxed">
										<button
											type="button"
											class="tab cursor-pointer"
											class:tab-active={detailTab === 'performance'}
											onclick={() => (detailTab = 'performance')}>{m.tab_performance()}</button
										>
										<button
											type="button"
											class="tab cursor-pointer"
											class:tab-active={detailTab === 'log'}
											onclick={async () => {
												detailTab = 'log';
												logs = await gpLogs();
											}}>{m.tab_connection_log()}</button
										>
									</div>
									{#if detailTab === 'log'}
										<pre
											class="bg-base-200 rounded-box max-h-56 overflow-auto p-3 font-mono text-xs whitespace-pre-wrap">{#if logs.length === 0}{m.logs_empty()}{:else}{#each logs as line, i (i)}{parsePortalLogLine(
														line
													)}
												{/each}{/if}</pre>
									{:else}
										<p class="text-ink-muted text-sm">{m.perf_idle()}</p>
									{/if}
								</div>
							</div>
						</aside>
					</div>
				</div>
			{/if}
		</section>
	</div>

	<ProfileIconPicker
		bind:open={iconPickerOpen}
		value={iconPickerValue}
		onSave={onSaveProfileIcon}
		onCancel={() => {
			iconPickerOpen = false;
			iconPickerProfileId = null;
		}}
		onPickImage={() => pickGpPath('iconImage')}
	/>

	{#if authOpen}
		<dialog class="modal modal-open">
			<div class="modal-box max-w-md">
				<h2 class="card-title text-primary font-bold">{m.auth_title()}</h2>
				<label class="form-control mt-3 w-full">
					<span class="label-text">{m.password()}</span>
					<input
						type="password"
						class="input input-bordered w-full cursor-text"
						bind:value={authPassword}
					/>
				</label>
				<label class="form-control mt-2 w-full">
					<span class="label-text">{m.key_passphrase()}</span>
					<input
						type="password"
						class="input input-bordered w-full cursor-text"
						bind:value={authKeyPass}
					/>
				</label>
				<label class="mt-2 flex cursor-pointer items-center gap-2 text-sm">
					<input type="checkbox" class="checkbox checkbox-sm" bind:checked={authRemember} />
					{m.remember_credentials()}
				</label>
				{#if authRemember}
					<div class="mt-2 flex flex-wrap gap-3 text-sm">
						<label class="flex cursor-pointer items-center gap-2">
							<input
								type="radio"
								class="radio radio-sm"
								checked={authStore === 'session'}
								onchange={() => (authStore = 'session')}
							/>
							{m.store_session()}
						</label>
						<label class="flex cursor-pointer items-center gap-2">
							<input
								type="radio"
								class="radio radio-sm"
								checked={authStore === 'keyring'}
								onchange={() => (authStore = 'keyring')}
							/>
							{m.store_keyring()}
						</label>
					</div>
				{/if}
				<div class="modal-action">
					<button type="button" class="btn cursor-pointer" onclick={() => (authOpen = false)}
						>{m.cancel()}</button
					>
					<button
						type="button"
						class="btn btn-primary cursor-pointer"
						class:loading={busy}
						disabled={busy}
						onclick={() => doConnect(draft?.username ?? undefined, authPassword, authKeyPass)}
						>{m.connect()}</button
					>
				</div>
			</div>
			<form method="dialog" class="modal-backdrop">
				<button type="button" class="cursor-pointer" onclick={() => (authOpen = false)}
					>close</button
				>
			</form>
		</dialog>
	{/if}

	{#if challengeOpen}
		<dialog class="modal modal-open">
			<div class="modal-box max-w-md">
				<h2 class="card-title text-primary font-bold">{m.challenge_title()}</h2>
				<p class="text-ink-muted mt-2 text-sm whitespace-pre-wrap">{challengeText}</p>
				<label class="form-control mt-3 w-full">
					<span class="label-text">{m.challenge_response()}</span>
					<input
						type="password"
						class="input input-bordered w-full cursor-text"
						bind:value={challengePassword}
					/>
				</label>
				<div class="modal-action">
					<button type="button" class="btn cursor-pointer" onclick={() => (challengeOpen = false)}
						>{m.cancel()}</button
					>
					<button
						type="button"
						class="btn btn-primary cursor-pointer"
						class:loading={busy}
						disabled={busy}
						onclick={async () => {
							busy = true;
							try {
								await respondGpAuth('Auth', challengePassword);
								challengeOpen = false;
								challengePassword = '';
							} catch (err) {
								onToast('error', err instanceof Error ? err.message : String(err));
							} finally {
								busy = false;
							}
						}}>{m.submit()}</button
					>
				</div>
			</div>
			<form method="dialog" class="modal-backdrop">
				<button type="button" class="cursor-pointer" onclick={() => (challengeOpen = false)}
					>close</button
				>
			</form>
		</dialog>
	{/if}
</div>
