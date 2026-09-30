/** Titlebar refresh pub/sub for Portal. */
type Listener = () => void;

const listeners = new Set<Listener>();

export function requestPortalRefresh(): void {
	for (const listener of listeners) listener();
}

export function subscribePortalRefresh(listener: Listener): () => void {
	listeners.add(listener);
	return () => {
		listeners.delete(listener);
	};
}
