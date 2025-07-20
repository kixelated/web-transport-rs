# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Development Commands

This project uses `just` as the task runner. Key commands:

- `just check` - Run all CI checks (clippy, format, audit, etc.) including WASM target validation
- `just test` - Run tests across all crates
- `just fix` - Auto-fix linting and formatting issues

**Important**: Always run `just check` before committing to ensure code quality and WASM compatibility.

## Project Architecture

This is a Rust workspace implementing WebTransport protocol support with platform-specific backends:

### Crate Structure
- **`web-transport`** - Generic interface that auto-selects platform implementation
- **`web-transport-proto`** - Core HTTP/3 protocol implementation for WebTransport session establishment
- **`web-transport-quinn`** - Native implementation using Quinn QUIC library (client + server)
- **`web-transport-wasm`** - Browser WebTransport API bindings (client only)

### Key Design Patterns
- Platform abstraction through conditional compilation
- Unified API that hides native/WASM differences
- Protocol layer separation from transport implementations
- QUIC streams (reliable, ordered) and datagrams (unreliable, unordered) as primary APIs

### WASM Considerations
- Use `--target wasm32-unknown-unknown` for WASM-specific builds
- Set `RUSTFLAGS="--cfg=web_sys_unstable_apis"` when working with browser APIs
- Only client functionality available in WASM (no server support)

## Testing Strategy
- Run `cargo test` for standard tests
- CI includes both native and WASM target validation
- Examples in `web-transport-quinn` provide integration test patterns
