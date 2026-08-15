# Pyboat

Pyboat is an early multiplayer, top-down naval combat game with readable,
semi-realistic sailing. The playable code currently has two paths:

| Path | State | Experience |
| --- | --- | --- |
| Rust browser + server | Active target | Shared procedural world, authoritative multiplayer, prediction, ramming, damage, wrecks, weather, and terrain |
| Python desktop | Retained reference | Standalone buoy course and multiplayer sailing, ramming, damage, and wrecks |

The intended game grows this foundation into persistent, drop-in matches with
varied ships, mostly automatic weapons, progression, and a vast procedural sea.
See [product decisions](PRODUCT.md) for committed scope rather than treating the
current prototype as the complete design.

## Start here

The Python path needs Python and `uv`. The browser path needs Rust with the
`wasm32-unknown-unknown` target, `wasm-pack`, and Bun. The optional Nix shell
provides the complete development toolchain on `x86_64-linux`.

```sh
nix develop       # optional
uv sync
bun install
```

Choose what you want to run:

| Goal | Command |
| --- | --- |
| Procedural playground | `bun run dev:playground` |
| Python standalone | `uv run game` |
| Python server | `uv run server` |
| Python client | `uv run client` |

The Python standalone game is a four-buoy time trial. Pass the numbered buoys in
order. `R` resets the boat and current lap while preserving the best completed
lap.

### Browser multiplayer

1. Start the authoritative server:

   ```sh
   cargo run -p pyboat-server
   ```

2. In another terminal, start the browser workspace:

   ```sh
   bun run dev
   ```

3. Copy the server's complete `Browser certificate hash` and append it to the
   Vite URL:

   ```text
   http://localhost:5173/?webtransportCertificateHash=d7:3a:...:72:48
   ```

4. Open that URL in a WebGPU- and WebTransport-capable browser.

The development certificate changes whenever the server restarts. Replace the
hash and reload the page after each restart.

### Python multiplayer

Start `uv run server`, then run `uv run client` in each additional terminal.

## Controls

| Context | Input | Action |
| --- | --- | --- |
| Browser and Python multiplayer | `W` / `S`, `A` / `D` | Forward / reverse power, turn |
| Browser | Gamepad left stick | World-relative movement |
| Browser | Mouse wheel / middle mouse | Zoom / reset zoom |
| Python standalone, modes `1` and `3` | `W` / `S`, `A` / `D` | Forward / reverse power, turn |
| Python standalone, mode `2` | `W`, `A` / `D` | Sail power, direct rudder control |
| Python standalone, modes `1` and `3` | Gamepad left stick | World-relative movement |
| Python standalone, mode `2` | Gamepad left stick | Sail power and direct rudder control |
| Python standalone | `1` / `2` / `3` | Computed yaw-rate / auto-sheet / fixed yaw-rate control |
| Python standalone | `R` | Reset the course |

The browser HUD reports sailing and under-keel conditions. Destroyed browser
boats remain in the match as physical wrecks, with their owners spectating them.
The retained Python client instead exits after local destruction or disconnect.

Additional notes cover [browser TLS](web/README.md) and
[server deployment](crates/server/README.md).

## Project guide

| Document | Use it for |
| --- | --- |
| [Product decisions](PRODUCT.md) | Intended player experience, scope, and unresolved product choices |
| [Architecture decisions](ARCHITECTURE.md) | Durable technical boundaries and unresolved architecture choices |
| [Maintainer guide](MAINTAINERS.md) | Verification and repository navigation |
