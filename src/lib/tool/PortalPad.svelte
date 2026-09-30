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
		portalLogs,
		renameProfile,
		respondAuthChallenge,
		savePortalSettings,
		type ConnStatus,
		type PortalDiskSettings,
		type ProfileSummary
	} from '$lib/tool/portal-ipc';
	import { loadSettings, saveSettings } from '$lib/store/local-storage/settings';

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
	let challengeOpen = $state(false);
	let challengeText = $state('');
	let challengePassword = $state('');
	let settingsOpen = $state(false);
	let onboarded = $state(loadSettings().onboarded);
	let renameValue = $state('');
	let reconnectArmed = $state(false);

	const selected = $derived(profiles.find((p) => p.id === selectedId) ?? null);
	const filtered = $derived(
		profiles.filter((p) => p.name.toLowerCase().includes(query.trim().toLowerCase()))
	);

	function showToast(tone: 'success' | 'error' | 'info' | 'warning', text: string) {
		if (toastTimer) clearTimeout(toastTimer);
		toast = { tone, text };
		toastTimer = setTimeout(() => {
			toast = null;
			toastTimer = null;
		}, 5000);
	}

	function formatDuration(startedAtMs: number | null | undefined): string {
		if (!startedAtMs) return '-';
		const sec = Math.max(0, Math.floor((Date.now() - startedAtMs) / 1000));
		const h = Math.floor(sec / 3600);
		const m_ = Math.floor((sec % 3600) / 60);
		const s = sec % 60;
		if (h > 0) return `${h}h ${m_}m`;
		if (m_ > 0) return `${m_}m ${s}s`;
		return `${s}s`;
	}

	async function refreshAll() {
		try {
			profiles = await listProfiles();
			status = await connectionStatus();
			logs = await portalLogs();
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
		if (isTauri()) {
			void listen<ConnStatus>('portal://status', (event) => {
				status = event.payload;
				if (
					settings?.reconnectOnDrop &&
					event.payload.phase === 'error' &&
					event.payload.profileId &&
					!reconnectArmed
				) {
					reconnectArmed = true;
					const id = event.payload.profileId;
					const profile = profiles.find((p) => p.id === id);
					if (profile) {
						void beginConnect(profile).finally(() => {
							reconnectArmed = false;
						});
					} else {
						reconnectArmed = false;
					}
				}
			}).then((fn) => {
				unlistenStatus = fn;
			});
			void listen<string>('portal://log', (event) => {
				logs = [...logs, event.payload].slice(-2000);
			}).then((fn) => {
				unlistenLog = fn;
			});
			void listen<string>('portal://auth-challenge', (event) => {
				challengeText = event.payload;
				challengeOpen = true;
			}).then((fn) => {
				unlistenChallenge = fn;
			});
		}

		const tick = setInterval(() => {
			if (status.phase === 'connected') {
				status = { ...status };
			}
		}, 1000);

		return () => {
			unsub();
			unlistenStatus?.();
			unlistenLog?.();
			unlistenChallenge?.();
			clearInterval(tick);
		};
	});

	$effect(() => {
		if (selected) renameValue = selected.name;
	});
</script>

{#if !onboarded}
	<div class="bg-base-100 flex flex-1 flex-col items-center justify-center gap-4 p-8 text-center">
		<WashIcon icon={washIcons.shield} class="text-primary size-16" />
		<h1 class="font-display text-2xl font-semibold">{m.onboard_title()}</h1>
		<p class="text-base-content/70 max-w-lg text-sm leading-relaxed">{m.onboard_body()}</p>
		<button type="button" class="btn btn-primary cursor-pointer" onclick={completeOnboarding}
			>{m.onboard_continue()}</button
		>
	</div>
{:else}
	<div class="flex min-h-0 flex-1 flex-col gap-0 overflow-hidden md:flex-row">
		<aside
			class="border-ink-border/15 flex w-full shrink-0 flex-col border-b md:w-72 md:border-r md:border-b-0"
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
			<ul class="menu menu-sm min-h-0 flex-1 overflow-y-auto p-1">
				{#if filtered.length === 0}
					<li class="text-base-content/60 px-2 py-4 text-sm">{m.no_profiles()}</li>
				{:else}
					{#each filtered as profile (profile.id)}
						<li>
							<button
								type="button"
								class="cursor-pointer"
								class:active={selectedId === profile.id}
								onclick={() => (selectedId = profile.id)}
							>
								<WashIcon icon={washIcons.shield} class="size-4 opacity-70" />
								<span class="truncate">{profile.name}</span>
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
				<span class="badge badge-outline capitalize">{status.phase}</span>
				{#if status.phase === 'connected' || status.phase === 'connecting'}
					<button
						type="button"
						class="btn btn-error btn-sm cursor-pointer"
						class:loading={busy}
						disabled={busy}
						onclick={onDisconnect}
					>
						<WashIcon icon={washIcons.unplug} class="size-4" />
						{m.disconnect()}
					</button>
				{:else if selected}
					<button
						type="button"
						class="btn btn-primary btn-sm cursor-pointer"
						class:loading={busy}
						disabled={busy}
						onclick={() => selected && beginConnect(selected)}
					>
						<WashIcon icon={washIcons.plug} class="size-4" />
						{m.connect()}
					</button>
				{/if}
			</div>

			{#if selected}
				<div class="grid shrink-0 grid-cols-2 gap-2 p-3 text-sm sm:grid-cols-4">
					<div class="bg-base-200/40 rounded-box p-2">
						<p class="text-base-content/50 text-xs">{m.status_label()}</p>
						<p>{status.message || status.phase}</p>
					</div>
					<div class="bg-base-200/40 rounded-box p-2">
						<p class="text-base-content/50 text-xs">{m.duration_label()}</p>
						<p class="font-mono">{formatDuration(status.startedAtMs)}</p>
					</div>
					<div class="bg-base-200/40 rounded-box p-2">
						<p class="text-base-content/50 text-xs">{m.vpn_ip_label()}</p>
						<p class="font-mono">{status.vpnIp ?? m.not_available()}</p>
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
				<div class="flex shrink-0 items-center justify-between px-3 py-1">
					<p class="text-xs font-medium opacity-70">{m.logs_title()}</p>
					<button
						type="button"
						class="btn btn-ghost btn-xs cursor-pointer"
						onclick={() => navigator.clipboard.writeText(logs.join('\n'))}
					>
						<WashIcon icon={washIcons.copy} class="size-3.5" />
						{m.copy_logs()}
					</button>
				</div>
				<pre
					class="bg-base-200/30 min-h-0 flex-1 overflow-auto p-3 font-mono text-xs leading-relaxed">{logs.length
						? logs.join('\n')
						: m.logs_empty()}</pre>
			</div>
		</section>
	</div>
{/if}

{#if authOpen}
	<dialog class="modal modal-open">
		<div class="modal-box">
			<h2 class="card-title text-primary font-bold">{m.auth_title()}</h2>
			<p class="text-base-content/70 py-2 text-sm">{m.auth_body()}</p>
			<label class="form-control w-full">
				<span class="label-text"
					>{m.username()}<span class="text-error align-top text-sm" aria-hidden="true">*</span
					></span
				>
				<input class="input input-bordered cursor-text" bind:value={authUsername} required />
			</label>
			<label class="form-control mt-2 w-full">
				<span class="label-text"
					>{m.password()}<span class="text-error align-top text-sm" aria-hidden="true">*</span
					></span
				>
				<input
					class="input input-bordered cursor-text"
					type="password"
					bind:value={authPassword}
					required
				/>
			</label>
			<label class="form-control mt-2 w-full">
				<span class="label-text">{m.key_passphrase()}</span>
				<input class="input input-bordered cursor-text" type="password" bind:value={authKeyPass} />
			</label>
			<label class="mt-3 flex cursor-pointer items-center gap-2 text-sm">
				<input type="checkbox" class="checkbox checkbox-sm" bind:checked={authRemember} />
				{m.remember_credentials()}
			</label>
			{#if authRemember}
				<div class="mt-2 flex gap-2 text-sm">
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
			<div class="modal-action">
				<button type="button" class="btn cursor-pointer" onclick={() => (authOpen = false)}
					>{m.cancel()}</button
				>
				<button
					type="button"
					class="btn btn-primary cursor-pointer"
					class:loading={busy}
					disabled={busy}
					onclick={() => authProfileId && doConnect(authProfileId)}>{m.connect()}</button
				>
			</div>
		</div>
		<form method="dialog" class="modal-backdrop">
			<button type="button" class="cursor-pointer" onclick={() => (authOpen = false)}>close</button>
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
			<label class="mt-2 flex cursor-pointer items-center justify-between gap-2 text-sm">
				<span>{m.setting_tray()}</span>
				<input
					type="checkbox"
					class="toggle"
					checked={settings.trayEnabled}
					onchange={(e) =>
						settings && (settings.trayEnabled = (e.currentTarget as HTMLInputElement).checked)}
				/>
			</label>
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
