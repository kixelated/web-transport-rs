# Example

A simple [server](echo-server.rs) and [client](echo-client.rs).

There's also advanced examples [server](echo-server-advanced.rs) and [client](echo-client-advanced.rs) that construct the QUIC connection manually.

QUIC requires TLS, which makes the initial setup a bit more involved.

-   Generate a certificate: `./cert/generate`
-   Run the Rust server: `cargo run --example echo-server -- --tls-cert cert/localhost.crt --tls-key cert/localhost.key`
-   Run the Rust client: `cargo run --example echo-client -- --tls-cert cert/localhost.crt`
-   Run a Web client: `cd web; npm install; npx parcel serve client.html --open`

If you get a certificate error with the web client, try deleting `.parcel-cache`.
