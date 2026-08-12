# Maintainer guide

This document is for developers working on the repository. Read
[`PRODUCT.md`](PRODUCT.md) for intended product behavior and
[`ARCHITECTURE.md`](ARCHITECTURE.md) before changing gameplay or architecture.

## Development environment

Python 3.10 or newer and Rust 1.87 or newer are required. The Rust toolchain must
include the `wasm32-unknown-unknown` target. Browser development also requires
`wasm-pack` and Bun. Set up the Python project using the environment and package
manager of your choice. Repository commands use `uv`; prepare its environment
with:

```sh
uv sync
```

The optional Nix development environment provides the Python and Rust toolchains,
including the WebAssembly target:

```sh
nix develop
```

Pygame, Pymunk, and websockets are core dependencies. Matplotlib and pytest are
development dependencies.

Prepare the browser workspace with:

```sh
bun install
```

Run the complete suite:

```sh
uv run pytest
cargo test --workspace
cargo check --workspace --exclude pyboat-server --target wasm32-unknown-unknown
bun run build
```

The suite covers force equations, actuator behavior, all three controllers,
coupled physics, timestep convergence, course progression, input mapping,
multiplayer protocol behavior, interest management, prediction and
reconciliation, deterministic terrain generation, damage and wreck lifecycle,
and representative sailing maneuvers.

Exercise a multiplayer client with controlled round-trip latency and one-way
jitter:

```sh
uv run client --latency-ms 100 --jitter-ms 10
```

## Engineering principles

- Teach both sailing and coding with physics that stays readable.
- Use one articulated plant. Pymunk resolves rigid-body motion, joints, surface
  inertia, and reaction forces while the project supplies fluid loads and sailor
  controls.
- Keep keyboard motion boat-relative and joystick motion world-relative; each is
  adapted to what feels natural on that device.
- Prefer behavioral checks over calibrated speed ceilings. A physics correction
  that makes the boat faster should not fail only because it exceeds an old
  maximum.
- Treat graphics as supporting feedback rather than a simulation priority.
- Add physical detail only when its educational value justifies the code.

The repository-level [`AGENTS.md`](AGENTS.md) adds rules for abstraction,
documentation, decision classification, and choosing tests by observable risk.

## Code layout

- `src/game` owns boat configuration, course logic, input, rendering, the
  authoritative multiplayer server, and the Pygame client loops.
- `src/sailing` provides the reusable boat model, controllers, simulation adapter,
  coefficient curves, and articulated physics.
- `src/plotting` contains speed-polar and coefficient-plot tools.
- `crates/simulation` is the runtime-independent Rust/Rapier simulation library.
- `crates/game` contains concrete Rust dinghy tuning, deterministic procedural
  environment sampling, and control composition shared by the server and browser.
- `crates/protocol` contains typed compact-JSON messages shared by Rust runtimes.
- `crates/server` contains the native async WebSocket server executable and
  connection lifecycle.
- `crates/browser` owns Rust/WASM networking, replicated browser state,
  world-to-scene conversion, and the WebGPU renderer. Its retained local demo is a
  native simulation test harness.
- `web` contains the Vite page shell and TypeScript frame scheduling.
- `tests` contains unit, invariant, maneuver, rendering, protocol, and loopback
  multiplayer integration tests.

## Rust migration

The Python implementation remains a historical source for accepted behavior, not
a compatibility target. Port one concrete behavior at a time into
`pyboat-simulation`, test its physical invariants and gameplay behavior in Rust,
and keep the crate independent of browser and native-server runtimes.
Keep concrete dinghy and match tuning in `pyboat-game` so authoritative and
predicted simulation consume the same values without making the simulation crate
game-specific. Its procedural environment sampler is a pure function of match
seed, simulation tick, and world position so native and WebAssembly runtimes can
reproduce the same wind. Keep procedural tuning and sampling in this shared crate;
the protocol transfers the seed rather than sampled fields or parameters.
Static terrain routes several seeded density fields in uncompressed real metres.
Three cluster octaves over 2,048–8,192 m and five island-detail octaves over
64–1,024 m shape sparse large islands without a continent-scale field that could
close regional seas. Three 2,048–8,192 m geology octaves form one coherent
modulation field. Its 0.31–0.56 upper-tail spline scales signed base elevation by
1–50× while preserving the zero contour. Representative samples must include
beachless, mixed, and beach-dominant 2 km coastal regions while beaches remain
the global majority, most sampled cliffs remain steep immediately offshore, and
depths beyond 100 m occur without exceeding 5% of water samples. Four 4–32 m reef
octaves feed two smooth masks. Between -4 and -0.1 m elevation, up to 0.6 m of
signed relief cuts channels and raises
the submerged seabed. Between -2 and -0.1 m, a 0.65–0.85 upper-tail spline adds
up to 2 m for sparse emergent rocks. This creates a distribution of channel
depths without placing objects, checking clearance around individual features,
or applying reef-scale noise to the parent island. Keep land at 5–15% across
representative regions and each sampled 65 km region below 20%. The shared
sampler returns continuous elevation relative to sea level from only the match
seed and world position. Each procedural environment converts elevation to water
depth for authoritative and predicted grounding without adding terrain to the
protocol. The browser also exposes that predicted depth, depth minus dinghy draft,
and bottom support fraction to the navigation HUD.
Coastal relief is calibrated against [NOAA BlueTopo](https://www.nauticalcharts.noaa.gov/data/bluetopo.html)
4 m tiles `BH5475HC` (rocky Maine, 2026-06-01) and `BH4TB55V` (Florida Keys,
2026-05-20), using the same -4 to 1 m elevation band in data and generator.
At 32 m and 128 m baselines, keep generated 90th-percentile relief below the
observed `1.94 m` and `2.92 m` rocky-coast upper bounds. Treat those data as
roughness limits, not shoreline-contour targets. The source grids are public
navigation bathymetry; calibration data stays outside the repository.
Unsheltered wind is the sum of 48 divergence-free Fourier modes. Weather modes
carry 90% of component variance over 32,768–131,072 m wavelengths, regional modes
carry 8% over 4,096–32,768 m, and gust modes carry 2% over 64–512 m. This gives an
island-scale view a prevailing direction while distant regions remain unrelated.
Seeded Box–Muller coefficients create isotropic Gaussian vector components, giving
speed the configured `k = 2`, `λ = 9 m/s` Weibull marginal. `libm` supplies the
shared software transcendental functions used by native and WebAssembly sampling.
One seed-derived transport vector moves the entire frozen field, so spatial
structures remain coherent instead of individual modes propagating in unrelated
directions. Cloud density evaluates six octaves of smooth value noise over
4,096–128 m in the same transported coordinates. Map its signal through a
-0.15–0.45 smoothstep; representative samples must contain both broad clear and
cloudy regions, with density above 0.85 present but below 10%. Final surface wind
samples terrain at six fixed points from 16–512 m upwind. Elevation through 1.5 m
scales obstruction, a 256 m rational distance falloff models recovery, and the
strongest obstruction reduces speed by at most 70% without changing direction.
Keep this calculation in `pyboat-game`; server and browser must apply identical
shelter from the shared match seed rather than
transferring wind samples.
Regenerate the checked-in shelter visualization from the shared sampler with:

```sh
cargo run -p pyboat-game --example wind_shelter --release -- docs/wind-shelter.svg
```

The initial slices construct the hull and articulated hull/sail/rudder assembly,
restore and read authoritative motion, and advance it with equal-and-opposite
actuator torques and native Rapier bearing damping in a zero-gravity world at 60
Hz. Flat-plate aerodynamic load is sampled at the sail's moving center of effort
and applied there each tick. Streamlined underwater loads model attached flow
through stall and act at the articulated rudder's moving center of effort and the
hull-mounted keel position. Quadratic anisotropic hull resistance is sampled at
the bow and stern, which opposes translation and yaw using each point's local
water-relative velocity. When sampled depth falls below the boat's draft, the
seabed supports a proportional fraction of the boat's weight. A static-friction
wrench fixes the hull while combined external force and yaw demand remain inside
its breakaway envelope. After breakaway, capped kinetic friction acts at the bow
and stern without numerically overshooting rest. The dinghy's 0.6 m draft uses
effective static and kinetic coefficients of 1.0 and 0.6. Surface angle targets
become critically damped, torque-limited actuator commands; sheet targets
suppress outward sail torque.
Arcade control predicts drive and yaw effects across candidate surface angles,
then selects auto-sheet or yaw-rate targets before producing actuator torques.
Candidate prediction includes hull angular velocity but remains quasi-static with
respect to each candidate surface. The controlled-advance API composes this policy
with one synchronous physics tick and reports the chosen targets and active yaw
rate. The shared simulation inserts multiple articulated boats into one Rapier
world, accepts a separate environment for each boat, applies their loads before a
single world step, and reports active hull contacts with resolved impulse
magnitudes and hull-local contact points. The Rust server uses those contacts for
the reference ramming-damage tuning and leaves
destroyed boats in the world with passive forces and zero actuator torque. The
retained local browser harness advances the same shared simulation for one
controlled boat and one passive practice boat. The current browser entry point
instead converts authoritative snapshot motion to WebGPU vertices. TypeScript
samples `W/S`, `A/D`, and the first connected gamepad; Rust converts the gamepad's
world-relative left-stick vector into local motion intent without adding a
deadzone. Rust owns the WebSocket, latest-snapshot mailbox, compact-JSON codec,
fixed-tick input sequencing, and the small sailing view used by the TypeScript DOM
HUD. Tests target physical
invariants, replicated-state behavior, mirrored controlled maneuvers, and accepted
gameplay behavior rather than exact Pymunk trajectories.

Runtime adapters should use asynchronous I/O wherever work can suspend. Keep the
fixed-step simulation synchronous: callers gather available inputs before a tick
and publish results after it rather than awaiting work during simulation.

Rapier uses `enhanced-determinism` without SIMD or parallel features. Native tests
exercise behavior; the WebAssembly check verifies that the same library remains
portable to the browser target.

## Controllers

The three modes use two concrete controller functions in
`src/sailing/controllers/arcade.py`:

- Computed yaw-rate arcade is the default. Its maximum yaw rate is the local water
  speed at the rudder multiplied by the tangent of maximum rudder angle and divided
  by the keel-to-rudder distance. Turn input selects a continuous fraction of that
  rate, including non-integer joystick values.
- Auto-sheet chooses sail trim from requested power while mapping steering directly
  to physical rudder angle. It exposes the clearest tiller-to-turn relationship.
- Fixed yaw-rate arcade is retained for comparison. It uses the same policy with
  the historical maximum yaw rate of 35°/s.

Both yaw-rate modes choose a rudder angle from the difference between requested and
measured yaw rate. The standalone HUD displays measured and active maximum rates.

Normal trim treats the sail command as a sheet limit: the sailor can pull the boom
inward or hold it against the wind, but cannot push it outward. Explicit reverse
commands use a backed-sail target.

## Physics model

The game uses the articulated model in `src/sailing/physics/coupled.py`. The hull,
sail/boom, and rudder/tiller have selected masses. Pivots and rotary limits connect
the surfaces to the hull, fluid loads act on their bodies, and actuator torques act
equally and oppositely on each surface and the hull. Appendage motion therefore
reacts back on the boat while preserving coupled momentum.

The sail uses a symmetric flat-plate lift curve. The streamlined keel and rudder
build lift linearly to a 15° stall, then transition toward separated flat-plate
cross-flow. Sail and underwater foils have independently tunable profile-drag
coefficients. `BoatParameters` receives separate sail, keel, and rudder foil
properties from the caller. Each foil selects its area, lift curve, drag curve, and
base drag coefficient. Keel force acts at `keel_position_m`, so moving that point
changes both local apparent flow and yaw moment. Hull resistance is sampled at the
bow and stern, producing yaw damping from local lateral flow. The configured hull
lateral drag coefficient is `0.02`; the keel remains the main source of lateral
resistance.

Callers compose coefficient functions before constructing the boat; the plant has
no implicit curve defaults:

```python
import math

from sailing import boat
from sailing.physics import curves

base_underwater_lift = curves.streamlined_lift(math.radians(12), 0.9)

def underwater_lift(angle_of_attack_rad):
    return 0.95 * base_underwater_lift(angle_of_attack_rad)

keel_foil = boat.FoilProperties(
    area_m2=0.6,
    lift_coefficient=underwater_lift,
    drag_coefficient=curves.flat_plate_drag,
    base_drag_coefficient=0.02,
)
```

Controller candidates are predicted quasi-statically. Prediction includes hull
angular velocity but not the candidate surface's own angular motion. The
articulated plant recalculates actual load from each physical surface body's
velocity, so prediction and plant load can briefly differ during fast movement.

## Multiplayer implementation

The Python server and client remain a historical behavioral reference. Their
authority, fixed-tick commands, prediction bubble, reconciliation, spatial
interest, damage, and wreck decisions are recorded in
[`ARCHITECTURE.md`](ARCHITECTURE.md). Rust/Python interoperability and exact
Pymunk/Rapier parity are not requirements. Update the record in the same change
whenever these behaviors change.

Loopback integration tests open local WebSocket sockets. Environments that restrict
socket creation must grant loopback permission to run the complete suite.

The Rust server assigns identities, creates one articulated Rapier boat per
completed WebSocket connection, consumes sequenced arcade input, and publishes
authoritative state at 60 Hz. The first deterministic position with at least
1.5 m of water becomes the match's spawn origin. Later boats search seed-derived
bearings at least 8 m beyond the fleet radius around that origin and point along
the procedural wind sampled at the chosen position and current tick. Start the
server on the default `127.0.0.1:8765` address with:

```sh
cargo run -p pyboat-server
```

Ctrl-C signals the async serving loop to send WebSocket close frames with code
1001 and the reason `Server shutting down`, allow up to one second for established
connections to flush them, and then release the listener. Connection handlers
exit after writing the frame rather than waiting for a peer close response.
Integration tests call `serve_until` with a one-shot future for the same controlled
shutdown path; `serve` remains the indefinite entry point.

`pyboat-protocol` uses serde-tagged compact JSON and validates normalized motion
input on both encode and decode. It is the Rust stack's wire contract and need not
remain compatible with Python. Rust welcome messages include `matchSeed`, which
the retained Python server does not send. WebSocket upgrades must finish within
five seconds. Identity and spawn allocation happen after a successful upgrade, so
failed handshakes do not enter the match or advance player numbering.
The connection task waits for central player and shutdown-mailbox registration
before sending `welcome`, preserving welcome-before-snapshot ordering and making
the welcomed session visible to controlled shutdown.
Client messages and individual WebSocket frames are limited to 1 KiB; the current
input and viewport messages are both compact. The server queues newer input
sequences, consumes at most one per physics tick, and acknowledges only consumed
commands. A player's pending queue retains the newest 15 commands; overflow drops
the oldest unapplied command rather than accumulating more than 250 ms of stale
control.
Snapshot fan-out uses Tokio watch channels so a slow connection retains only the
newest state. Rapier's maintained broad phase supplies interest candidates before
exact radial checks. Boats within 8 m include intent for interaction prediction.
Beyond that, boats through the client's requested viewport radius include visual
motion, with server-side bounds of 8–24 m and a 24 m default for clients without
viewport updates. The boats array is a complete current interest set;
`removedPlayerIds` remains empty in Rust snapshots.
Rust server integration tests bind an ephemeral loopback port and exercise actual
WebSocket clients through handshake limits, input, motion, spatial interest, and
disconnect and server-shutdown cleanup. One shutdown test owns the server in an
isolated Tokio runtime and drops that runtime when the serving future returns to
verify the close frame was flushed first.
The browser crate also owns a native loopback test that sends its real sequenced
commands through `pyboat-server`, drops intermediate snapshots before acceptance,
and verifies acknowledgement and bounded prediction. Its Tokio, server, and
WebSocket dependencies are dev-only and do not enter the WebAssembly artifact.
The Rust/WASM browser connects to port `8765` on the page hostname, using `wss` for
an HTTPS page and `ws` otherwise; HTTPS deployment therefore requires TLS
termination in front of the server. It keeps only the latest pending snapshot
between frames, replaces its replicated interest set, sends its viewport radius
after welcome and canvas aspect-ratio changes, and sends sequenced input at 60 Hz.
Welcome also supplies the server-selected match seed. Authoritative and predicted
ticks sample the same smooth wind function separately at each boat, and the local
sample supplies the apparent-wind HUD.
The local boat scene subtracts predicted boat velocity from that seeded true wind
and renders the result as one windsock attached to its bow. Five equal bands map
linearly to 0–15 knots of apparent wind. The renderer emits only complete bands
before the current extension and one partial final band; remote boats have no wind
indicator.
The browser draws one fullscreen terrain quad. Its fragment shader reconstructs
world position and mirrors the Rust cluster, island-detail, and reef density
fields for every pixel, including 64-bit seed mixing represented as
pairs of WebGPU-compatible 32-bit integers. Keep WGSL terrain constants, spline
edges, masks, hashing, and peak transforms synchronized with `pyboat-game`; Rust
remains authoritative for physical depth. The shader owns underwater color and
the shoreline and beach materials. Each value-noise octave returns its analytic
world-space gradient alongside its value; the shader propagates gradients through
the same amplitude normalization and density composition. Gradients at or above
`0.08 m/m` render as bare rock; below that cutoff, shoreline renders as beach and
inland terrain renders green.
The scene uniform also carries predicted simulation time. WGSL reconstructs the
same weather transport and cloud value noise as `pyboat-game`; keep octave
wavelengths, salt, transport, and density mapping synchronized. Cloud density
shades both the fullscreen terrain and later boat fragments. Low density is
translucent light grey, while the same continuous density darkens and conceals the
scene toward storm grey at its upper tail.
Middle mouse resets the original 15 m view height. Terrain can remain visible
outside the server's clamped boat-interest radius, where snapshots contain no
boats. The lower-left DOM scale bar selects a 1–2–5 distance near 120 CSS pixels
from the current vertical view height and canvas client height; keep its metric
label derived from the same distance.
The radius covers the visible half-diagonal plus a 2.5 m geometry margin. On a
connection error or close, the browser stops emitting network input, marks remote
boats as wrecks, and continues fixed-tick local prediction; the connection status
shows a clean close reason when one is available. The client rebuilds one nearby
Rapier world from each authoritative snapshot, replays up to 250 ms of
unacknowledged local commands, and advances it with newly emitted commands. It
retains only those 15 replayable commands, so a snapshot stall cannot grow
prediction history without bound. Boats outside that world extrapolate for at
most 250 ms. A cubic 100 ms render correction preserves pose and velocity across
snapshots and physics-bubble transitions while physical state changes immediately.
Health and recent-hit state drive the browser HUD and hull feedback. Wrecks use
passive nearby prediction. A destroyed owner remains connected, emits no more
input, and continues receiving snapshots centered on the wreck.

## Model tuning

Generate a simulated speed polar for the articulated plant:

```sh
uv run polar
```

The helper drives the retained yaw-rate controller with a local heading-error
command, lets each run settle, and writes `polar.png`. It reports requested and
actual heading separately so controller error does not masquerade as a physics
change.

The physical acceptance tests use a 4 m, 250 kg displacement dinghy in 6 m/s wind.
Their deliberately broad reference envelope is:

| Behavior | Accepted range |
| --- | --- |
| In irons | A 15° attempt stays below 0.2 m/s upwind progress |
| 30° close-hauled | At least 0.6 m/s, under 12° leeway |
| 90° beam reach | At least 2 m/s |
| 120° broad reach | At least 2.2 m/s |
| 180° run | At least 0.5 m/s forward |
| Polar tracking | Under 8° heading error at sampled headings |
| Coast-down | Monotonic; 2 m/s falls below 1.5 m/s in 5 seconds |
| Tack | Both directions complete within 15 seconds with forward speed retained |
| Turn from rest | Computed yaw-rate control turns 135° within 28 seconds |
| Neutral steering | Released yaw rate decreases over 3 seconds |

Compare the lift and drag curves selected for all three foils:

```sh
uv run coefficients
```

## Documentation maintenance

- Keep `README.md` user-facing: product summary, current gameplay, controls, setup,
  and run instructions.
- Keep this file developer-facing: repository layout, development workflow,
  implementation guidance, tests, and tooling.
- Record product decisions in `PRODUCT.md` and technical decisions in
  `ARCHITECTURE.md`, with the classification required by `AGENTS.md`.
- Update commands and paths when their implementation changes; do not leave stale
  compatibility instructions behind.
