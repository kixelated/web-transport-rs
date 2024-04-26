//! A generic WebTransport interface.
//!
//! The underlying implementation switches based on the platform:
//!  - native: [web_transport_quinn]
//!  - web: [web_transport_wasm](https://github.com/kixelated/web-transport-rs/tree/main/web-transport-wasm)
//!
//! Currently, you have to use either of those traits to accept (server) or create (client) a session.
//! Then you can use [Session::from()] to cast to this generic interface.

#[cfg(any(not(target_arch = "wasm32"), target_os = "wasi"))]
#[path = "quinn.rs"]
mod quic;

#[cfg(all(target_arch = "wasm32", not(target_os = "wasi")))]
#[path = "wasm.rs"]
mod quic;

pub use quic::*;
