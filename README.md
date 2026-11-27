# Sailing game

A browser sailing game with multiplayer naval combat. The prototype supports
steering, ramming, collision damage, and persistent wrecks in a procedural
world, whose islands a drone-like camera shows in perspective. Wooden hull
sprites carry cloth sails that fill or luff with apparent wind. Day/night
lighting models eye sensitivity and color as illumination changes; darkness
can hide unlit boats and coasts. Supported browsers and displays show HDR
highlights. The local wind raises a fully developed sea that lifts, tilts and
carries boats; swell from distant storms isn't drawn, but it does the same
and can ground them in its troughs. Water mirrors the sky and
glints in sunlight and moonlight off short wind chop that travels across
it, duller where gusts or rain roughen it; its depth,
its bed, and the sediment waves stir set its color. Whitecaps that break
and leave drifting, fading foam are off by default; the playground's
Whitecaps and foam control shows them. Ground darkens where
tide, waves, dew, or humid and rainy weather wet it. Boats cast and receive
shadows, including on their wooden mast and rigging. Storm lightning lights
the scene from its position and casts shadows.

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
sailor with keyboard, gamepad, or a draggable pointer stick. Try Force trim
and Load-aware helm on the same boat. Repeat start restores its spawn position
and weather clock for another run. Forward intent controls sail power and turn
intent controls the rudder. Driving uses real time or pause. Removing the boat
restores the clock controls.

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

The Settings gear opens from the menu and during play. Changes apply immediately
and save on this device. Scene resolution follows CSS pixels by default, with
a 2× upscale limit. Boats stay at display resolution. Touch screens default to
lower detail, a frame-rate cap, and adaptive resolution.

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
Choose a live player to follow while spectating. Escape or the corner menu
button opens the menu during play. Resume returns to your boat. Leave boat
returns to spectating. Join spawns a new boat. Abandoned and destroyed boats
persist as wrecks. Destruction or connection failure returns to the menu with
a reason and an action to join again or reconnect.

Gameplay has a fixed zoom and a compact player list. The scale appears with the
world view, and a tiny FPS counter appears at the top left once measured.

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
