import * as Stream from "./stream";
import { VarInt } from "./varint";

export const Type = {
	Padding: 0x00,
	Ping: 0x01,
	ResetStream: 0x04,
	StopSending: 0x05,
	Stream: 0x08, // Base type, actual value depends on flags
	ApplicationClose: 0x1d,
} as const;

export interface Data {
	type: "stream";
	id: Stream.Id;
	offset: VarInt;
	data: Uint8Array;
	fin: boolean;
}

export interface ResetStream {
	type: "reset_stream";
	id: Stream.Id;
	code: VarInt;
	size: VarInt;
}

export interface StopSending {
	type: "stop_sending";
	id: Stream.Id;
	code: VarInt;
}

export interface ConnectionClose {
	type: "connection_close";
	code: VarInt;
	reason: string;
}

export interface Padding {
	type: "padding";
}

export interface Ping {
	type: "ping";
}

export type Any =
	| Data
	| ResetStream
	| StopSending
	| ConnectionClose
	| Padding
	| Ping;

export function encode(frame: Any): Uint8Array {
	const chunks: Uint8Array[] = [];

	switch (frame.type) {
		case "padding":
			chunks.push(new Uint8Array([Type.Padding]));
			break;

		case "ping":
			chunks.push(new Uint8Array([Type.Ping]));
			break;

		case "stream": {
			// Calculate frame type based on flags
			let frameType = Type.Stream;
			if (frame.fin) frameType |= 0x01;
			if (frame.offset.value !== 0n) frameType |= 0x04;
			frameType |= 0x02; // Always set length bit

			chunks.push(new Uint8Array([frameType]));
			chunks.push(frame.id.value.encode());

			if (frame.offset.value !== 0n) {
				chunks.push(frame.offset.encode());
			}

			// Always encode length
			const length = VarInt.from(frame.data.length);
			chunks.push(length.encode());
			chunks.push(frame.data);
			break;
		}

		case "reset_stream":
			chunks.push(new Uint8Array([Type.ResetStream]));
			chunks.push(frame.id.value.encode());
			chunks.push(frame.code.encode());
			chunks.push(frame.size.encode());
			break;

		case "stop_sending":
			chunks.push(new Uint8Array([Type.StopSending]));
			chunks.push(frame.id.value.encode());
			chunks.push(frame.code.encode());
			break;

		case "connection_close":
			chunks.push(new Uint8Array([Type.ApplicationClose]));
			chunks.push(frame.code.encode());
			chunks.push(new TextEncoder().encode(frame.reason));
			break;
	}

	// Combine all chunks
	const totalLength = chunks.reduce((sum, chunk) => sum + chunk.length, 0);
	const result = new Uint8Array(totalLength);
	let offset = 0;
	for (const chunk of chunks) {
		result.set(chunk, offset);
		offset += chunk.length;
	}
	return result;
}

export function decode(buffer: Uint8Array): Any {
	let offset = 0;

	const frameTypeResult = VarInt.decode(buffer, offset);
	offset += frameTypeResult.bytesRead;
	const frameType = frameTypeResult.value.value;

	if (frameType === BigInt(Type.Padding)) {
		return { type: "padding" };
	}

	if (frameType === BigInt(Type.Ping)) {
		return { type: "ping" };
	}

	if (frameType === BigInt(Type.ResetStream)) {
		const idResult = VarInt.decode(buffer, offset);
		offset += idResult.bytesRead;
		const codeResult = VarInt.decode(buffer, offset);
		offset += codeResult.bytesRead;
		const sizeResult = VarInt.decode(buffer, offset);
		offset += sizeResult.bytesRead;

		return {
			type: "reset_stream",
			id: new Stream.Id(idResult.value),
			code: codeResult.value,
			size: sizeResult.value,
		};
	}

	if (frameType === BigInt(Type.StopSending)) {
		const idResult = VarInt.decode(buffer, offset);
		offset += idResult.bytesRead;
		const codeResult = VarInt.decode(buffer, offset);
		offset += codeResult.bytesRead;

		return {
			type: "stop_sending",
			id: new Stream.Id(idResult.value),
			code: codeResult.value,
		};
	}

	if (frameType === BigInt(Type.ApplicationClose)) {
		const codeResult = VarInt.decode(buffer, offset);
		offset += codeResult.bytesRead;
		const reasonBytes = buffer.slice(offset);
		const reason = new TextDecoder().decode(reasonBytes);

		return {
			type: "connection_close",
			code: codeResult.value,
			reason,
		};
	}

	// Stream frame (0x08-0x0f)
	if (frameType >= 0x08n && frameType <= 0x0fn) {
		const idResult = VarInt.decode(buffer, offset);
		offset += idResult.bytesRead;

		let frameOffset = VarInt.from(0);
		if ((frameType & 0x04n) !== 0n) {
			const offsetResult = VarInt.decode(buffer, offset);
			offset += offsetResult.bytesRead;
			frameOffset = offsetResult.value;
		}

		let dataLength: number;
		if ((frameType & 0x02n) !== 0n) {
			const lengthResult = VarInt.decode(buffer, offset);
			offset += lengthResult.bytesRead;
			dataLength = Number(lengthResult.value.value);
		} else {
			dataLength = buffer.length - offset;
		}

		const data = buffer.slice(offset, offset + dataLength);
		const fin = (frameType & 0x01n) !== 0n;

		return {
			type: "stream",
			id: new Stream.Id(idResult.value),
			offset: frameOffset,
			data,
			fin,
		};
	}

	throw new Error(`Invalid frame type: ${frameType}`);
}
