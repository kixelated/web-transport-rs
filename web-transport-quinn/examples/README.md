# Example

An example [server](echo-server.rs) and [client](echo-client.rs).

QUIC requires TLS, which makes the initial setup a bit more involved.

-   Generate a certificate: `./cert/generate`
-   Run the Rust server: `cargo run --example echo-server -- --tls-cert cert/localhost.crt --tls-key cert/localhost.key`
-   Run a Web client: `cd web; npm install; npx parcel serve client.html --open`

If you get a certificate error with the web client, try deleting `.parcel-cache`.

The Rust client example seems to be broken.
It would be amazing if somebody could fix it: `cargo run --example echo-client -- --tls-cert cert/localhost.crt`
