// @ts-ignore embed the certificate fingerprint using bundler
import fingerprintHex from 'bundle-text:../cert/localhost.hex';

// Convert the hex to binary.
let fingerprint = [];
for (let c = 0; c < fingerprintHex.length - 1; c += 2) {
    fingerprint.push(parseInt(fingerprintHex.substring(c, c + 2), 16));
}

const params = new URLSearchParams(window.location.search)

const url = params.get("url") || "https://localhost:4443"

function log(msg) {
    const element = document.createElement("div");
    element.innerText = msg;

    document.body.appendChild(element);
}

async function run() {
    // Connect using the hex fingerprint in the cert folder.
    const transport = new WebTransport(url, {
        serverCertificateHashes: [{
            "algorithm": "sha-256",
            "value": new Uint8Array(fingerprint),
        }],
    });
    await transport.ready;

    log("connected");

    // Create a bidirectional stream
    const stream = await transport.createBidirectionalStream();

    log("created stream");

    // Write a message to it
    const msg = 'Hello, world!';
    const writer = stream.writable.getWriter();
    await writer.write(new TextEncoder().encode(msg));
    await writer.close();
    writer.releaseLock();

    log("send: " + msg);

    // Read a message from it
    const reader = stream.readable.getReader();
    const { value } = await reader.read();

    const recv = new TextDecoder().decode(value);
    log("recv: " + recv);

    await transport.close();
    log("closed");
}

run();
