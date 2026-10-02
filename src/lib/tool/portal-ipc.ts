import { invoke } from '@tauri-apps/api/core';
import { z } from 'zod';

export function isTauri(): boolean {
	return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export const portalDiskSettingsSchema = z.object({
	onboarded: z.boolean().default(false),
	autoConnectProfileId: z.string().nullable().optional().default(null),
	reconnectOnDrop: z.boolean().default(false),
	killSwitch: z.boolean().default(false),
	credentialStore: z.enum(['keyring', 'session']).default('keyring'),
	elevateOnConnect: z.boolean().default(true),
	trayEnabled: z.boolean().default(true)
});

export type PortalDiskSettings = z.infer<typeof portalDiskSettingsSchema>;

export const profileSummarySchema = z.object({
	id: z.string(),
	name: z.string(),
	sourcePath: z.string(),
	needsUserPass: z.boolean(),
	needsKeyPassphrase: z.boolean(),
	lastUsedMs: z.number().nullable().optional(),
	createdMs: z.number()
});

export type ProfileSummary = z.infer<typeof profileSummarySchema>;

export const connStatusSchema = z.object({
	phase: z.enum(['idle', 'connecting', 'connected', 'reconnecting', 'error']),
	profileId: z.string().nullable().optional(),
	message: z.string(),
	vpnIp: z.string().nullable().optional(),
	startedAtMs: z.number().nullable().optional(),
	managementPort: z.number().nullable().optional()
});

export type ConnStatus = z.infer<typeof connStatusSchema>;

export async function loadPortalSettings(): Promise<PortalDiskSettings> {
	if (!isTauri()) return portalDiskSettingsSchema.parse({});
	const raw = await invoke<unknown>('load_portal_settings');
	return portalDiskSettingsSchema.parse(raw);
}

export async function savePortalSettings(settings: PortalDiskSettings): Promise<void> {
	if (!isTauri()) return;
	await invoke('save_portal_settings', { settings });
}

export async function listProfiles(): Promise<ProfileSummary[]> {
	if (!isTauri()) return [];
	const raw = await invoke<unknown>('list_profiles');
	return z.array(profileSummarySchema).parse(raw);
}

export async function importOvpnFile(path: string): Promise<ProfileSummary> {
	const raw = await invoke<unknown>('import_ovpn_file', { path });
	return profileSummarySchema.parse(raw);
}

export async function importOvpnFolder(path: string): Promise<ProfileSummary[]> {
	const raw = await invoke<unknown>('import_ovpn_folder', { path });
	return z.array(profileSummarySchema).parse(raw);
}

export async function deleteProfile(id: string): Promise<void> {
	await invoke('delete_profile', { id });
}

export async function renameProfile(id: string, name: string): Promise<void> {
	await invoke('rename_profile', { id, name });
}

export async function connectionStatus(): Promise<ConnStatus> {
	if (!isTauri()) {
		return connStatusSchema.parse({ phase: 'idle', message: '' });
	}
	const raw = await invoke<unknown>('connection_status');
	return connStatusSchema.parse(raw);
}

/** Max connection-log blocks kept in UI and Rust ring buffer. */
export const MAX_CONNECTION_LOG_BLOCKS = 20;

export async function portalLogs(): Promise<string[]> {
	if (!isTauri()) return [];
	const lines = await invoke<string[]>('portal_logs');
	// Keep last N mockup-code blocks (matches Rust MAX_LOG_BLOCKS).
	return lines.slice(-MAX_CONNECTION_LOG_BLOCKS);
}

export async function connectVpn(payload: {
	profileId: string;
	username?: string;
	password?: string;
	keyPassphrase?: string;
	remember?: boolean;
	store?: 'keyring' | 'session';
}): Promise<void> {
	await invoke('connect_vpn', {
		req: {
			profileId: payload.profileId,
			username: payload.username ?? null,
			password: payload.password ?? null,
			keyPassphrase: payload.keyPassphrase ?? null,
			remember: payload.remember ?? false,
			store: payload.store ?? null
		}
	});
}

export async function disconnectVpn(): Promise<void> {
	await invoke('disconnect_vpn');
}

export async function respondAuthChallenge(passwordType: string, password: string): Promise<void> {
	await invoke('respond_auth_challenge', {
		req: { passwordType, password }
	});
}

export async function hasStoredCredentials(id: string): Promise<boolean> {
	if (!isTauri()) return false;
	return invoke<boolean>('has_stored_credentials', { id });
}

export async function clearProfileCredentials(id: string): Promise<void> {
	await invoke('clear_profile_credentials', { id });
}
