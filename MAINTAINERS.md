# Maintainer guide

This document is for developers working on the repository. Read
[`PRODUCT.md`](PRODUCT.md) for intended product behavior and
[`ARCHITECTURE.md`](ARCHITECTURE.md) before changing gameplay or architecture.

## Development environment

Python 3.10 or newer is required. Set up the project using the Python environment
and package manager of your choice. Repository commands use `uv`; prepare its
environment with:

```sh
uv sync
```

The optional Nix development environment provides `uv` and the other development
tools:

```sh
nix develop
```

Pygame, Pymunk, and websockets are core dependencies. Matplotlib and pytest are
development dependencies.

Run the complete suite:

```sh
uv run pytest
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
- `tests` contains unit, invariant, maneuver, rendering, protocol, and loopback
  multiplayer integration tests.

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
