import { describe, expect, it } from 'vitest';
import { connStatusSchema, portalDiskSettingsSchema } from './portal-ipc';

describe('portal schemas', () => {
	it('parses default settings', () => {
		const s = portalDiskSettingsSchema.parse({});
		expect(s.killSwitch).toBe(false);
		expect(s.elevateOnConnect).toBe(true);
		expect(s.credentialStore).toBe('keyring');
	});

	it('parses idle status', () => {
		const st = connStatusSchema.parse({ phase: 'idle', message: '' });
		expect(st.phase).toBe('idle');
	});

	it('parses connected status with vpn ip', () => {
		const st = connStatusSchema.parse({
			phase: 'connected',
			message: 'Connected',
			profileId: 'abc',
			vpnIp: '10.8.0.2',
			startedAtMs: 1_710_000_000_000
		});
		expect(st.vpnIp).toBe('10.8.0.2');
		expect(st.phase).toBe('connected');
	});
});
