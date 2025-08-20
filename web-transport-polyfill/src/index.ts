import { WebTransportSession } from "./session";

// Install polyfill if WebTransport is not available
export function install() {
    if (typeof globalThis !== "undefined" && !("WebTransport" in globalThis)) {
        // biome-ignore lint/suspicious/noExplicitAny: polyfill
		(globalThis as any).WebTransport = WebTransportSession;
	}
}

export default WebTransportSession;
