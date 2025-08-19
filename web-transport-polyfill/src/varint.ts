export class VarInt {
	static readonly MAX = (1n << 62n) - 1n
	static readonly MAX_SIZE = 8
	readonly value: bigint

	constructor(value: bigint) {
		if (value < 0n || value > VarInt.MAX) {
			throw new Error(`VarInt value out of range: ${value}`)
		}
		this.value = value
	}

	static from(value: number | bigint): VarInt {
		return new VarInt(BigInt(value))
	}

	size(): number {
		const x = this.value
		if (x < 2n ** 6n) return 1
		if (x < 2n ** 14n) return 2
		if (x < 2n ** 30n) return 4
		if (x < 2n ** 62n) return 8
		throw new Error("VarInt value too large")
	}

	encode(): Uint8Array {
		const x = this.value
		const buffer = new ArrayBuffer(this.size())
		const view = new DataView(buffer)

		if (x < 2n ** 6n) {
			view.setUint8(0, Number(x))
		} else if (x < 2n ** 14n) {
			view.setUint16(0, (0b01 << 14) | Number(x), false)
		} else if (x < 2n ** 30n) {
			view.setUint32(0, (0b10 << 30) | Number(x), false)
		} else if (x < 2n ** 62n) {
			view.setBigUint64(0, (0b11n << 62n) | x, false)
		} else {
			throw new Error("VarInt value too large")
		}

		return new Uint8Array(buffer)
	}

	static decode(buffer: Uint8Array, offset = 0): { value: VarInt; bytesRead: number } {
		if (offset >= buffer.length) {
			throw new Error("Unexpected end of buffer")
		}

		const view = new DataView(buffer.buffer, buffer.byteOffset + offset)
		const firstByte = view.getUint8(0)
		const tag = firstByte >> 6

		let value: bigint
		let bytesRead: number

		switch (tag) {
			case 0b00:
				value = BigInt(firstByte & 0b00111111)
				bytesRead = 1
				break
			case 0b01:
				if (offset + 2 > buffer.length) {
					throw new Error("Unexpected end of buffer")
				}
				value = BigInt(view.getUint16(0, false) & 0x3fff)
				bytesRead = 2
				break
			case 0b10:
				if (offset + 4 > buffer.length) {
					throw new Error("Unexpected end of buffer")
				}
				value = BigInt(view.getUint32(0, false) & 0x3fffffff)
				bytesRead = 4
				break
			case 0b11:
				if (offset + 8 > buffer.length) {
					throw new Error("Unexpected end of buffer")
				}
				value = view.getBigUint64(0, false) & 0x3fffffffffffffffn
				bytesRead = 8
				break
			default:
				throw new Error("Invalid VarInt tag")
		}

		return { value: new VarInt(value), bytesRead }
	}
}