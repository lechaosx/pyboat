# Sailing game

A browser sailing game with multiplayer naval combat. The prototype supports
steering, ramming, collision damage, and persistent wrecks in a procedural
world, whose islands a drone-like camera shows in perspective. Wooden hull
sprites carry cloth sails that fill or luff with apparent wind. Day/night
lighting uses gradual eye adaptation to the brightness and color of the light
falling on each part of the view, with HDR highlights on supported browsers and
displays. The local wind raises a fully developed sea that lifts, tilts and
carries boats; swell from distant storms isn't drawn, but it does the same
and can ground them in its troughs. Water mirrors the sky and
glints in sunlight and moonlight; its depth, its bed, and the sediment waves
stir set its color; ground darkens where tide, waves, dew, or humid and rainy
weather wet it. Boats cast and receive shadows, including on their wooden mast
and rigging. Storm lightning lights the scene from its position and casts shadows.

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

Run `npm run dev` to serve the game at `/` and the procedural playground at
`/playground/`. The playground provides a live preview and controls for inspecting
the world and tuning its appearance; it needs no backend.
“Drive a boat” spawns a boat in safe nearby water, using the game’s Simple
sailor with keyboard, gamepad, or a draggable pointer stick. Forward intent
controls sail power; turn intent controls the rudder. Boat driving uses real
time or pause, and removing the boat restores the clock controls.

For GitHub Pages and backend deployment, follow the
[hosting setup](crates/server/README.md#hosting).

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

The Settings button changes detail, resolution, and frame rate immediately,
without leaving the game, and saves choices on this device. Scene resolution
defaults to CSS pixels, bounded by the 2× upscale limit; boats stay at display
resolution. Touch screens default to lower detail, a frame-rate cap, and
automatic resolution.

Sound is currently unavailable in the game and playground.

Startup URL overrides remain available: `quality` (`low`, `medium`, or `high`),
`renderScale` (scene fraction of output resolution), `pixelRatio` (device pixel
ratio cap), `fps` (frame-rate cap, 0 for none), and `autoResolution` (1 to lower
the scale when frames run long).

## Controls

| Input | Action |
| --- | --- |
| `W` / `S`, `A` / `D` | Forward / reverse power, turn |
| Gamepad left stick | World-relative movement |
| Touch / left mouse drag | World-relative movement; floating wheel shows rudder position |

The menu shows loading stages over the live sea. Keep the generated nickname
or choose your own, then Join when ready; the camera moves to your spawned boat.
Before joining or after destruction, choose a live player to spectate. Escape
or the corner menu button opens the menu during play; Resume returns to your
boat. Settings opens inside the same dialog. Destruction or connection failure
returns to the menu with an action to join again or reconnect. Wrecks persist.

Gameplay has a fixed zoom, a compact player list, a tiny FPS counter, and a scale
indicator once the view is known.

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
