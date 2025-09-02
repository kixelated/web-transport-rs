import { WebTransportSocket } from "./session";

// Install polyfill if WebTransport is not available, returning true if installed
export function install(): boolean {
    if ("WebTransport" in globalThis) return false;

    // biome-ignore lint/suspicious/noExplicitAny: polyfill
    (globalThis as any).WebTransport = WebTransportSocket;
    return true
}

export default WebTransportSocket;
