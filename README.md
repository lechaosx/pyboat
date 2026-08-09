# Simple Sailing Game

A compact 2D sailing simulation for learning how sailing forces, control systems,
and a small arcade game fit together. The game deliberately leaves out heel,
waves, and sail deformation so the force calculations remain readable.

## Design principles

- Teach both sailing and coding with physics that stays readable.
- Use one articulated plant. Pymunk resolves rigid-body motion, joints, surface
  inertia, and reaction forces while the project supplies the fluid loads and
  sailor controls.
- Keep keyboard motion boat-relative and joystick motion world-relative; each is
  intentionally adapted to what feels natural on that device.
- Prefer behavioral checks over calibrated speed ceilings. A physics correction
  that makes the boat faster should not fail only because it exceeds an old
  maximum.
- Treat graphics as supporting feedback rather than a simulation priority.
- Add physical detail only when its educational value justifies the code.

## Run it

Enter the development environment and start the game:

```sh
nix develop
uv run sailing-game
```

The boat starts on a four-buoy time-trial course. Pass the numbered buoys in
order; the active buoy is magenta. `R` resets the boat and current lap while
retaining the best completed lap.

## Controls

| Key | Controller |
| --- | --- |
| `1` | Computed yaw-rate arcade (default) |
| `2` | Auto-sheet + rudder |
| `3` | Fixed yaw-rate arcade |

Both yaw-rate modes use `W/S` for forward/reverse power and `A/D` for turn.
Auto-sheet uses `W` for powered trim and `A/D` for direct rudder position.

Keyboard commands are immediate and full-scale, including diagonals. This lets
keyboard players create intermediate inputs with deliberate key frequency and
duty cycle. The gamepad's left stick denotes a world-space direction; it is
converted into boat-relative forward/turn components without controller-side
input shaping. Auto-sheet instead maps its vertical axis to power and horizontal
axis directly to rudder position.

## Controllers

The three modes use two concrete controller functions in
`src/simple_sailing_game/controllers/arcade.py`:

- Computed yaw-rate arcade is the default. Its maximum yaw rate is the local
  water speed at the rudder multiplied by the tangent of maximum rudder angle
  and divided by the keel-to-rudder distance. Turn input selects a continuous
  fraction of that rate, including non-integer joystick values.
- Auto-sheet chooses sail trim from the requested power while mapping steering
  directly to physical rudder angle. It exposes the clearest tiller-to-turn
  relationship.
- Fixed yaw-rate arcade is the retained comparison controller. It uses the same
  policy with the historical maximum yaw rate of 35°/s.

Both yaw-rate modes choose a rudder angle from the difference between requested
and measured yaw rate. The HUD displays the measured and active maximum rates.

Normal trim treats the sail command as a sheet limit: the sailor can pull the
boom inward or hold it against the wind, but cannot push it outward. Explicit
reverse commands use a backed-sail target.

## Physics model

The game always uses the `articulated` model in
`src/simple_sailing_game/physics/coupled.py`. The hull, sail/boom, and
rudder/tiller have their selected masses. Pivots and rotary limits connect the
surfaces to the hull, fluid loads act on their bodies, and actuator torques act
equally and oppositely on each surface and the hull. Appendage motion therefore
reacts back on the boat while preserving coupled momentum.

The sail uses a symmetric flat-plate lift curve. The streamlined keel and rudder
instead build lift linearly to a 15° stall, then transition toward separated
flat-plate cross-flow. Sail and underwater foils also have independently tunable
profile-drag coefficients. `BoatParameters` receives separate sail, keel, and
rudder foil properties from the caller. Each foil selects its own area, lift
curve, drag curve, and base drag coefficient, so alternate boats and coefficient
laws require no simulation-code changes. Keel force acts at `keel_position_m`,
so moving that point changes both local apparent flow and yaw moment. Hull
resistance is sampled at the bow and stern, which produces yaw damping from local
lateral flow. The configured hull lateral drag coefficient is `0.02`; the keel
remains the main source of lateral resistance.

Callers compose coefficient functions before constructing the boat; the plant
has no implicit curve defaults:

```python
import math

from simple_sailing_game import boat
from simple_sailing_game.physics import curves

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

Controller candidates are predicted quasi-statically. The prediction includes
hull angular velocity but not the candidate surface's own angular motion. The
articulated plant recalculates actual load from each physical surface body's
velocity, so prediction and plant load can briefly differ during fast movement.

## Code layout

- `game.py` owns runtime parameters, mode selection, and the Pygame loop.
- `simulation.py` adapts the three controllers to the articulated plant.
- `controllers/arcade.py` owns intents and surface-target policies.
- `controllers/actuator.py` converts surface targets into bounded torques.
- `input.py` converts keyboard and gamepad state into controller intent.
- `render.py` owns Pygame drawing.
- `physics/curves.py` provides selectable and parameterized foil coefficient
  curves.
- `physics/coupled.py` constructs and advances the articulated boat.

## Model tuning

Generate a simulated speed polar for the articulated plant:

```sh
uv run --extra plots sailing-polar
```

The polar helper drives the retained yaw-rate controller with a local heading
error command, lets each run settle, and writes `polar.png`. It reports requested
and actual heading separately so controller error does not masquerade as a
physics change.

The physical acceptance tests use a 4 m, 250 kg displacement dinghy in 6 m/s
wind. Their deliberately broad reference envelope is:

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

The simpler coefficient plot compares the lift and drag curves selected for all
three foils:

```sh
uv run --extra plots sailing-plot
```

Pygame and Pymunk are the only core dependencies. Matplotlib is confined to the
optional `plots` dependency group.

## Tests

Run the complete suite from the repository root:

```sh
uv run --all-extras python -m unittest discover -v
```

The suite covers force equations, actuator behavior, all three controllers, both
physics models, timestep convergence, course progression, input mapping, and
representative sailing maneuvers.
