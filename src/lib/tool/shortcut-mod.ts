/** Platform modifier label for menu shortcut hints. */

export function isApplePlatform(): boolean {
	if (typeof navigator === 'undefined') return false;
	const platform = navigator.platform ?? '';
	return /Mac|iPhone|iPad|iPod/i.test(platform) || /Mac OS X/i.test(navigator.userAgent);
}

/** Formal modifier key name: Cmd on Apple platforms, Ctrl elsewhere. */
export function modLabel(): string {
	return isApplePlatform() ? 'Cmd' : 'Ctrl';
}

/** Chord like Ctrl+Shift+S (mod first, then extra parts). */
export function shortcutChord(...parts: string[]): string {
	return [modLabel(), ...parts].join('+');
}
