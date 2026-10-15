# Sailing game

A browser sailing game with multiplayer naval combat. The prototype supports
steering, ramming, collision damage, and persistent wrecks in a procedural world.
Day/night lighting uses gradual exposure adaptation, with HDR highlights on
supported browsers and displays.

The intended game adds varied ships, mostly automatic weapons, and progression.
[Product decisions](PRODUCT.md) describe that scope.

## Start here

You need Rust, its `wasm32-unknown-unknown` target, `wasm-pack`, `cargo-about`
(for the web build's third-party notices), and Node.js/npm versions matching
[package.json](package.json). On `x86_64-linux`, the optional
Nix shell provides the toolchain:

```sh
nix develop       # optional
npm ci
```

For the procedural playground, run `npm run dev:playground`. It provides a live
preview and controls for inspecting the world and tuning its appearance.

### Browser multiplayer

1. Run `cargo run -p server`.
2. In another terminal, run `npm run dev`.
3. Copy the server's complete `Browser certificate hash` into the browser URL:

   ```text
   http://localhost:5173/?webtransportCertificateHash=d7:3a:...:72:48
   ```

4. Open the URL in a browser with WebGPU and WebTransport support.

Replace the hash and reload after every server restart. TLS alert 42 usually
means the hash is missing or stale. Omit it when using a browser-trusted server
certificate; the local Vite page can use HTTP, but WebTransport stays encrypted.

Graphics settings are URL parameters: `quality` (`low`, `medium`, or `high`
detail), `renderScale` (scene resolution fraction), `pixelRatio` (device pixel
ratio cap), `fps` (frame-rate cap, 0 for none), and `autoResolution` (1 to lower
the scale when frames run long). Touch screens default to low quality, a reduced
scale, a frame-rate cap, and automatic resolution.

## Controls

| Input | Action |
| --- | --- |
| `W` / `S`, `A` / `D` | Forward / reverse power, turn |
| Gamepad left stick | World-relative movement |
| Mouse wheel / middle mouse | Zoom / reset zoom |

The HUD reports sailing and under-keel conditions. Destroyed boats remain as
physical wrecks while their owners spectate.

## Project guide

| Read | For |
| --- | --- |
| [Product decisions](PRODUCT.md) | Player experience, future scope, and open product questions |
| [Architecture decisions](ARCHITECTURE.md) | Technical boundaries, contracts, and open technical questions |
| [Maintainer guide](MAINTAINERS.md) | Checks, visual verification, and source navigation |
| [Server guide](crates/server/README.md) | Listen address and deployed certificates |
| [Engineering guidelines](AGENTS.md) | Change, documentation, and testing policy |

Implementation details and calibration rationale live beside the relevant code;
accepted behavior lives in tests. Use the maintainer guide to find those sources.
