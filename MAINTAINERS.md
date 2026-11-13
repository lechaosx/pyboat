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

Compare controller motion under paired initial states and prescribed inputs:

```sh
cargo run --release -p browser --features playground --example controller-intent-assessment -- /tmp/controller-assessment
```

Use `--help` for output definitions and training/held-out splits. Motion outcomes
and runtime are separate measurements; the tool assigns no overall winner.

For the standalone neural pilot, run `cargo run --release -p browser --features
playground --example train-standalone-controller -- --help` for the reproducible
training command, objective, splits, and checkpoint selection.

## Added-inertia investigation

The November 2026 investigation compared constraint-based added inertia,
full directional tensors on ordinary and articulated bodies, a hinge-matched
yaw shortcut, and intrinsic centroid-spin inertia. The reference was
thin-elliptical-foil potential flow, not real-boat measurements. Full tensors
required engine changes and more global stepping; the hinge shortcut could
over-resist motion whose foil-centre normal velocity cancelled. Intrinsic spin
improved some responses and worsened others against that approximate reference;
collision impulses stayed near the model without added inertia and peak rudder
speed barely changed.

Matched tests based on `3e59d33` used 64 boats, eight internal partitions,
unchanged controller gains, and native/Node 24.21 WebAssembly on an i5-13600KF.
With one pair colliding at 40 m/s each, ordinary full tensors averaged 39.95
versus 4.60 ms per WASM tick without added inertia. In a separate procedural
workload, intrinsic spin averaged 5.59 versus 5.86 ms, accompanied by fewer
adaptive updates. These comparisons include trajectory changes and exclude
rendering, networking, and spawning; they do not isolate matrix arithmetic.
The tensor timing prototypes omitted ambient-flow acceleration.
Timestep refinement and physical-model disagreement were measured separately;
real-boat calibration remains unverified.
The accepted compromise is in [ARCHITECTURE.md](ARCHITECTURE.md#authority-and-coordinates);
its coefficient and source are beside `foil_spin_inertia_kg_m2`.

## Visual checks

A build cannot validate appearance. Run `npm run dev` and open `/playground/`
to inspect the world, or `/` with the server to check it during play.

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

For the game's opt-in workaround, keep that preference at `true`, restart
Firefox, and add `freshSurface=1` to the game URL (`?freshSurface=1`, or
`&freshSurface=1` after existing parameters). It removed the flicker on the
tested Linux/NVIDIA setup. It reconfigures the canvas before each submitted
frame, replacing recycled surfaces without copying pixels to the CPU. Surface
allocation and compositor setup may still cost time. Once supported Firefox
builds carry the upstream fix, compare flicker and steering responsiveness with
and without the parameter on the same hardware, scene, and window size. Remove
the workaround once that comparison passes without it.

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
