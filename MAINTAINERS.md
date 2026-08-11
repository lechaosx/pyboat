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
reconciliation, damage and wreck lifecycle, and representative sailing
maneuvers.

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
documentation, and decision classification.

## Code layout

- `src/game` owns boat configuration, course logic, input, rendering, the
  authoritative multiplayer server, and the Pygame client loops.
- `src/sailing` provides the reusable boat model, controllers, simulation adapter,
  coefficient curves, and articulated physics.
- `src/plotting` contains speed-polar and coefficient-plot tools.
- `crates/simulation` is the runtime-independent Rust/Rapier simulation library.
- `crates/game` contains concrete Rust dinghy tuning, the match environment, and
  control composition shared by the server and browser.
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

The Python implementation remains the behavioral reference. Port one concrete
behavior at a time into `pyboat-simulation`, add a Rust parity test for that
behavior, and keep the crate independent of browser and native-server runtimes.
Keep concrete dinghy and match tuning in `pyboat-game` so authoritative and
predicted simulation consume the same values without making the simulation crate
game-specific.
The initial slices construct the hull and articulated hull/sail/rudder assembly,
restore and read authoritative motion, and advance it with equal-and-opposite
actuator torques and native Rapier bearing damping in a zero-gravity world at 60
Hz. Flat-plate aerodynamic load is sampled at the sail's moving center of effort
and applied there each tick. Streamlined underwater loads model attached flow
through stall and act at the articulated rudder's moving center of effort and the
hull-mounted keel position. Quadratic anisotropic hull resistance is sampled at
the bow and stern, which opposes translation and yaw using each point's local
water-relative velocity. Surface angle targets become critically damped,
torque-limited actuator commands; sheet targets suppress outward sail torque.
Arcade control predicts drive and yaw effects across candidate surface angles,
then selects auto-sheet or yaw-rate targets before producing actuator torques.
Candidate prediction includes hull angular velocity but remains quasi-static with
respect to each candidate surface. The controlled-advance API composes this policy
with one synchronous physics tick and reports the chosen targets and active yaw
rate. The shared simulation inserts multiple articulated boats into one Rapier
world, applies their loads before a single world step, and reports active hull
contacts with resolved impulse magnitudes. The retained local browser harness
advances the same shared simulation for one controlled boat and one passive
practice boat. The current browser entry point instead converts authoritative
snapshot motion to WebGPU vertices. TypeScript samples `W/S`, `A/D`, and the first
connected gamepad; Rust converts the gamepad's world-relative left-stick vector
into local motion intent without adding a deadzone. Rust owns the WebSocket,
latest-snapshot mailbox, compact-JSON codec, fixed-tick input sequencing, and the
small sailing view used by the TypeScript DOM HUD. Tests target physical
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

The Python server and client are the current behavioral reference. Their authority,
fixed-tick commands, prediction bubble, reconciliation, spatial interest, damage,
and wreck decisions are recorded in [`ARCHITECTURE.md`](ARCHITECTURE.md). Update
that record in the same change whenever these behaviors change.

Loopback integration tests open local WebSocket sockets. Environments that restrict
socket creation must grant loopback permission to run the complete suite.

The Rust server assigns identities, creates one articulated Rapier boat per
completed WebSocket connection, consumes sequenced arcade input, and publishes
authoritative state at 60 Hz. Start it on the default `127.0.0.1:8765` address
with:

```sh
cargo run -p pyboat-server
```

`pyboat-protocol` uses serde-tagged compact JSON and validates normalized motion
input on both encode and decode. The welcome and input shapes intentionally match
the Python protocol while migration is incomplete. WebSocket upgrades must finish
within five seconds. Identity and spawn allocation happen after a successful
upgrade, so failed handshakes do not enter the match or advance player numbering.
Client messages and individual WebSocket frames are limited to 1 KiB because
inputs are the only client message type. The server queues newer input sequences,
consumes at most one per physics tick, and acknowledges only consumed commands. A
player's pending queue retains the newest 15 commands; overflow drops the oldest
unapplied command rather than accumulating more than 250 ms of stale control.
Snapshot fan-out uses Tokio watch channels so a slow connection retains only the
newest state. Rapier's maintained broad phase supplies interest candidates before
exact radial checks. Boats within 8 m include intent for interaction prediction,
boats through 24 m include visual motion, and farther boats are omitted. The boats
array is a complete current interest set; `removedPlayerIds` remains empty in Rust
snapshots for Python protocol compatibility.
Rust server integration tests bind an ephemeral loopback port and exercise actual
WebSocket clients through handshake limits, input, motion, spatial interest, and
disconnect cleanup.
The browser crate also owns a native loopback test that sends its real sequenced
commands through `pyboat-server`, drops intermediate snapshots before acceptance,
and verifies acknowledgement and bounded prediction. Its Tokio, server, and
WebSocket dependencies are dev-only and do not enter the WebAssembly artifact.
The Rust/WASM browser connects to port `8765` on the page hostname, using `wss` for
an HTTPS page and `ws` otherwise; HTTPS deployment therefore requires TLS
termination in front of the server. It keeps only the latest pending snapshot
between frames, replaces its replicated interest set, and sends sequenced input at
60 Hz. The client rebuilds one nearby Rapier world from each authoritative
snapshot, replays up to 250 ms of unacknowledged local commands, and advances it
with newly emitted commands. It retains only those 15 replayable commands, so a
snapshot stall cannot grow prediction history without bound. Boats outside that
world extrapolate for at most 250 ms. A cubic 100 ms render correction preserves
pose and velocity across snapshots and physics-bubble transitions while physical
state changes immediately.

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
