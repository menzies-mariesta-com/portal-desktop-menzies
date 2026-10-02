<script lang="ts">
	import { onMount } from 'svelte';
	import { listen } from '@tauri-apps/api/event';
	import { open } from '@tauri-apps/plugin-dialog';
	import { washRecipes } from '@menzies-mariesta-com/menzies-design-wash-ui/core';
	import { m } from '$lib/paraglide/messages.js';
	import WashIcon from '$lib/tool/WashIcon.svelte';
	import WashSelect from '$lib/tool/WashSelect.svelte';
	import { washIcons } from '$lib/tool/wash-icons';
	import { subscribePortalRefresh } from '$lib/tool/portal-refresh';
	import { subscribeFocusConnected } from '$lib/tool/portal-focus';
	import {
		clearProfileCredentials,
		connectVpn,
		connectionStatus,
		deleteProfile,
		disconnectVpn,
		hasStoredCredentials,
		importOvpnFile,
		importOvpnFolder,
		isTauri,
		listProfiles,
		loadPortalSettings,
		MAX_CONNECTION_LOG_BLOCKS,
		portalLogs,
		renameProfile,
		respondAuthChallenge,
		savePortalSettings,
		type ConnStatus,
		type PortalDiskSettings,
		type ProfileSummary
	} from '$lib/tool/portal-ipc';
	import { loadSettings, saveSettings } from '$lib/store/local-storage/settings';
	import { parsePortalLogLine } from '$lib/tool/portal-log-line';

	let profiles = $state<ProfileSummary[]>([]);
	let selectedId = $state<string | null>(null);
	let query = $state('');
	let status = $state<ConnStatus>({ phase: 'idle', message: '' });
	let logs = $state<string[]>([]);
	let settings = $state<PortalDiskSettings | null>(null);
	let busy = $state(false);
	let toast = $state<{ tone: 'success' | 'error' | 'info' | 'warning'; text: string } | null>(null);
	let toastTimer: ReturnType<typeof setTimeout> | null = null;

	let authOpen = $state(false);
	let authUsername = $state('');
	let authPassword = $state('');
	let authKeyPass = $state('');
	let authRemember = $state(true);
	let authStore = $state<'keyring' | 'session'>('keyring');
	let authProfileId = $state<string | null>(null);
	let switchOpen = $state(false);
	let switchTarget = $state<ProfileSummary | null>(null);
	let challengeOpen = $state(false);
	let challengeText = $state('');
	let challengePassword = $state('');
	let settingsOpen = $state(false);
	let onboarded = $state(loadSettings().onboarded);
	let renameValue = $state('');
	let reconnectArmed = $state(false);
	let detailTab = $state<'performance' | 'log'>('performance');
	let downBps = $state(0);
	let upBps = $state(0);
	let bytesIn = $state(0);
	let bytesOut = $state(0);

	const selected = $derived(profiles.find((p) => p.id === selectedId) ?? null);
	const filtered = $derived(
		profiles.filter((p) => p.name.toLowerCase().includes(query.trim().toLowerCase()))
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

	const isActiveSession = $derived(status.phase === 'connected' || status.phase === 'connecting');
	const selectedNeedsSwitch = $derived(
		!!selected && isActiveSession && !!status.profileId && status.profileId !== selected.id
	);
	const selectedOwnsSession = $derived(
		!!selected && !!status.profileId && status.profileId === selected.id
	);
	const selectedIsConnected = $derived(selectedOwnsSession && status.phase === 'connected');
	const selectedIsConnecting = $derived(
		selectedOwnsSession && (status.phase === 'connecting' || status.phase === 'reconnecting')
	);
	const connectedProfileName = $derived(
		profiles.find((p) => p.id === status.profileId)?.name ?? m.switch_profile_unknown()
	);
	const profileStatusLabel = $derived(
		selectedIsConnected
			? m.status_connected()
			: selectedIsConnecting
				? m.status_connecting()
				: m.status_not_connected()
	);
	const profileStatusClass = $derived(
		selectedIsConnected
			? 'text-success'
			: selectedIsConnecting
				? 'text-warning'
				: 'text-base-content/50'
	);
	const profileDurationText = $derived(
		selectedOwnsSession && (selectedIsConnected || selectedIsConnecting)
			? formatDuration(status.startedAtMs)
			: '0s'
	);
	const profileVpnIpText = $derived(
		selectedIsConnected ? (status.vpnIp ?? m.not_available()) : m.not_available()
	);

	function showToast(tone: 'success' | 'error' | 'info' | 'warning', text: string) {
		if (toastTimer) clearTimeout(toastTimer);
		toast = { tone, text };
		toastTimer = setTimeout(() => {
			toast = null;
			toastTimer = null;
		}, 5000);
	}

	function formatRate(bps: number): string {
		if (!Number.isFinite(bps) || bps < 0) return '0 B/s';
		const units = ['B/s', 'KB/s', 'MB/s', 'GB/s'];
		let v = bps;
		let i = 0;
		while (v >= 1024 && i < units.length - 1) {
			v /= 1024;
			i += 1;
		}
		return `${v < 10 && i > 0 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
	}

	function formatBytes(n: number): string {
		if (!Number.isFinite(n) || n < 0) return '0 B';
		const units = ['B', 'KB', 'MB', 'GB'];
		let v = n;
		let i = 0;
		while (v >= 1024 && i < units.length - 1) {
			v /= 1024;
			i += 1;
		}
		return `${v < 10 && i > 0 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
	}

	function focusConnectedProfile() {
		if (!status.profileId) return;
		selectedId = status.profileId;
		query = '';
		detailTab = 'performance';
	}

	async function openLogTab() {
		detailTab = 'log';
		try {
			logs = await portalLogs();
		} catch {
			/* keep current */
		}
	}

	async function refreshAll() {
		try {
			profiles = await listProfiles();
			status = await connectionStatus();
			if (detailTab === 'log') {
				logs = await portalLogs();
			}
			settings = await loadPortalSettings();
			if (settings) {
				authStore = settings.credentialStore;
			}
			if (!selectedId && profiles[0]) selectedId = profiles[0].id;
		} catch (err) {
			showToast('error', err instanceof Error ? err.message : String(err));
		}
	}

	async function onImportFile() {
		if (!isTauri()) {
			showToast('warning', m.desktop_required());
			return;
		}
		busy = true;
		try {
			const picked = await open({
				multiple: false,
				filters: [{ name: 'OpenVPN', extensions: ['ovpn', 'conf'] }]
			});
			if (!picked || typeof picked !== 'string') return;
			const profile = await importOvpnFile(picked);
			showToast('success', m.import_ok({ name: profile.name }));
			await refreshAll();
			selectedId = profile.id;
		} catch (err) {
			showToast('error', err instanceof Error ? err.message : String(err));
		} finally {
			busy = false;
		}
	}

	async function onImportFolder() {
		if (!isTauri()) {
			showToast('warning', m.desktop_required());
			return;
		}
		busy = true;
		try {
			const picked = await open({ directory: true, multiple: false });
			if (!picked || typeof picked !== 'string') return;
			const list = await importOvpnFolder(picked);
			showToast('success', m.import_folder_ok({ count: String(list.length) }));
			await refreshAll();
		} catch (err) {
			showToast('error', err instanceof Error ? err.message : String(err));
		} finally {
			busy = false;
		}
	}

	async function beginConnect(profile: ProfileSummary) {
		if (!isTauri()) {
			showToast('warning', m.desktop_required());
			return;
		}
		authProfileId = profile.id;
		authUsername = '';
		authPassword = '';
		authKeyPass = '';
		authRemember = true;
		const has = await hasStoredCredentials(profile.id);
		if (profile.needsUserPass && !has) {
			authOpen = true;
			return;
		}
		if (profile.needsKeyPassphrase) {
			authOpen = true;
			return;
		}
		await doConnect(profile.id);
	}

	function requestConnect(profile: ProfileSummary) {
		if (
			(status.phase === 'connected' || status.phase === 'connecting') &&
			status.profileId &&
			status.profileId !== profile.id
		) {
			switchTarget = profile;
			switchOpen = true;
			return;
		}
		void beginConnect(profile);
	}

	async function waitUntilIdle(timeoutMs = 10000): Promise<void> {
		const started = Date.now();
		while (Date.now() - started < timeoutMs) {
			const next = await connectionStatus();
			status = next;
			if (next.phase === 'idle' || next.phase === 'error') return;
			await new Promise((resolve) => setTimeout(resolve, 100));
		}
	}

	async function onConfirmSwitch() {
		const target = switchTarget;
		if (!target || busy) return;
		busy = true;
		try {
			await disconnectVpn();
			await waitUntilIdle();
			showToast('success', m.disconnected());
			await refreshAll();
			switchOpen = false;
			switchTarget = null;
		} catch (err) {
			showToast('error', err instanceof Error ? err.message : String(err));
			return;
		} finally {
			busy = false;
		}
		await beginConnect(target);
	}

	function onCancelSwitch() {
		if (busy) return;
		switchOpen = false;
		switchTarget = null;
	}

	async function doConnect(profileId: string) {
		busy = true;
		try {
			await connectVpn({
				profileId,
				username: authUsername || undefined,
				password: authPassword || undefined,
				keyPassphrase: authKeyPass || undefined,
				remember: authRemember,
				store: authStore
			});
			authOpen = false;
			showToast('info', m.connecting());
			await refreshAll();
		} catch (err) {
			showToast('error', err instanceof Error ? err.message : String(err));
		} finally {
			busy = false;
		}
	}

	async function onDisconnect() {
		busy = true;
		try {
			await disconnectVpn();
			showToast('success', m.disconnected());
			await refreshAll();
		} catch (err) {
			showToast('error', err instanceof Error ? err.message : String(err));
		} finally {
			busy = false;
		}
	}

	async function onDelete(id: string) {
		if (!confirm(m.confirm_delete_profile())) return;
		busy = true;
		try {
			await deleteProfile(id);
			if (selectedId === id) selectedId = null;
			showToast('success', m.profile_deleted());
			await refreshAll();
		} catch (err) {
			showToast('error', err instanceof Error ? err.message : String(err));
		} finally {
			busy = false;
		}
	}

	async function onRename() {
		if (!selected || !renameValue.trim()) return;
		busy = true;
		try {
			await renameProfile(selected.id, renameValue.trim());
			showToast('success', m.profile_renamed());
			await refreshAll();
		} catch (err) {
			showToast('error', err instanceof Error ? err.message : String(err));
		} finally {
			busy = false;
		}
	}

	async function saveSettingsForm() {
		if (!settings) return;
		busy = true;
		try {
			settings.trayEnabled = true;
			await savePortalSettings(settings);
			showToast('success', m.settings_saved());
			settingsOpen = false;
		} catch (err) {
			showToast('error', err instanceof Error ? err.message : String(err));
		} finally {
			busy = false;
		}
	}

	function completeOnboarding() {
		const local = loadSettings();
		local.onboarded = true;
		saveSettings(local);
		onboarded = true;
		if (settings) {
			settings = { ...settings, onboarded: true };
			void savePortalSettings(settings);
		}
	}

	onMount(() => {
		const unsub = subscribePortalRefresh(() => {
			void refreshAll();
		});
		const unsubFocus = subscribeFocusConnected(() => {
			focusConnectedProfile();
		});
		void refreshAll().then(async () => {
			if (!settings) return;
			const autoId = settings.autoConnectProfileId;
			if (autoId && status.phase === 'idle') {
				const profile = profiles.find((p) => p.id === autoId);
				if (profile) await beginConnect(profile);
			}
		});

		let unlistenStatus: (() => void) | undefined;
		let unlistenLog: (() => void) | undefined;
		let unlistenChallenge: (() => void) | undefined;
		let unlistenTraffic: (() => void) | undefined;
		let unlistenTrayStats: (() => void) | undefined;
		let unlistenTrayToast: (() => void) | undefined;
		let unlistenTrayAuth: (() => void) | undefined;
		if (isTauri()) {
			void listen<ConnStatus>('portal://status', (event) => {
				const prev = status.phase;
				status = event.payload;
				if (event.payload.phase !== 'connected') {
					downBps = 0;
					upBps = 0;
				}
				if (
					settings?.reconnectOnDrop &&
					prev === 'connected' &&
					event.payload.phase === 'error' &&
					event.payload.profileId &&
					!reconnectArmed &&
					!busy
				) {
					reconnectArmed = true;
					const id = event.payload.profileId;
					const profile = profiles.find((p) => p.id === id);
					window.setTimeout(() => {
						if (profile) {
							void beginConnect(profile).finally(() => {
								reconnectArmed = false;
							});
						} else {
							reconnectArmed = false;
						}
					}, 1500);
				}
			}).then((fn) => {
				unlistenStatus = fn;
			});
			void listen<string>('portal://log', (event) => {
				// Only print into the log panel while that tab is open.
				if (detailTab !== 'log') return;
				logs = [...logs, event.payload].slice(-MAX_CONNECTION_LOG_BLOCKS);
			}).then((fn) => {
				unlistenLog = fn;
			});
			void listen<{
				bytesIn: number;
				bytesOut: number;
				downBps: number;
				upBps: number;
			}>('portal://traffic', (event) => {
				bytesIn = event.payload.bytesIn;
				bytesOut = event.payload.bytesOut;
				downBps = event.payload.downBps;
				upBps = event.payload.upBps;
			}).then((fn) => {
				unlistenTraffic = fn;
			});
			void listen<string>('portal://auth-challenge', (event) => {
				challengeText = event.payload;
				challengeOpen = true;
			}).then((fn) => {
				unlistenChallenge = fn;
			});
			void listen('portal://tray-show-stats', () => {
				focusConnectedProfile();
			}).then((fn) => {
				unlistenTrayStats = fn;
			});
			void listen<string>('portal://tray-toast', (event) => {
				showToast('error', event.payload);
			}).then((fn) => {
				unlistenTrayToast = fn;
			});
			void listen<string>('portal://tray-needs-auth', (event) => {
				const profile = profiles.find((p) => p.id === event.payload);
				showToast('info', m.tray_needs_auth());
				if (profile) void beginConnect(profile);
			}).then((fn) => {
				unlistenTrayAuth = fn;
			});
		}

		const tick = setInterval(() => {
			if (status.phase === 'connected') {
				status = { ...status };
			}
		}, 1000);

		return () => {
			unsub();
			unsubFocus();
			unlistenStatus?.();
			unlistenLog?.();
			unlistenChallenge?.();
			unlistenTraffic?.();
			unlistenTrayStats?.();
			unlistenTrayToast?.();
			unlistenTrayAuth?.();
			clearInterval(tick);
		};
	});

	$effect(() => {
		if (selected) renameValue = selected.name;
	});
</script>

{#if !onboarded}
	<div class="bg-base-100 flex flex-1 flex-col items-center justify-center gap-4 p-8 text-center">
		<WashIcon icon={washIcons['shield-keyhole']} class="text-primary size-16" />
		<h1 class="font-display text-2xl font-semibold">{m.onboard_title()}</h1>
		<p class="text-base-content/70 max-w-lg text-sm leading-relaxed">{m.onboard_body()}</p>
		<button type="button" class="btn btn-primary cursor-pointer" onclick={completeOnboarding}
			>{m.onboard_continue()}</button
		>
	</div>
{:else}
	<div class="flex min-h-0 flex-1 flex-col gap-0 overflow-hidden md:flex-row">
		<aside
			class="border-ink-border/15 flex min-h-0 w-full min-w-0 shrink-0 flex-col overflow-hidden border-b md:h-full md:w-72 md:border-r md:border-b-0"
		>
			<div class="flex shrink-0 flex-wrap items-center gap-1 p-2">
				<div class={washRecipes.tooltipIcon('primary', 'bottom')} data-tip={m.import_file()}>
					<button
						type="button"
						class="btn btn-ghost btn-square btn-sm btn-primary cursor-pointer"
						class:loading={busy}
						disabled={busy}
						aria-label={m.import_file()}
						onclick={onImportFile}
					>
						{#if !busy}
							<WashIcon icon={washIcons['file-plus']} class="size-4" />
						{/if}
					</button>
				</div>
				<div class={washRecipes.tooltipIcon('primary', 'bottom')} data-tip={m.import_folder()}>
					<button
						type="button"
						class="btn btn-ghost btn-square btn-sm btn-primary cursor-pointer"
						disabled={busy}
						aria-label={m.import_folder()}
						onclick={onImportFolder}
					>
						<WashIcon icon={washIcons['folder-open']} class="size-4" />
					</button>
				</div>
				<div class={washRecipes.tooltipIcon('secondary', 'bottom')} data-tip={m.settings()}>
					<button
						type="button"
						class="btn btn-ghost btn-square btn-sm btn-secondary cursor-pointer"
						aria-label={m.settings()}
						onclick={() => (settingsOpen = true)}
					>
						<WashIcon icon={washIcons.settings} class="size-4" />
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
					<li class="text-base-content/60 w-full px-2 py-4 text-sm">{m.no_profiles()}</li>
				{:else}
					{#each filtered as profile (profile.id)}
						<li class="w-full min-w-0">
							<button
								type="button"
								class="flex w-full min-w-0 cursor-pointer items-center gap-2 overflow-hidden"
								class:active={selectedId === profile.id}
								onclick={() => (selectedId = profile.id)}
							>
								<WashIcon icon={washIcons['shield-keyhole']} class="size-4 shrink-0 opacity-70" />
								<span class="min-w-0 flex-1 truncate text-start">{profile.name}</span>
							</button>
						</li>
					{/each}
				{/if}
			</ul>
		</aside>

		<section class="flex min-h-0 min-w-0 flex-1 flex-col">
			<div class="border-ink-border/15 flex shrink-0 flex-wrap items-center gap-2 border-b p-3">
				<div class="min-w-0 flex-1">
					<p class="font-display text-lg font-semibold">
						{selected?.name ?? m.no_selection()}
					</p>
					<p class="text-base-content/60 truncate text-xs">
						{selected?.sourcePath ?? ''}
					</p>
				</div>
				{#if selectedNeedsSwitch && selected}
					<button
						type="button"
						class="btn btn-primary btn-sm cursor-pointer"
						class:loading={busy}
						disabled={busy}
						onclick={() => selected && requestConnect(selected)}
					>
						{#if !busy}
							<WashIcon icon={washIcons.plug} class="size-4" />
						{/if}
						{m.connect()}
					</button>
				{:else if isActiveSession}
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
				{:else if selected}
					<button
						type="button"
						class="btn btn-primary btn-sm cursor-pointer"
						class:loading={busy}
						disabled={busy}
						onclick={() => selected && requestConnect(selected)}
					>
						{#if !busy}
							<WashIcon icon={washIcons.plug} class="size-4" />
						{/if}
						{m.connect()}
					</button>
				{/if}
			</div>

			{#if selected}
				<div class="grid shrink-0 grid-cols-2 gap-2 p-3 text-sm sm:grid-cols-4">
					<div class="bg-base-200/40 rounded-box p-2">
						<p class="text-base-content/50 text-xs">{m.status_label()}</p>
						<p class={profileStatusClass}>{profileStatusLabel}</p>
					</div>
					<div class="bg-base-200/40 rounded-box p-2">
						<p class="text-base-content/50 text-xs">{m.duration_label()}</p>
						<p class="font-mono">{profileDurationText}</p>
					</div>
					<div class="bg-base-200/40 rounded-box p-2">
						<p class="text-base-content/50 text-xs">{m.vpn_ip_label()}</p>
						<p class="font-mono">{profileVpnIpText}</p>
					</div>
					<div class="bg-base-200/40 rounded-box p-2">
						<p class="text-base-content/50 text-xs">{m.auth_label()}</p>
						<p>
							{selected.needsUserPass ? m.auth_user_pass() : m.auth_cert()}
						</p>
					</div>
				</div>

				<div class="flex shrink-0 flex-wrap items-end gap-2 px-3 pb-2">
					<label class="form-control w-full max-w-xs">
						<span class="label-text text-xs">{m.rename_profile()}</span>
						<input class="input input-bordered input-sm cursor-text" bind:value={renameValue} />
					</label>
					<button
						type="button"
						class="btn btn-secondary btn-sm cursor-pointer"
						disabled={busy}
						onclick={onRename}>{m.rename()}</button
					>
					<button
						type="button"
						class="btn btn-ghost btn-sm btn-error cursor-pointer"
						disabled={busy}
						onclick={() => selected && onDelete(selected.id)}
					>
						<WashIcon icon={washIcons['trash-2']} class="size-4" />
						{m.delete()}
					</button>
					<button
						type="button"
						class="btn btn-ghost btn-sm cursor-pointer"
						onclick={() =>
							selected &&
							clearProfileCredentials(selected.id).then(() =>
								showToast('success', m.credentials_cleared())
							)}
					>
						{m.clear_credentials()}
					</button>
				</div>
			{/if}

			<div class="border-ink-border/15 flex min-h-0 flex-1 flex-col border-t">
				<div class="tabs tabs-box tabs-sm mx-3 mt-2 shrink-0">
					<label class="tab cursor-pointer gap-1">
						<input
							type="radio"
							name="portal_detail_tabs"
							class="hidden"
							checked={detailTab === 'performance'}
							onchange={() => (detailTab = 'performance')}
						/>
						<WashIcon icon={washIcons.gauge} class="size-3.5" />
						{m.tab_performance()}
					</label>
					<label class="tab cursor-pointer gap-1">
						<input
							type="radio"
							name="portal_detail_tabs"
							class="hidden"
							checked={detailTab === 'log'}
							onchange={() => void openLogTab()}
						/>
						<WashIcon icon={washIcons['scroll-text']} class="size-3.5" />
						{m.tab_connection_log()}
					</label>
				</div>

				{#if detailTab === 'performance'}
					<div class="flex min-h-0 flex-1 flex-col gap-3 overflow-auto p-3">
						{#if selectedIsConnected}
							<div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
								<div class="bg-base-200/40 rounded-box flex items-center gap-3 p-4">
									<WashIcon icon={washIcons['arrow-down']} class="text-success size-8" />
									<div>
										<p class="text-base-content/50 text-xs">{m.speed_down()}</p>
										<p class="font-mono text-xl font-semibold">{formatRate(downBps)}</p>
										<p class="text-base-content/50 font-mono text-xs">
											{m.bytes_in()}: {formatBytes(bytesIn)}
										</p>
									</div>
								</div>
								<div class="bg-base-200/40 rounded-box flex items-center gap-3 p-4">
									<WashIcon icon={washIcons['arrow-up']} class="text-info size-8" />
									<div>
										<p class="text-base-content/50 text-xs">{m.speed_up()}</p>
										<p class="font-mono text-xl font-semibold">{formatRate(upBps)}</p>
										<p class="text-base-content/50 font-mono text-xs">
											{m.bytes_out()}: {formatBytes(bytesOut)}
										</p>
									</div>
								</div>
							</div>
						{:else}
							<p class="text-base-content/60 p-4 text-sm">{m.perf_idle()}</p>
						{/if}
					</div>
				{:else}
					<div class="relative flex min-h-0 flex-1 flex-col px-3 pt-1.5 pb-3">
						<div class="absolute top-2 right-4 z-10">
							<div class={washRecipes.tooltipIcon('secondary', 'left')} data-tip={m.copy_logs()}>
								<button
									type="button"
									class="{washRecipes.btnRipple} btn-ghost btn-square btn-secondary btn-xs cursor-pointer"
									aria-label={m.copy_logs()}
									onclick={() => void navigator.clipboard.writeText(logs.join('\n'))}
								>
									<WashIcon icon={washIcons.copy} class="size-3.5" />
								</button>
							</div>
						</div>
						<div
							class="mockup-code min-h-0 flex-1 overflow-x-hidden overflow-y-auto font-mono text-xs leading-relaxed"
							role="log"
							aria-live="polite"
							aria-relevant="additions"
							aria-label={m.logs_title()}
						>
							{#if logs.length}
								{#each logs as line, i (i)}
									{@const parsed = parsePortalLogLine(line)}
									<pre
										data-prefix={parsed.prefix}
										class="{parsed.prefixClass} !w-full max-w-full !min-w-0 break-all whitespace-pre-wrap"><code
											class="break-all whitespace-pre-wrap"
											>{#each parsed.segments as seg, si (`${i}-${si}`)}<span class={seg.className}
													>{seg.text}</span
												>{/each}</code
										></pre>
								{/each}
							{:else}
								<pre
									data-prefix="~"
									class="text-base-content/50 !w-full max-w-full !min-w-0 break-all whitespace-pre-wrap"><code
										class="text-base-content/50 break-all whitespace-pre-wrap"
										>{m.logs_empty()}</code
									></pre>
							{/if}
						</div>
					</div>
				{/if}
			</div>
		</section>
	</div>
{/if}

{#if authOpen}
	<dialog class="modal modal-open">
		<div class="modal-box bg-transparent p-0 shadow-none sm:max-w-md">
			<div class="flex min-h-80 items-center justify-center p-2">
				<form
					class="card border-base-300 bg-base-100 w-full max-w-sm border shadow-sm"
					onsubmit={(e) => {
						e.preventDefault();
						if (authProfileId && !busy) void doConnect(authProfileId);
					}}
				>
					<div class="card-body gap-4">
						<h2 class="card-title text-primary font-bold">{m.auth_title()}</h2>

						<fieldset class="fieldset">
							<label class="label" for="portal-auth-username">
								{m.username()}<span
									class="text-error align-top text-sm leading-none"
									aria-hidden="true">*</span
								>
							</label>
							<label class="input w-full">
								<WashIcon icon={washIcons.user} class="size-4 opacity-50" />
								<input
									id="portal-auth-username"
									class="grow cursor-text"
									type="text"
									name="username"
									autocomplete="username"
									bind:value={authUsername}
									required
								/>
							</label>
						</fieldset>

						<fieldset class="fieldset">
							<label class="label" for="portal-auth-password">
								{m.password()}<span
									class="text-error align-top text-sm leading-none"
									aria-hidden="true">*</span
								>
							</label>
							<label class="input w-full">
								<WashIcon icon={washIcons.lock} class="size-4 opacity-50" />
								<input
									id="portal-auth-password"
									class="grow cursor-text"
									type="password"
									name="password"
									autocomplete="current-password"
									bind:value={authPassword}
									required
								/>
							</label>
						</fieldset>

						<fieldset class="fieldset">
							<label class="label" for="portal-auth-key-pass">{m.key_passphrase()}</label>
							<label class="input w-full">
								<WashIcon icon={washIcons.key} class="size-4 opacity-50" />
								<input
									id="portal-auth-key-pass"
									class="grow cursor-text"
									type="password"
									name="keyPassphrase"
									autocomplete="off"
									bind:value={authKeyPass}
								/>
							</label>
						</fieldset>

						<label class="flex cursor-pointer items-center gap-2 text-sm">
							<input type="checkbox" class="checkbox checkbox-sm" bind:checked={authRemember} />
							{m.remember_credentials()}
						</label>
						{#if authRemember}
							<div class="flex flex-wrap gap-3 text-sm">
								<label class="flex cursor-pointer items-center gap-1">
									<input
										type="radio"
										class="radio radio-sm"
										checked={authStore === 'keyring'}
										onchange={() => (authStore = 'keyring')}
									/>
									{m.store_keyring()}
								</label>
								<label class="flex cursor-pointer items-center gap-1">
									<input
										type="radio"
										class="radio radio-sm"
										checked={authStore === 'session'}
										onchange={() => (authStore = 'session')}
									/>
									{m.store_session()}
								</label>
							</div>
						{/if}

						<div class="card-actions justify-end gap-2">
							<button
								type="button"
								class="btn"
								disabled={busy}
								class:cursor-pointer={!busy}
								class:cursor-not-allowed={busy}
								onclick={() => (authOpen = false)}>{m.cancel()}</button
							>
							<button
								type="submit"
								class="btn btn-primary"
								class:loading={busy}
								class:cursor-pointer={!busy}
								class:cursor-not-allowed={busy}
								disabled={busy}
								aria-busy={busy}>{m.connect()}</button
							>
						</div>
					</div>
				</form>
			</div>
		</div>
		<form method="dialog" class="modal-backdrop">
			<button type="button" class="cursor-pointer" onclick={() => (authOpen = false)}>close</button>
		</form>
	</dialog>
{/if}

{#if switchOpen && switchTarget}
	<dialog class="modal modal-open">
		<div class="modal-box">
			<h2 class="card-title text-primary font-bold">{m.switch_profile_title()}</h2>
			<p class="text-base-content/70 py-2 text-sm">
				{m.switch_profile_body({
					target: switchTarget.name,
					current: connectedProfileName
				})}
			</p>
			<div class="modal-action">
				<button type="button" class="btn cursor-pointer" disabled={busy} onclick={onCancelSwitch}
					>{m.cancel()}</button
				>
				<button
					type="button"
					class="btn btn-primary cursor-pointer"
					class:loading={busy}
					disabled={busy}
					aria-busy={busy}
					onclick={onConfirmSwitch}>{m.switch_profile_confirm()}</button
				>
			</div>
		</div>
		<form method="dialog" class="modal-backdrop">
			<button type="button" class="cursor-pointer" disabled={busy} onclick={onCancelSwitch}
				>close</button
			>
		</form>
	</dialog>
{/if}

{#if challengeOpen}
	<dialog class="modal modal-open">
		<div class="modal-box">
			<h2 class="card-title text-secondary font-bold">{m.challenge_title()}</h2>
			<p class="py-2 font-mono text-xs break-all">{challengeText}</p>
			<label class="form-control w-full">
				<span class="label-text"
					>{m.challenge_response()}<span class="text-error align-top text-sm" aria-hidden="true"
						>*</span
					></span
				>
				<input class="input input-bordered cursor-text" bind:value={challengePassword} />
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
							await respondAuthChallenge('Auth', challengePassword);
							challengeOpen = false;
							challengePassword = '';
						} catch (err) {
							showToast('error', err instanceof Error ? err.message : String(err));
						} finally {
							busy = false;
						}
					}}>{m.submit()}</button
				>
			</div>
		</div>
	</dialog>
{/if}

{#if settingsOpen && settings}
	<dialog class="modal modal-open">
		<div class="modal-box max-w-lg">
			<h2 class="card-title text-secondary font-bold">{m.settings()}</h2>
			<label class="mt-3 flex cursor-pointer items-center justify-between gap-2 text-sm">
				<span>{m.setting_reconnect()}</span>
				<input
					type="checkbox"
					class="toggle"
					checked={settings.reconnectOnDrop}
					onchange={(e) =>
						settings && (settings.reconnectOnDrop = (e.currentTarget as HTMLInputElement).checked)}
				/>
			</label>
			<label class="mt-2 flex cursor-pointer items-center justify-between gap-2 text-sm">
				<span>{m.setting_kill_switch()}</span>
				<input
					type="checkbox"
					class="toggle"
					checked={settings.killSwitch}
					onchange={(e) =>
						settings && (settings.killSwitch = (e.currentTarget as HTMLInputElement).checked)}
				/>
			</label>
			<label class="mt-2 flex cursor-pointer items-center justify-between gap-2 text-sm">
				<span>{m.setting_elevate()}</span>
				<input
					type="checkbox"
					class="toggle"
					checked={settings.elevateOnConnect}
					onchange={(e) =>
						settings && (settings.elevateOnConnect = (e.currentTarget as HTMLInputElement).checked)}
				/>
			</label>
			<div class="form-control mt-3 w-full">
				<span class="label-text mb-1">{m.setting_auto_connect()}</span>
				<WashSelect
					placeholder={m.auto_connect_off()}
					value={settings.autoConnectProfileId ?? ''}
					options={[
						{ value: '', label: m.auto_connect_off() },
						...profiles.map((p) => ({ value: p.id, label: p.name }))
					]}
					onchange={(v) => {
						if (!settings) return;
						settings.autoConnectProfileId = v ? v : null;
					}}
				/>
			</div>
			<p class="text-base-content/70 mt-3 text-sm">{m.setting_tray()}</p>
			<p class="text-base-content/50 mt-2 text-xs">{m.setting_kill_switch_hint()}</p>
			<div class="modal-action">
				<button type="button" class="btn cursor-pointer" onclick={() => (settingsOpen = false)}
					>{m.cancel()}</button
				>
				<button
					type="button"
					class="btn btn-primary cursor-pointer"
					class:loading={busy}
					disabled={busy}
					onclick={saveSettingsForm}>{m.save()}</button
				>
			</div>
		</div>
	</dialog>
{/if}

{#if toast}
	<div class="toast toast-bottom toast-end z-[300]">
		<div class="alert alert-{toast.tone} shadow-lg">
			<span>{toast.text}</span>
		</div>
	</div>
{/if}
