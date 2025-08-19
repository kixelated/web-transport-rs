import { WebTransportSession } from "./session";

// Install polyfill if WebTransport is not available
//if (typeof globalThis !== "undefined" && !("WebTransport" in globalThis)) {
	// biome-ignore lint/suspicious/noExplicitAny: polyfill
	(globalThis as any).WebTransport =
		WebTransportSession;
//}

// For environments that support it, also export as default
export default WebTransportSession;
