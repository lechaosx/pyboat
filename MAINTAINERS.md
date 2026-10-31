# Maintainer guide

Use the [root setup](README.md#start-here) before running checks.

## Verification

| Surface | Command |
| --- | --- |
| Rust diagnostics | `cargo clippy --workspace --all-targets --all-features` |
| Native Rust workspace | `cargo test --workspace` |
| Rust browser target | `cargo clippy -p browser --target wasm32-unknown-unknown --all-features` |
| TypeScript tooling tests | `npm test` |
| Browser and playground | `npm run build` |

WebTransport integration tests need loopback UDP access. Headless rendering tests
need a hardware Vulkan GPU; software adapters are rejected.

## Visual checks

A build cannot validate appearance. Use `npm run dev:playground` to inspect the
world and `npm run dev` with the server to check it during play.

For repeatable visual checks, capture an image outside the repository:

```sh
cargo run -p playground-capture -- --output /tmp/pyboat.png
```

The PNG's `.json` sidecar records world inputs, derived conditions, and image
metrics. Use `cargo run -p playground-capture -- --help` for world, camera, clock,
and diagnostic options and limits. A fresh capture has no accumulated cloud
transport unless `--cloud-motion` supplies it, so matching its clock alone does
not reproduce a long-running interactive cloud view. Capture requires a hardware
Vulkan GPU.

### Firefox presentation flicker

On Linux with NVIDIA, Firefox's hardware WebGPU presentation can show stale or
mixed frames. In this game, disabling
`dom.webgpu.allow-present-without-readback` and restarting Firefox removed the
flicker but reduced responsiveness. Mozilla tracks the DMA-BUF recycling
regression in [bug 2078553](https://bugzilla.mozilla.org/show_bug.cgi?id=2078553);
its [fix](https://github.com/mozilla-firefox/firefox/commit/99709e1e8633)
disables cached DMA-BUF texture hosts.

To try the game's experimental workaround, restore that preference to `true`,
restart Firefox, and add `freshSurface=1` to the game URL (`?freshSurface=1`, or
`&freshSurface=1` after existing parameters). It reconfigures the canvas before
each submitted frame, replacing Firefox's swapchain and its recycled surfaces
without copying pixels to the CPU. New surface allocation and compositor setup
may still cost time; compare flicker and steering responsiveness with the same
scene and window size. This path needs affected-hardware validation and stays
off by default. Remove it once the supported Firefox builds carry the upstream
fix and the same hardware comparison passes without it.

## Rendering performance

The scenario bench times the production terrain-cache, surface-bake,
environment-field, occluder-field, scene, and upscale passes for the views in
[bench-scenarios.json](playground/src/bench-scenarios.json), as shipped and with
normally skipped shader work forced. Each scenario's terrain cache and baked
surface fill until they settle before its first frame; forced runs refill the
active cache level and bake one frame budget of the read level every frame, as
a fast pan does. A cold-start column times shipped frames from empty caches
until they settle, as after a teleport.
Scenarios with a `renderScale` render the scene below the output size and report
cost per output pixel; scenarios with a `quality` use that graphics tier:

```sh
cargo run --release -p playground-capture -- --bench --output-dir /tmp/bench-before
cargo run --release -p playground-capture -- --bench --output-dir /tmp/bench-after --baseline /tmp/bench-before
```

Each run writes `report.json`, one PNG per scenario, and a contact sheet; a
baseline adds timing ratios, image differences, and difference images. On
thermally limited hardware, consecutive runs can differ by about 5% in timing,
so repeat runs or start comparisons from a similar thermal state. Use `--help`
for resolution, frame counts, and the frame budget.

The same run then follows the camera paths in
[bench-sequences.json](playground/src/bench-sequences.json), timed as shipped
from caches settled at each path's first view. They show the slow frames that
cache fills and bakes cause during motion, and the report keeps every frame's
pass times for finding them.

On phones and other browsers, open the playground with `?bench` (optionally
`&width=`, `&height=`, `&warmup=`, `&frames=`). It renders the same scenarios
with the production shaders and shows per-pass GPU times when the adapter
supports timestamp queries, otherwise submit-to-completion time.

## Source navigation

| Find | Start at |
| --- | --- |
| Physics, control, world geometry | [simulation](crates/simulation/src/lib.rs), [behavior tests](crates/simulation/tests) |
| Game policy, environment, calibration | [game](crates/game/src/lib.rs), [behavior tests](crates/game/tests) |
| Wire messages | [protocol](crates/protocol/src/lib.rs) |
| Server lifecycle and transport | [server](crates/server/src/lib.rs), [operation guide](crates/server/README.md) |
| Prediction and browser rendering | [browser](crates/browser/src/lib.rs), [WebGPU/WebTransport adapter](crates/browser/src/web.rs) |
| Procedural shader mechanisms | [shader components](crates/browser/src/shader/components) |
| Browser presentation | [web](web/src/main.ts) |
| Playground controls and diagnostics | [playground](playground/src/main.ts), [tooling tests](playground/tests) |
| Headless capture | [capture](tools/playground-capture/src/main.rs) |
| Rendering performance bench | [scenarios](playground/src/bench-scenarios.json), [sequences](playground/src/bench-sequences.json), [capture bench](tools/playground-capture/src/bench.rs), [browser bench](playground/src/bench.ts) |
