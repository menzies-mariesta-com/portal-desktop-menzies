/** Pub/sub: focus the active VPN profile and performance stats. */
type Listener = () => void;

const listeners = new Set<Listener>();

export function requestFocusConnected(): void {
	for (const listener of listeners) listener();
}

export function subscribeFocusConnected(listener: Listener): () => void {
	listeners.add(listener);
	return () => {
		listeners.delete(listener);
	};
}
