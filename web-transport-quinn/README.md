[![crates.io](https://img.shields.io/crates/v/web-transport-quinn)](https://crates.io/crates/web-transport-quinn)
[![docs.rs](https://img.shields.io/docsrs/web-transport-quinn)](https://docs.rs/web-transport-quinn)
[![discord](https://img.shields.io/discord/1124083992740761730)](https://discord.gg/FCYF3p99mr)

# web-transport-quinn
A wrapper around the Quinn API, abstracting away the annoying HTTP/3 internals.
Provides a QUIC-like API but with web support!

## Example

See the example [server](examples/echo-server.rs) and [client](examples/echo-client.rs).

QUIC requires TLS, which makes the initial setup a bit more involved.

-   Generate a certificate: `./cert/generate`
-   Run the Rust server: `cargo run --example echo-server -- --tls-cert cert/localhost.crt --tls-key cert/localhost.key`
-   Run a Web client: `cd web; npm install; npx parcel serve client.html --open`

If you get a certificate error with the web client, try deleting `.parcel-cache`.

The Rust client example seems to be broken.
It would be amazing if somebody could fix it: `cargo run --example echo-client -- --tls-cert cert/localhost.crt`

## Limitations

This library doesn't support pooling HTTP/3 or multiple WebTransport sessions.
It's means to be analogous to the QUIC API.

-   If you want to support HTTP/3 on the same host/port, you should use another crate (ex. `h3-webtransport`).
-   If you want to support multiple WebTransport sessions over the same QUIC connection... you should just dial a new QUIC connection instead.
