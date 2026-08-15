# Browser development notes

Follow the
[root quick start](../README.md#browser-multiplayer) to run it.

## Local TLS

The Vite page may use HTTP on loopback, but WebTransport itself is always
encrypted. A server handshake failure ending in TLS alert 42 usually means the
certificate hash in the URL is absent or stale.

Omit the query parameter when the server uses a certificate already trusted by
the browser.
