[![Documentation](https://docs.rs/web-transport-quinn/badge.svg)](https://docs.rs/web-transport-quinn/)
[![Crates.io](https://img.shields.io/crates/v/web-transport-quinn.svg)](https://crates.io/crates/web-transport-quinn)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE-MIT)

# web-transport-quinn

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
