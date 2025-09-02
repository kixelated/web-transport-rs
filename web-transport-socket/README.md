# WebTransport Polyfill

A WebTransport polyfill that uses WebSocket as the underlying transport, with implementations in both Rust and TypeScript/JavaScript.

## Structure

This package contains both Rust and TypeScript implementations that share the same wire protocol:

- `src/` - Rust implementation
- `src-js/` - TypeScript/JavaScript implementation
- `Cargo.toml` - Rust package configuration
- `package.json` - Node.js package configuration

## Wire Protocol

Both implementations use the same QUIC-like frame encoding over WebSocket:
- Variable-length integer encoding (VarInt)
- Stream multiplexing with bidirectional and unidirectional streams
- Frame types: STREAM, RESET_STREAM, STOP_SENDING, etc.

## JavaScript/TypeScript Usage

The polyfill automatically installs itself when WebTransport is not available:

```javascript
import "@kixelated/web-transport-polyfill"

// Now WebTransport is available even in Safari
const transport = new WebTransport("https://example.com/path")
```

URLs are automatically rewritten to include `/ws` prefix for WebSocket compatibility:
- `https://example.com/path` → `wss://example.com/ws/path`

## Building

### Rust
```bash
cargo build
```

### TypeScript/JavaScript
```bash
npm install
npm run build
```

## License

MIT OR Apache-2.0