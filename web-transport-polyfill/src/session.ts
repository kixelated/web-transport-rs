import * as Frame from "./frame";
//import { WebTransportReceiveStream } from "./receive-stream";
import * as Stream from "./stream";
//import { WebTransportSendStream } from "./stream";
import { VarInt } from "./varint";

export class WebTransportSession implements WebTransport {
	#ws: WebSocket;
	#isServer = false;
	#closed?: Error;
	#closeReason?: Error;

	#sendStreams = new Map<bigint, WritableStreamDefaultController>();
	#recvStreams = new Map<bigint, ReadableStreamDefaultController<Uint8Array>>();

	#nextUniStreamId = 0n;
	#nextBiStreamId = 0n;

	readonly ready: Promise<void>;
	#readyResolve!: () => void;
	readonly closed: Promise<WebTransportCloseInfo>;
	#closedResolve!: (info: WebTransportCloseInfo) => void;

    readonly incomingBidirectionalStreams: ReadableStream<WebTransportBidirectionalStream>;
    #incomingBidirectionalStreams!: ReadableStreamDefaultController<WebTransportBidirectionalStream>;
    readonly incomingUnidirectionalStreams: ReadableStream<ReadableStream<Uint8Array>>;
    #incomingUnidirectionalStreams!: ReadableStreamDefaultController<ReadableStream<Uint8Array>>;

    // TODO: Implement datagrams
	readonly datagrams = new Datagrams();

	constructor(url: string | URL) {
		url = WebTransportSession.#convertToWebSocketUrl(url);

		this.#ws = new WebSocket(url, ["webtransport"]);

		this.ready = new Promise((resolve) => {
			this.#readyResolve = resolve;
		});

		this.closed = new Promise((resolve) => {
			this.#closedResolve = resolve;
		});

		this.#ws.binaryType = "arraybuffer";
		this.#ws.onopen = () => this.#readyResolve();
		this.#ws.onmessage = (event) => this.#handleMessage(event);
		this.#ws.onerror = (event) => this.#handleError(event);
		this.#ws.onclose = (event) => this.#handleClose(event);

        this.incomingBidirectionalStreams = new ReadableStream<WebTransportBidirectionalStream>({
            start: (controller) => {
                this.#incomingBidirectionalStreams = controller;
            },
        });

        this.incomingUnidirectionalStreams = new ReadableStream<ReadableStream<Uint8Array>>({
            start: (controller) => {
                this.#incomingUnidirectionalStreams = controller;
            },
        });
	}

	static #convertToWebSocketUrl(url: string | URL): string {
		const urlObj = typeof url === "string" ? new URL(url) : url;

		// Convert https:// to wss:// and http:// to ws://
		let protocol = urlObj.protocol;
		if (protocol === "https:") {
			protocol = "wss:";
		} else if (protocol === "http:") {
			protocol = "ws:";
		} else if (protocol !== "ws:" && protocol !== "wss:") {
			throw new Error(`Unsupported protocol: ${protocol}`);
		}

		// Build WebSocket URL
		return `${protocol}//${urlObj.host}/ws${urlObj.pathname}${urlObj.search}`;
	}

	#handleMessage(event: MessageEvent) {
		if (!(event.data instanceof ArrayBuffer)) return;

		const data = new Uint8Array(event.data);
		try {
			const frame = Frame.decode(data);
			this.#processFrame(frame);
		} catch (error) {
			console.error("Failed to decode frame:", error);
			this.close({ closeCode: 1002, reason: "Protocol violation" });
		}
	}

	#handleError(event: Event) {
		if (this.#closed) return;

		this.#closed = new Error(`WebSocket error: ${event.type}`);
		this.#closedResolve({
			closeCode: 1006,
			reason: "WebSocket error",
		});
	}

	#handleClose(event: CloseEvent) {
		if (this.#closed) return;

        console.log("WebSocket close event - code:", event.code, "reason:", event.reason || "(no reason)");

		this.#closed = new Error(
			`Connection closed: ${event.code} ${event.reason}`,
		);

		this.#closedResolve({
			closeCode: event.code,
			reason: event.reason,
		});
	}

	#processFrame(frame: Frame.Any) {
		if (frame.type === "padding" || frame.type === "ping") {
			// No-op
		} else if (frame.type === "stream") {
			this.#handleStreamFrame(frame);
		} else if (frame.type === "reset_stream") {
			this.#handleResetStream(frame);
		} else if (frame.type === "stop_sending") {
			this.#handleStopSending(frame);
		} else if (frame.type === "connection_close") {
			this.#closeReason = new Error(
				`Connection closed: ${frame.code.value}: ${frame.reason}`,
			);
			this.#ws.close();
		} else {
			const exhaustive: never = frame;
			throw new Error(`Unknown frame type: ${exhaustive}`);
		}
	}

	#handleStreamFrame(frame: Frame.Data) {
		const streamId = frame.id.value.value;

		if (!frame.id.canRecv(this.#isServer)) {
			throw new Error("Invalid stream ID direction");
		}

		const stream = this.#recvStreams.get(streamId);
		if (!stream) {
			// We created the stream, we can skip it.
			if (frame.id.serverInitiated === this.#isServer) return;
			if (!frame.id.canRecv(this.#isServer)) {
				throw new Error("received write-only stream");
			}

			const reader = new ReadableStream<Uint8Array>({
				start: (controller) => {
					this.#recvStreams.set(streamId, controller);
				},
				cancel: () => {
					this.#sendPriorityFrame({
						type: "stop_sending",
						id: frame.id,
						code: VarInt.from(0),
					});
				},
			});

			if (frame.id.dir === Stream.Dir.Bi) {
				let offset = 0n;

				// Incoming bidirectional stream
				const writer = new WritableStream<Uint8Array>({
					start: (controller) => {
						this.#sendStreams.set(streamId, controller);
					},
					write: async (chunk) => {
						await Promise.race([
							this.#sendFrame({
								type: "stream",
								id: frame.id,
								offset: VarInt.from(0),
								data: chunk,
								fin: false,
							}),
							this.closed,
						]);
						offset += BigInt(chunk.length);
					},
					abort: async () => {
						this.#sendPriorityFrame({
							type: "reset_stream",
							id: frame.id,
							code: VarInt.from(0),
							size: VarInt.from(offset),
						});
					},
					close: async () => {
						await Promise.race([
							this.#sendFrame({
								type: "stream",
								id: frame.id,
								offset: VarInt.from(offset),
								data: new Uint8Array(),
								fin: true,
							}),
							this.closed,
						]);
					},
				});

                this.#incomingBidirectionalStreams.enqueue({ readable: reader, writable: writer });
			} else {
                this.#incomingUnidirectionalStreams.enqueue(reader);
            }
		}

		stream?.enqueue(frame.data);

		if (frame.fin) {
			stream?.close();
            this.#recvStreams.delete(streamId);
		}
	}

	#handleResetStream(frame: Frame.ResetStream) {
		const streamId = frame.id.value.value;
		const stream = this.#recvStreams.get(streamId);
		if (!stream) return;

        stream.error(new Error(`RESET_STREAM: ${frame.code.value}`));
        this.#recvStreams.delete(streamId);
	}

	#handleStopSending(frame: Frame.StopSending) {
		const streamId = frame.id.value.value;
		const stream = this.#sendStreams.get(streamId);
		if (!stream) return;

        stream.error(new Error(`STOP_SENDING: ${frame.code.value}`));
        this.#sendStreams.delete(streamId);

        this.#sendPriorityFrame({
            type: "reset_stream",
            id: frame.id,
            code: frame.code,
            // TODO get the correct finaly size
            size: new VarInt(0n),
        });
	}

	async #sendFrame(frame: Frame.Any) {
		const encoded = Frame.encode(frame);
		// Add some backpressure so we don't saturate the connection
		while (this.#ws.bufferedAmount > 64 * 1024) {
			await new Promise((resolve) => setTimeout(resolve, 10));
		}
		this.#ws.send(encoded);
	}

	#sendPriorityFrame(frame: Frame.Any) {
		const encoded = Frame.encode(frame);
		this.#ws.send(encoded);
	}

	async createBidirectionalStream(): Promise<WebTransportBidirectionalStream> {
		await this.ready;

		if (this.#closed) {
			throw this.#closeReason || new Error("Connection closed");
		}

		const streamId = Stream.Id.create(
			this.#nextBiStreamId++,
			Stream.Dir.Bi,
			this.#isServer,
		);

		const writer = new WritableStream<Uint8Array>({
			start: (controller) => {
				this.#sendStreams.set(streamId.value.value, controller);
			},
			write: async (chunk) => {
				await Promise.race([
					this.#sendFrame({
						type: "stream",
						id: streamId,
						offset: VarInt.from(0),
						data: chunk,
						fin: false,
					}),
					this.closed,
				]);
			},
			abort: async () => {
				this.#sendPriorityFrame({
					type: "reset_stream",
					id: streamId,
					code: VarInt.from(0),
					size: VarInt.from(0),
				});

				this.#sendStreams.delete(streamId.value.value);
			},
			close: async () => {
				await Promise.race([
					this.#sendFrame({
						type: "stream",
						id: streamId,
						offset: VarInt.from(0),
						data: new Uint8Array(),
						fin: true,
					}),
					this.closed,
				]);

				this.#sendStreams.delete(streamId.value.value);
			},
		});

		const reader = new ReadableStream<Uint8Array>({
			start: (controller) => {
				this.#recvStreams.set(streamId.value.value, controller);
			},
			cancel: async () => {
				this.#sendPriorityFrame({
					type: "stop_sending",
					id: streamId,
					code: VarInt.from(0),
				});

				this.#recvStreams.delete(streamId.value.value);
			},
		});

		return { readable: reader, writable: writer };
	}

	async createUnidirectionalStream(): Promise<WritableStream<Uint8Array>> {
		await this.ready;

		if (this.#closed) {
			throw this.#closed;
		}

		const streamId = Stream.Id.create(
			this.#nextUniStreamId++,
			Stream.Dir.Uni,
			this.#isServer,
		);

		let offset = 0n;
		const session = this;

		const writer = new WritableStream<Uint8Array>({
			start: (controller) => {
				session.#sendStreams.set(streamId.value.value, controller);
			},
			async write(chunk) {
				await Promise.race([
					session.#sendFrame({
						type: "stream",
						id: streamId,
						offset: VarInt.from(offset),
						data: chunk,
						fin: false,
					}),
					session.closed,
				]);
				offset += BigInt(chunk.length);
			},
			async abort() {
				session.#sendPriorityFrame({
					type: "reset_stream",
					id: streamId,
					code: VarInt.from(0),
					size: VarInt.from(offset),
				});

				session.#sendStreams.delete(streamId.value.value);
			},
			async close() {
				await Promise.race([
					session.#sendFrame({
						type: "stream",
						id: streamId,
						offset: VarInt.from(offset),
						data: new Uint8Array(),
						fin: true,
					}),
					session.closed,
				]);

				session.#sendStreams.delete(streamId.value.value);
			},
		});

		return writer;
	}


	close(info?: { closeCode?: number; reason?: string }) {
		if (this.#closed) return;

		const code = info?.closeCode ?? 0;
		const reason = info?.reason ?? "";

		this.#sendPriorityFrame({
			type: "connection_close",
			code: VarInt.from(code),
			reason,
		});

		setTimeout(() => {
			this.#ws.close();
		}, 100);
	}

	get congestionControl(): string {
		return "default";
	}
}

// TODO Implement this
export class Datagrams implements WebTransportDatagramDuplexStream {
	incomingHighWaterMark: number;
	incomingMaxAge: number | null;
	readonly maxDatagramSize: number;
	outgoingHighWaterMark: number;
	outgoingMaxAge: number | null;
	readonly readable: ReadableStream;
	readonly writable: WritableStream;

	constructor() {
		this.incomingHighWaterMark = 1024;
		this.incomingMaxAge = null;
		this.maxDatagramSize = 1200;
		this.outgoingHighWaterMark = 1024;
		this.outgoingMaxAge = null;
		this.readable = new ReadableStream<Uint8Array>({});
		this.writable = new WritableStream<Uint8Array>({});
	}
}
