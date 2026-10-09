import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { z } from 'zod';

export function isTauri(): boolean {
	return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export const vpnProtocolSchema = z.enum(['openvpn', 'globalprotect']);
export type VpnProtocol = z.infer<typeof vpnProtocolSchema>;

export const protocolSettingsSchema = z.object({
	autoConnectProfileId: z.string().nullable().optional().default(null),
	reconnectOnDrop: z.boolean().default(false),
	killSwitch: z.boolean().default(false),
	credentialStore: z.enum(['keyring', 'session']).default('keyring'),
	elevateOnConnect: z.boolean().default(true)
});

export type ProtocolSettings = z.infer<typeof protocolSettingsSchema>;

export const gpProtocolSettingsSchema = protocolSettingsSchema.extend({
	cookieCacheEnabled: z.boolean().default(true),
	defaultBrowserMode: z.enum(['auto', 'default', 'remote', 'embedded']).default('auto'),
	ignoreTlsErrors: z.boolean().default(false),
	fixOpenssl: z.boolean().default(false),
	defaultOs: z.string().default('Linux')
});

export type GpProtocolSettings = z.infer<typeof gpProtocolSettingsSchema>;

export const portalDiskSettingsSchema = z
	.object({
		onboarded: z.boolean().default(false),
		trayEnabled: z.boolean().default(true),
		activeUiProtocol: vpnProtocolSchema.default('openvpn'),
		openvpn: protocolSettingsSchema.default({
			autoConnectProfileId: null,
			reconnectOnDrop: false,
			killSwitch: false,
			credentialStore: 'keyring',
			elevateOnConnect: true
		}),
		globalprotect: gpProtocolSettingsSchema.default({
			autoConnectProfileId: null,
			reconnectOnDrop: false,
			killSwitch: false,
			credentialStore: 'keyring',
			elevateOnConnect: true,
			cookieCacheEnabled: true,
			defaultBrowserMode: 'auto',
			ignoreTlsErrors: false,
			fixOpenssl: false,
			defaultOs: 'Linux'
		}),
		// Legacy flat fields (pre dual-mode); stripped after normalize.
		autoConnectProfileId: z.string().nullable().optional(),
		reconnectOnDrop: z.boolean().optional(),
		killSwitch: z.boolean().optional(),
		credentialStore: z.enum(['keyring', 'session']).optional(),
		elevateOnConnect: z.boolean().optional()
	})
	.transform((raw) => {
		// Prefer legacy flat fields when present (migration). Nested defaults are always set by Zod.
		const openvpn = protocolSettingsSchema.parse({
			autoConnectProfileId: raw.autoConnectProfileId ?? raw.openvpn?.autoConnectProfileId ?? null,
			reconnectOnDrop: raw.reconnectOnDrop ?? raw.openvpn?.reconnectOnDrop ?? false,
			killSwitch: raw.killSwitch ?? raw.openvpn?.killSwitch ?? false,
			credentialStore: raw.credentialStore ?? raw.openvpn?.credentialStore ?? 'keyring',
			elevateOnConnect: raw.elevateOnConnect ?? raw.openvpn?.elevateOnConnect ?? true
		});
		const globalprotect = gpProtocolSettingsSchema.parse(raw.globalprotect ?? {});
		return {
			onboarded: raw.onboarded,
			trayEnabled: raw.trayEnabled,
			activeUiProtocol: raw.activeUiProtocol,
			openvpn,
			globalprotect
		};
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

export const gpAuthModeSchema = z.enum(['password', 'browser', 'webview', 'cert', 'mixed']);
export type GpAuthMode = z.infer<typeof gpAuthModeSchema>;

export const gpHipSchema = z.object({
	enabled: z.boolean().default(false),
	scriptPath: z.string().nullable().optional().default(null),
	hipUser: z.string().nullable().optional().default(null)
});

export const gpAdvancedSchema = z.object({
	mtu: z.number().nullable().optional().default(null),
	noDtls: z.boolean().default(false),
	disableIpv6: z.boolean().default(false),
	os: z.string().nullable().optional().default(null),
	clientVersion: z.string().nullable().optional().default(null),
	interface: z.string().nullable().optional().default(null),
	reconnectTimeout: z.number().default(300),
	noXmlpost: z.boolean().default(false),
	csc: z.enum(['auto', 'yes', 'no']).default('auto'),
	localHostname: z.string().nullable().optional().default(null),
	dpdInterval: z.number().nullable().optional().default(null),
	cleanCookie: z.boolean().default(false),
	cookieCachePath: z.string().nullable().optional().default(null),
	script: z.string().nullable().optional().default(null),
	scriptTun: z.boolean().default(false)
});

export const gpPathsSchema = z.object({
	certificate: z.string().nullable().optional().default(null),
	sslKey: z.string().nullable().optional().default(null),
	hipScript: z.string().nullable().optional().default(null),
	cookieCache: z.string().nullable().optional().default(null)
});

/** Built-in Lucide keys for per-profile list icons. */
export const gpProfileIconBuiltinIds = [
	'globe',
	'cloud',
	'cloud-check',
	'server',
	'house',
	'building-2',
	'briefcase',
	'wifi',
	'network',
	'radio',
	'laptop',
	'smartphone',
	'map-pin',
	'rocket',
	'star',
	'heart',
	'zap',
	'factory',
	'shield',
	'shield-check',
	'lock',
	'key',
	'earth',
	'plug'
] as const;

export type GpProfileIconBuiltinId = (typeof gpProfileIconBuiltinIds)[number];

export const gpProfileIconSchema = z.object({
	kind: z.enum(['builtin', 'image']).default('builtin'),
	builtin: z.enum(gpProfileIconBuiltinIds).default('globe'),
	color: z.string().default(''),
	imagePath: z.string().nullable().optional().default(null)
});

export type GpProfileIcon = z.infer<typeof gpProfileIconSchema>;

export const gpProfileSchema = z.object({
	id: z.string(),
	name: z.string(),
	portal: z.string(),
	gateway: z.string().nullable().optional().default(null),
	autoGateway: z.boolean().default(false),
	asGateway: z.boolean().default(false),
	username: z.string().nullable().optional().default(null),
	authMode: gpAuthModeSchema.default('password'),
	browser: z.string().nullable().optional().default(null),
	certificatePath: z.string().nullable().optional().default(null),
	sslKeyPath: z.string().nullable().optional().default(null),
	hip: gpHipSchema.default({ enabled: false, scriptPath: null, hipUser: null }),
	advanced: gpAdvancedSchema.default({
		mtu: null,
		noDtls: false,
		disableIpv6: false,
		os: null,
		clientVersion: null,
		interface: null,
		reconnectTimeout: 300,
		noXmlpost: false,
		csc: 'auto',
		localHostname: null,
		dpdInterval: null,
		cleanCookie: false,
		cookieCachePath: null,
		script: null,
		scriptTun: false
	}),
	paths: gpPathsSchema.default({
		certificate: null,
		sslKey: null,
		hipScript: null,
		cookieCache: null
	}),
	icon: gpProfileIconSchema.default({
		kind: 'builtin',
		builtin: 'globe',
		color: '',
		imagePath: null
	}),
	lastUsedMs: z.number().nullable().optional().default(null),
	createdMs: z.number()
});

export type GpProfile = z.infer<typeof gpProfileSchema>;

export const connStatusSchema = z.object({
	phase: z.enum(['idle', 'connecting', 'connected', 'reconnecting', 'error']),
	profileId: z.string().nullable().optional(),
	message: z.string(),
	vpnIp: z.string().nullable().optional(),
	startedAtMs: z.number().nullable().optional(),
	managementPort: z.number().nullable().optional(),
	protocol: vpnProtocolSchema.optional().default('openvpn'),
	interfaceName: z.string().nullable().optional().default(null)
});

export type ConnStatus = z.infer<typeof connStatusSchema>;

export function emptyGpProfile(partial?: Partial<GpProfile>): GpProfile {
	return gpProfileSchema.parse({
		id: partial?.id ?? '',
		name: partial?.name ?? 'New portal',
		portal: partial?.portal ?? '',
		gateway: partial?.gateway ?? '',
		username: partial?.username ?? '',
		browser: partial?.browser ?? '',
		createdMs: partial?.createdMs ?? Date.now(),
		...partial
	});
}

/** Normalize nullable string fields so text inputs never bind to null. */
export function coerceGpProfileStrings(profile: GpProfile): GpProfile {
	return {
		...profile,
		gateway: profile.gateway ?? '',
		username: profile.username ?? '',
		browser: profile.browser ?? '',
		certificatePath: profile.certificatePath ?? '',
		sslKeyPath: profile.sslKeyPath ?? '',
		paths: {
			certificate: profile.paths.certificate ?? '',
			sslKey: profile.paths.sslKey ?? '',
			hipScript: profile.paths.hipScript ?? '',
			cookieCache: profile.paths.cookieCache ?? ''
		},
		hip: {
			...profile.hip,
			scriptPath: profile.hip.scriptPath ?? '',
			hipUser: profile.hip.hipUser ?? ''
		},
		advanced: {
			...profile.advanced,
			os: profile.advanced.os ?? '',
			clientVersion: profile.advanced.clientVersion ?? '',
			interface: profile.advanced.interface ?? '',
			localHostname: profile.advanced.localHostname ?? '',
			cookieCachePath: profile.advanced.cookieCachePath ?? '',
			script: profile.advanced.script ?? ''
		},
		icon: gpProfileIconSchema.parse(profile.icon ?? {})
	};
}

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
		return connStatusSchema.parse({
			phase: 'idle',
			message: '',
			protocol: 'openvpn',
			interfaceName: null
		});
	}
	const raw = await invoke<unknown>('connection_status');
	return connStatusSchema.parse(raw);
}

export async function gpConnectionStatus(): Promise<ConnStatus> {
	if (!isTauri()) {
		return connStatusSchema.parse({
			phase: 'idle',
			message: '',
			protocol: 'globalprotect',
			interfaceName: null
		});
	}
	const raw = await invoke<unknown>('gp_connection_status');
	return connStatusSchema.parse({ ...(raw as object), protocol: 'globalprotect' });
}

/** Max connection-log blocks kept in UI and Rust ring buffer. */
export const MAX_CONNECTION_LOG_BLOCKS = 20;

export async function portalLogs(): Promise<string[]> {
	if (!isTauri()) return [];
	const lines = await invoke<string[]>('portal_logs');
	return lines.slice(-MAX_CONNECTION_LOG_BLOCKS);
}

export async function gpLogs(): Promise<string[]> {
	if (!isTauri()) return [];
	const lines = await invoke<string[]>('gp_logs');
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

export async function listGpProfiles(): Promise<GpProfile[]> {
	if (!isTauri()) return [];
	const raw = await invoke<unknown>('list_gp_profiles');
	return z.array(gpProfileSchema).parse(raw);
}

export async function saveGpProfile(profile: GpProfile): Promise<GpProfile> {
	const raw = await invoke<unknown>('save_gp_profile', { profile });
	return gpProfileSchema.parse(raw);
}

export async function deleteGpProfile(id: string): Promise<void> {
	await invoke('delete_gp_profile', { id });
}

export async function renameGpProfile(id: string, name: string): Promise<void> {
	await invoke('rename_gp_profile', { id, name });
}

export async function connectGp(payload: {
	profileId: string;
	username?: string;
	password?: string;
	keyPassphrase?: string;
	remember?: boolean;
	store?: 'keyring' | 'session';
	gateway?: string;
}): Promise<void> {
	await invoke('connect_gp', {
		req: {
			profileId: payload.profileId,
			username: payload.username ?? null,
			password: payload.password ?? null,
			keyPassphrase: payload.keyPassphrase ?? null,
			remember: payload.remember ?? false,
			store: payload.store ?? null,
			gateway: payload.gateway ?? null
		}
	});
}

export async function disconnectGp(): Promise<void> {
	await invoke('disconnect_gp');
}

/**
 * Native file picker via the dialog plugin (async). Do not use a sync Rust
 * blocking_pick_file command: on Linux that runs on the webview thread and can freeze or crash.
 */
export async function pickGpPath(
	category: 'certificate' | 'sslKey' | 'hipScript' | 'cookieCache' | 'iconImage'
): Promise<string | null> {
	if (!isTauri()) return null;
	const filters =
		category === 'certificate' || category === 'sslKey'
			? [
					{
						name: 'Certificates and keys',
						extensions: ['pem', 'crt', 'cer', 'key', 'p12', 'pfx', 'pkcs12']
					}
				]
			: category === 'iconImage'
				? [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif', 'svg'] }]
				: category === 'hipScript'
					? [{ name: 'Scripts', extensions: ['sh', 'py', 'pl', 'bash'] }]
					: undefined;
	const title =
		category === 'certificate'
			? 'Select client certificate'
			: category === 'sslKey'
				? 'Select SSL private key'
				: category === 'hipScript'
					? 'Select HIP script'
					: category === 'cookieCache'
						? 'Select cookie cache file'
						: category === 'iconImage'
							? 'Select profile icon image'
							: 'Select file';
	const picked = await open({
		multiple: false,
		title,
		filters
	});
	if (!picked || typeof picked !== 'string') return null;
	return picked;
}

export async function respondAuthChallenge(passwordType: string, password: string): Promise<void> {
	await invoke('respond_auth_challenge', {
		req: { passwordType, password }
	});
}

export async function respondGpAuth(passwordType: string, password: string): Promise<void> {
	await invoke('respond_gp_auth', {
		req: { passwordType, password }
	});
}

export async function hasStoredCredentials(id: string): Promise<boolean> {
	if (!isTauri()) return false;
	try {
		return await invoke<boolean>('has_stored_credentials', { id });
	} catch {
		// Keyring / Secret Service failures must not block the auth prompt.
		return false;
	}
}

export async function clearProfileCredentials(id: string): Promise<void> {
	await invoke('clear_profile_credentials', { id });
}

export async function hasGpStoredCredentials(id: string): Promise<boolean> {
	if (!isTauri()) return false;
	try {
		return await invoke<boolean>('has_gp_stored_credentials', { id });
	} catch {
		return false;
	}
}

export async function clearGpCredentials(id: string): Promise<void> {
	await invoke('clear_gp_credentials', { id });
}
