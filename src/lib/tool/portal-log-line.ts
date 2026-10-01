/** Color rules for OpenVPN management-style connection log lines. */

export type LogSegment = {
	text: string;
	className: string;
};

export type ParsedLogLine = {
	/** daisyUI mockup-code prefix glyph */
	prefix: string;
	/** Semantic color for the mockup-code prefix (::before) */
	prefixClass: string;
	segments: LogSegment[];
};

const MUTED = 'text-base-content/55';
const BODY = 'text-base-content/80';
const INFO = 'text-info';
const SUCCESS = 'text-success';
const WARNING = 'text-warning';
const ERROR = 'text-error';
const ACCENT = 'text-secondary';

const KEYWORD_CLASS: Record<string, string> = {
	INFO: INFO,
	SUCCESS: SUCCESS,
	CONNECTED: SUCCESS,
	CONNECTING: WARNING,
	RECONNECTING: WARNING,
	DISCONNECT: WARNING,
	DISCONNECTED: WARNING,
	END: MUTED,
	BYTECOUNT: ACCENT,
	ERROR: ERROR,
	FATAL: ERROR,
	WARN: WARNING,
	WARNING: WARNING,
	AUTH: INFO,
	STATE: INFO
};

/** Strip a leading management-interface `>` so we do not double the mockup prefix. */
function stripLeadingChevron(raw: string): string {
	return raw.replace(/^\s*>+\s*/, '');
}

/**
 * Split a log line into colored spans.
 * Highlights OpenVPN keywords (INFO, SUCCESS, CONNECTED, END, BYTECOUNT, …)
 * and keeps the rest muted mono body text.
 */
export function parsePortalLogLine(raw: string): ParsedLogLine {
	const line = stripLeadingChevron(raw);
	const trimmed = line.trim();

	if (!trimmed) {
		return {
			prefix: '>',
			prefixClass: MUTED,
			segments: [{ text: line || ' ', className: MUTED }]
		};
	}

	// Standalone END
	if (/^END$/i.test(trimmed)) {
		return {
			prefix: '>',
			prefixClass: MUTED,
			segments: [{ text: trimmed, className: MUTED }]
		};
	}

	// BYTECOUNT:in,out
	const bytecount = trimmed.match(/^(BYTECOUNT(?:_CLI)?)\s*:\s*(.*)$/i);
	if (bytecount) {
		const label = bytecount[1] ?? 'BYTECOUNT';
		const rest = bytecount[2] ?? '';
		return {
			prefix: '>',
			prefixClass: ACCENT,
			segments: [
				{ text: label.toUpperCase(), className: ACCENT },
				{ text: rest ? `:${rest}` : '', className: BODY }
			].filter((s) => s.text.length > 0)
		};
	}

	// INFO:… / SUCCESS:… / ERROR:… / WARN:…
	const colonKind = trimmed.match(/^(INFO|SUCCESS|ERROR|FATAL|WARN|WARNING|AUTH)\s*:\s*(.*)$/i);
	if (colonKind) {
		const kind = (colonKind[1] ?? 'INFO').toUpperCase();
		const rest = colonKind[2] ?? '';
		const kindClass = KEYWORD_CLASS[kind] ?? INFO;
		return {
			prefix: '>',
			prefixClass: kindClass,
			segments: [
				{ text: kind, className: kindClass },
				{ text: rest ? `: ${rest}` : ':', className: BODY }
			]
		};
	}

	// Comma status rows: ts,CONNECTED,SUCCESS,ip,…
	if (trimmed.includes(',')) {
		const parts = trimmed.split(',');
		const segments: LogSegment[] = [];
		for (let i = 0; i < parts.length; i++) {
			const part = parts[i] ?? '';
			const upper = part.toUpperCase();
			const kw = KEYWORD_CLASS[upper];
			if (i > 0) segments.push({ text: ',', className: MUTED });
			if (kw) {
				segments.push({ text: part, className: kw });
			} else if (/^\d+$/.test(part)) {
				segments.push({ text: part, className: MUTED });
			} else if (/^\d{1,3}(?:\.\d{1,3}){3}$/.test(part)) {
				segments.push({ text: part, className: INFO });
			} else {
				segments.push({ text: part, className: BODY });
			}
		}
		const hasConnected = parts.some((p) => p.toUpperCase() === 'CONNECTED');
		const hasError = parts.some((p) => /^(ERROR|FATAL)$/i.test(p));
		return {
			prefix: '>',
			prefixClass: hasError ? ERROR : hasConnected ? SUCCESS : MUTED,
			segments
		};
	}

	// Generic: highlight known keywords in place
	const segments = highlightKeywords(line);
	const firstKw = Object.keys(KEYWORD_CLASS).find((k) =>
		new RegExp(`\\b${k}\\b`, 'i').test(trimmed)
	);
	return {
		prefix: '>',
		prefixClass: firstKw ? (KEYWORD_CLASS[firstKw] ?? MUTED) : MUTED,
		segments
	};
}

function highlightKeywords(line: string): LogSegment[] {
	const re =
		/\b(INFO|SUCCESS|CONNECTED|CONNECTING|RECONNECTING|DISCONNECTED|DISCONNECT|END|BYTECOUNT(?:_CLI)?|ERROR|FATAL|WARN|WARNING|AUTH|STATE)\b/gi;
	const segments: LogSegment[] = [];
	let last = 0;
	let match: RegExpExecArray | null;
	while ((match = re.exec(line)) !== null) {
		if (match.index > last) {
			segments.push({ text: line.slice(last, match.index), className: BODY });
		}
		const word = match[0] ?? '';
		const key = word.toUpperCase().replace(/_CLI$/, '');
		segments.push({
			text: word,
			className: KEYWORD_CLASS[key] ?? KEYWORD_CLASS[word.toUpperCase()] ?? BODY
		});
		last = match.index + word.length;
	}
	if (last < line.length) {
		segments.push({ text: line.slice(last), className: BODY });
	}
	if (segments.length === 0) {
		segments.push({ text: line, className: BODY });
	}
	return segments;
}
