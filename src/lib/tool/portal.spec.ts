import { describe, expect, it } from 'vitest';
import {
	connStatusSchema,
	emptyGpProfile,
	gpProfileSchema,
	portalDiskSettingsSchema
} from './portal-ipc';

describe('portal schemas', () => {
	it('parses default nested settings', () => {
		const s = portalDiskSettingsSchema.parse({});
		expect(s.openvpn.killSwitch).toBe(false);
		expect(s.openvpn.elevateOnConnect).toBe(true);
		expect(s.openvpn.credentialStore).toBe('keyring');
		expect(s.globalprotect.cookieCacheEnabled).toBe(true);
		expect(s.activeUiProtocol).toBe('openvpn');
	});

	it('migrates legacy flat settings', () => {
		const s = portalDiskSettingsSchema.parse({
			killSwitch: true,
			elevateOnConnect: false,
			credentialStore: 'session',
			autoConnectProfileId: 'p1'
		});
		expect(s.openvpn.killSwitch).toBe(true);
		expect(s.openvpn.elevateOnConnect).toBe(false);
		expect(s.openvpn.credentialStore).toBe('session');
		expect(s.openvpn.autoConnectProfileId).toBe('p1');
	});

	it('parses idle status', () => {
		const st = connStatusSchema.parse({ phase: 'idle', message: '' });
		expect(st.phase).toBe('idle');
		expect(st.protocol).toBe('openvpn');
	});

	it('parses connected status with vpn ip', () => {
		const st = connStatusSchema.parse({
			phase: 'connected',
			message: 'Connected',
			profileId: 'abc',
			vpnIp: '10.8.0.2',
			startedAtMs: 1_710_000_000_000,
			protocol: 'globalprotect'
		});
		expect(st.vpnIp).toBe('10.8.0.2');
		expect(st.phase).toBe('connected');
		expect(st.protocol).toBe('globalprotect');
	});

	it('parses empty GP profile', () => {
		const p = emptyGpProfile({ portal: 'vpn.example.com' });
		expect(p.portal).toBe('vpn.example.com');
		expect(p.authMode).toBe('password');
		expect(gpProfileSchema.parse(p).autoGateway).toBe(false);
		expect(p.icon.builtin).toBe('globe');
		expect(p.icon.kind).toBe('builtin');
	});

	it('accepts custom GP profile icon', () => {
		const p = gpProfileSchema.parse({
			...emptyGpProfile({ portal: 'vpn.example.com' }),
			icon: { kind: 'builtin', builtin: 'server', color: '#276C8E', imagePath: null }
		});
		expect(p.icon.builtin).toBe('server');
		expect(p.icon.color).toBe('#276C8E');
	});

	it('exports connection log block cap', async () => {
		const { MAX_CONNECTION_LOG_BLOCKS } = await import('./portal-ipc');
		expect(MAX_CONNECTION_LOG_BLOCKS).toBe(20);
	});
});
