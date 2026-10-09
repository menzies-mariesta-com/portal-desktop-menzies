/** Pub/sub: focus a connected VPN profile (OpenVPN or GlobalProtect). */

export type FocusProtocolPayload = {
	protocol: 'openvpn' | 'globalprotect';
	profileId?: string | null;
};

type Listener = (payload?: FocusProtocolPayload) => void;

const listeners = new Set<Listener>();

export function requestFocusConnected(payload?: FocusProtocolPayload): void {
	for (const listener of listeners) listener(payload);
}

export function subscribeFocusConnected(listener: Listener): () => void {
	listeners.add(listener);
	return () => {
		listeners.delete(listener);
	};
}
