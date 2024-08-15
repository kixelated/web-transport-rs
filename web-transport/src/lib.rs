//! A generic WebTransport interface.
//!
//! The underlying implementation switches based on the platform:
//!  - native: [web-transport-quinn](https://docs.rs/web-transport-quinn/latest)
//!  - web: [web-transport-wasm](https://docs.rs/web-transport-wasm/latest)
//!
//! There is currently no generic way to establish a session.
//! Use the above libraries directly, then use [Session::from()] to cast to this generic interface.

#[cfg(any(not(target_arch = "wasm32"), target_os = "wasi"))]
#[path = "quinn.rs"]
mod quic;

#[cfg(all(target_arch = "wasm32", not(target_os = "wasi")))]
#[path = "wasm.rs"]
mod quic;

pub use quic::*;
