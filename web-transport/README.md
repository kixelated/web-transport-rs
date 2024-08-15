[![crates.io](https://img.shields.io/crates/v/web-transport)](https://crates.io/crates/web-transport)
[![docs.rs](https://img.shields.io/docsrs/web-transport)](https://docs.rs/web-transport)
[![discord](https://img.shields.io/discord/1124083992740761730)](https://discord.gg/FCYF3p99mr)

# web-transport

[WebTransport](https://developer.mozilla.org/en-US/docs/Web/API/WebTransport_API) is a new browser API powered by [QUIC](https://www.rfc-editor.org/rfc/rfc9000.html) intended as a replacement for WebSockets.
Most importantly, QUIC supports multiple independent data streams.

This crate provides a generic WebTransport implementation depending on the platform:

-   Native: [web-transport-quinn](../web-transport-quinn)
-   WASM: [web-transport-wasm](../web-transport-wasm)

## Why no trait?

[I did make a generic trait](https://docs.rs/webtransport-generic/latest/webtransport_generic/). However, async traits are quite problematic and difficult to use.
It shortly became impossible when trying to add WASM support because of `!Send`.

So this crate switches the implementation based on the underlying platform.
As an added benefit, you no longer need to litter your code with generics.
