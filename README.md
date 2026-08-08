# Simple Sailing Game

A compact 2D sailing simulation for learning how sailing forces, control systems,
and a small arcade game fit together. The game deliberately leaves out heel,
waves, and sail deformation so the force calculations remain readable.

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
| `1` | Direct arcade (default) |
| `2` | Auto-sheet + rudder |
| `3` | Yaw-rate arcade |

Direct and yaw-rate arcade use `W/S` for forward/reverse power and `A/D` for
turn. Auto-sheet uses `W` for powered trim and `A/D` for direct rudder position.

Keyboard commands are immediate and full-scale, including diagonals. This lets
keyboard players create intermediate inputs with deliberate key frequency and
duty cycle. The gamepad's left stick denotes a world-space direction; it is
converted into boat-relative forward/turn components without controller-side
input shaping. Auto-sheet instead maps its vertical axis to power and horizontal
axis directly to rudder position.

## Controllers

All three controllers are concrete functions in
`src/simple_sailing_game/controllers/arcade.py`:

- Direct arcade maps power to a fraction of currently achievable sail drive and
  nonzero turn to a fraction of currently achievable combined sail-and-rudder
  yaw moment. With the stick centered laterally, the sailor counteracts measured
  yaw using critical damping derived from boat inertia and current rudder moment
  authority. It does not latch or restore a heading.
- Auto-sheet chooses sail trim from the requested power while mapping steering
  directly to physical rudder angle. It exposes the clearest tiller-to-turn
  relationship.
- Yaw-rate arcade is the retained comparison controller. It maps turn input to
  the historical maximum yaw rate of 35°/s and chooses a rudder angle from the
  rate error. Unlike direct arcade, its turn command describes a motion target
  rather than a fraction of available yaw moment.

Normal trim treats the sail command as a sheet limit: the sailor can pull the
boom inward or hold it against the wind, but cannot push it outward. Explicit
reverse commands use a backed-sail target.

## Physics models

The game always uses the `articulated` model in
`src/simple_sailing_game/physics/coupled.py`. The hull, sail/boom, and
rudder/tiller have their selected masses. Pivots and rotary limits connect the
surfaces to the hull, fluid loads act on their bodies, and actuator torques act
equally and oppositely on each surface and the hull. Appendage motion therefore
reacts back on the boat while preserving coupled momentum.

The `lumped` model in `src/simple_sailing_game/physics/dynamic.py` places the
complete boat mass on one body and integrates sail and rudder angles separately.
It remains a compact educational reference covered by focused force, damping,
and timestep tests, but it is not a selectable game plant.

The sail and underwater foils have independently tunable profile-drag
coefficients. Keel force acts at `keel_position_m`, so moving that point changes
both local apparent flow and yaw moment. Hull resistance is sampled at the bow
and stern, which produces yaw damping from local lateral flow. The configured
hull lateral drag coefficient is `0.02`; the keel remains the main source of
lateral resistance.

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
- `physics/coupled.py` is the game plant; `physics/dynamic.py` is the lumped
  educational reference.

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
| 30° close-hauled | 0.6–1.4 m/s, under 12° leeway |
| 90° beam reach | 2–3 m/s |
| 120° broad reach | 2.2–3.2 m/s |
| Polar tracking | Under 8° heading error at sampled headings |
| Coast-down | Monotonic; 2 m/s falls below 1.5 m/s in 5 seconds |
| Tack | Both directions complete within 15 seconds with forward speed retained |
| Turn from rest | Direct control turns 135° within 28 seconds |
| Neutral steering | Released yaw rate falls by at least 75% within 3 seconds |

The simpler coefficient plot is also available:

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
