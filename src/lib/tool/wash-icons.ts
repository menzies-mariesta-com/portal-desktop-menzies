/**
 * Lucide icon nodes (Wash pin). Sync `__iconNode` only.
 */
import { __iconNode as check } from 'lucide-react/dist/esm/icons/check.mjs';
import { __iconNode as chevronDown } from 'lucide-react/dist/esm/icons/chevron-down.mjs';
import { __iconNode as copy } from 'lucide-react/dist/esm/icons/copy.mjs';
import { __iconNode as download } from 'lucide-react/dist/esm/icons/download.mjs';
import { __iconNode as folderOpen } from 'lucide-react/dist/esm/icons/folder-open.mjs';
import { __iconNode as filePlus } from 'lucide-react/dist/esm/icons/file-plus.mjs';
import { __iconNode as minus } from 'lucide-react/dist/esm/icons/minus.mjs';
import { __iconNode as plug } from 'lucide-react/dist/esm/icons/plug.mjs';
import { __iconNode as refreshCw } from 'lucide-react/dist/esm/icons/refresh-cw.mjs';
import { __iconNode as search } from 'lucide-react/dist/esm/icons/search.mjs';
import { __iconNode as settings } from 'lucide-react/dist/esm/icons/settings.mjs';
import { __iconNode as shield } from 'lucide-react/dist/esm/icons/shield.mjs';
import { __iconNode as square } from 'lucide-react/dist/esm/icons/square.mjs';
import { __iconNode as sun } from 'lucide-react/dist/esm/icons/sun.mjs';
import { __iconNode as trash2 } from 'lucide-react/dist/esm/icons/trash-2.mjs';
import { __iconNode as unplug } from 'lucide-react/dist/esm/icons/unplug.mjs';
import { __iconNode as x } from 'lucide-react/dist/esm/icons/x.mjs';
import type { WashIconNode } from '$lib/tool/wash-icon-node';

export type { WashIconNode };

export const washIcons = {
	sun,
	download,
	minus,
	square,
	copy,
	x,
	check,
	search,
	settings,
	shield,
	plug,
	unplug,
	'chevron-down': chevronDown,
	'refresh-cw': refreshCw,
	'folder-open': folderOpen,
	'file-plus': filePlus,
	'trash-2': trash2
} as const satisfies Record<string, WashIconNode>;
