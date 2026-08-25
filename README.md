# Sailing game

This repository contains an early multiplayer, top-down naval combat game with
readable, semi-realistic sailing. The browser and authoritative server provide a
deterministic rectangular procedural world whose terrain and seeded procedural
fields repeat seamlessly. Each rendered position receives its own longitude,
latitude-calibrated climate and opposite-hemisphere season, surface temperature,
snow and ice, astronomical tide, and celestial lighting, alongside coherent
coasts, physical depth, pressure-driven weather, and spatially formed clouds
whose thickness varies their opacity and shadows. Prediction, ramming, damage,
and persistent physical wrecks remain deterministic. Boats sail repeated
toroidal laps and remain visible, predictable, and physically interactive across
both seams and their corners.

The intended game grows this foundation into persistent, drop-in matches with
varied ships, mostly automatic weapons, progression, and a vast procedural sea.
See [product decisions](PRODUCT.md) for committed scope rather than treating the
current prototype as the complete design.

## Start here

Development needs Rust with the `wasm32-unknown-unknown` target, `wasm-pack`, and
Bun. The optional Nix shell provides the complete toolchain on `x86_64-linux`.

```sh
nix develop       # optional
bun install
```

Choose what you want to run:

| Goal | Command |
| --- | --- |
| Procedural playground | `bun run dev:playground` |
| Authoritative server | `cargo run -p server` |
| Browser workspace | `bun run dev` |

### Browser multiplayer

1. Start the authoritative server:

   ```sh
   cargo run -p server
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

## Controls

| Input | Action |
| --- | --- |
| `W` / `S`, `A` / `D` | Forward / reverse power, turn |
| Gamepad left stick | World-relative movement |
| Mouse wheel / middle mouse | Zoom / reset zoom |

The browser HUD reports sailing and under-keel conditions. Destroyed browser
boats remain in the match as physical wrecks, with their owners spectating them.

Additional notes cover [browser TLS](web/README.md) and
[server deployment](crates/server/README.md).

## Project guide

| Document | Use it for |
| --- | --- |
| [Product decisions](PRODUCT.md) | Intended player experience, scope, and unresolved product choices |
| [Architecture decisions](ARCHITECTURE.md) | Durable technical boundaries and unresolved architecture choices |
| [Maintainer guide](MAINTAINERS.md) | Verification and repository navigation |
