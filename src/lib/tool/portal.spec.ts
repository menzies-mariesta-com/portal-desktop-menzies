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
});
