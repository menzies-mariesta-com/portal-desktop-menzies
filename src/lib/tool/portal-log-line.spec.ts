import { describe, expect, it } from 'vitest';
import { parsePortalLogLine } from './portal-log-line';

describe('parsePortalLogLine', () => {
	it('colors INFO lines', () => {
		const parsed = parsePortalLogLine(
			'>INFO:OpenVPN Management Interface Version 5 -- type help for more info'
		);
		expect(parsed.segments[0]?.text).toBe('INFO');
		expect(parsed.segments[0]?.className).toBe('text-info');
		expect(parsed.prefixClass).toBe('text-info');
	});

	it('colors SUCCESS lines', () => {
		const parsed = parsePortalLogLine('SUCCESS: bytecount interval changed');
		expect(parsed.segments[0]?.text).toBe('SUCCESS');
		expect(parsed.segments[0]?.className).toBe('text-success');
	});

	it('colors CONNECTED status rows and IP addresses', () => {
		const parsed = parsePortalLogLine(
			'1790821958,CONNECTED,SUCCESS,172.21.90.249,108.171.108.87,1194,,'
		);
		const texts = parsed.segments.map((s) => s.text);
		expect(texts).toContain('CONNECTED');
		expect(texts).toContain('SUCCESS');
		expect(parsed.segments.find((s) => s.text === 'CONNECTED')?.className).toBe('text-success');
		expect(parsed.segments.find((s) => s.text === '172.21.90.249')?.className).toBe('text-info');
		expect(parsed.prefixClass).toBe('text-success');
	});

	it('colors END markers', () => {
		const parsed = parsePortalLogLine('END');
		expect(parsed.segments[0]?.className).toBe('text-base-content/55');
	});

	it('colors BYTECOUNT lines', () => {
		const parsed = parsePortalLogLine('>BYTECOUNT:54779617,8028766');
		expect(parsed.segments[0]?.text).toBe('BYTECOUNT');
		expect(parsed.segments[0]?.className).toBe('text-secondary');
		expect(parsed.segments[1]?.text).toBe(':54779617,8028766');
	});

	it('strips a leading chevron so mockup-code does not double it', () => {
		const parsed = parsePortalLogLine('> SUCCESS: ok');
		expect(parsed.segments[0]?.text).toBe('SUCCESS');
		expect(parsed.segments.some((s) => s.text.startsWith('>'))).toBe(false);
	});
});
