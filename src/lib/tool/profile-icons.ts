import { convertFileSrc } from '@tauri-apps/api/core';
import {
	gpProfileIconBuiltinIds,
	isTauri,
	type GpProfileIcon,
	type GpProfileIconBuiltinId
} from '$lib/tool/portal-ipc';
import { washIcons, type WashIconNode } from '$lib/tool/wash-icons';

export const PROFILE_ICON_COLORS: { id: string; label: string; value: string }[] = [
	{ id: 'default', label: 'Theme default', value: '' },
	{ id: 'mineral', label: 'Mineral', value: '#1A3A48' },
	{ id: 'blue', label: 'Mineral blue', value: '#276C8E' },
	{ id: 'ochre', label: 'Ochre', value: '#B87524' },
	{ id: 'rose', label: 'Rose', value: '#A33A32' },
	{ id: 'green', label: 'Forest', value: '#2D6A4F' },
	{ id: 'violet', label: 'Violet', value: '#5B4B8A' },
	{ id: 'copper', label: 'Copper', value: '#C45C26' }
];

export function profileBuiltinIconNode(id: GpProfileIconBuiltinId | string): WashIconNode {
	const key = (
		gpProfileIconBuiltinIds.includes(id as GpProfileIconBuiltinId) ? id : 'globe'
	) as keyof typeof washIcons;
	return washIcons[key] ?? washIcons.globe;
}

export function profileIconImageSrc(path: string | null | undefined): string | null {
	if (!path?.trim()) return null;
	if (isTauri()) {
		try {
			return convertFileSrc(path);
		} catch {
			return null;
		}
	}
	return path;
}

export function defaultProfileIcon(): GpProfileIcon {
	return { kind: 'builtin', builtin: 'globe', color: '', imagePath: null };
}
