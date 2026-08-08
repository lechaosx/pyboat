# Simple Sailing Game

A compact 2D sailing simulation for learning how sailing forces, control systems,
and a small arcade game fit together. The game deliberately keeps heel, waves,
and sail deformation out of the model so the force calculations remain readable.

## Run it

Enter the development environment and start the game:

```sh
nix develop
uv run sailing-game
```

The boat starts on a four-buoy time-trial course. Pass the numbered buoys in
order; the active buoy is magenta. Resetting starts a new attempt while retaining
the best completed lap.

## Controls

| Key | Action |
| --- | --- |
| `1` | Manual retained sheet/direct rudder control |
| `2` | Automatic sheet with direct rudder control |
| `3` | Direct force/moment arcade control (default) |
| `4` | Original yaw-rate arcade forward/turn control |
| `5` | Original arcade heading and yaw-rate control |
| `Space` | Reverse modifier in original arcade mode |
| `R` | Reset boat and current lap |

Modes 1–3 are the main manual-to-arcade progression. Modes 4–5 remain selectable
so their historical yaw-rate policies can be compared with direct control.

Keyboard input depends on the selected controller:

- Manual: hold `W/S` or up/down to sheet in or ease out. The sheet limit changes
  incrementally and stays where it is when released. `A/D` or left/right directly
  position the rudder and center it on release.
- Auto-sheet: hold `W` for powered trim; `A/D` directly position the rudder.
- Heading arcade mode: `W/A/S/D` continuously requests
  bow/port/stern/starboard relative to the boat. Mode 5 points the bow toward
  that direction.
- Move-intent arcade modes: `W/S` directly request forward/reverse power and
  `A/D` request turn. Mode 3 maps these values directly to fractions of the
  currently achievable drive force and yaw moment. Mode 4 preserves the
  original fixed-yaw-rate behavior.
- Original arcade: hold `Space` while providing a WASD direction to back up
  without changing the requested heading. `Space` alone does not apply power.

Keyboard diagonals retain full power and turn so players can use deliberate key
duty cycles for fine control. With a gamepad, mode 5 always points the bow toward
the left stick; hold the south face button to reverse the stick command. Modes 3
and 4 convert the stick to boat-relative forward/turn components without further
input shaping. Auto-sheet mode uses
the left stick's vertical axis for power and horizontal axis for rudder position.
Manual mode uses the left stick vertically to adjust the retained sheet limit and
horizontally to position the rudder.

## Physics models

The default `articulated` model, implemented in
`src/simple_sailing_game/physics/coupled.py`, gives the hull, sail/boom, and
rudder/tiller their selected masses. Pivots and rotary limits connect the
surfaces to the hull, fluid loads act on their bodies, and actuator torques act
equally and oppositely on each surface and the hull. This makes appendage motion
react back on the boat while preserving physically plausible coupled motion.

The `lumped` reference model, implemented in
`src/simple_sailing_game/physics/dynamic.py`, places the complete boat mass on
one body while integrating sail and rudder angles separately. It remains as a
compact coding reference and is covered by focused force, damping, and timestep
tests, but it is not a selectable game plant. This avoids multiplying every
controller and maneuver across two nearly identical steady-state models.

Normal trim treats the sail command as a sheet limit: the sailor can pull the
boom inward or hold it against the wind, but cannot push it outward. Explicit
reverse commands remain backed-sail targets rather than normal sheet trim.

The sail and underwater foils have independently tunable profile-drag
coefficients. Keel force acts at `keel_position_m`, so moving that point fore or
aft changes both its local apparent flow and its yaw moment. Hull resistance is
sampled at the bow and stern: streamlined longitudinal drag is small, while
order-one lateral pressure drag supplies natural yaw damping. The keel remains
the main source of lateral resistance.

## Controllers

The original optimizer remains as a standalone coding lab in
`src/simple_sailing_game/controllers/assisted.py`. Its tests demonstrate modes
that target the sampled global optimum, follow the local objective gradient, or
guide that gradient past poor local optima. It is deliberately absent from the
playable modes, telemetry, and renderer.

The direct controller in `src/simple_sailing_game/controllers/arcade.py` maps
mode 3 power to a fraction of the achievable drive force and turn to a fraction
of the achievable combined sail-and-rudder yaw moment. Nonzero turn input remains
fully direct. When steering is released, the sailor holds the release heading and
countersteers heading error and measured yaw. The correction uses critical
damping derived from the boat's inertia, current moment authority, and rudder
range. There is no chosen target yaw rate, response time, speed shaping, or
turn-dependent power reduction.

The same module retains the original controllers for comparison. Mode 5 turns
the bow toward the keyboard or stick direction; its separate reverse button
selects a backwinded sail without changing that heading or supplying power on its
own. Mode 4 takes forward/turn intent directly, blends sail trim continuously as
forward power crosses zero, and targets a fixed yaw rate. Both compensate for
the sail's yaw moment when selecting a rudder angle. Auto-sheet mode uses its own
power/rudder intent instead.

Manual control changes the retained sheet limit at one full configured range per
second and maps rudder input directly onto its physical angle range. The same
one-sided sheet and actuator-force limits used by the automatic modes govern the
actual surface motion.

The on-screen intent display follows those meanings: modes 3/4 show boat-relative
forward/turn vectors, manual shows sheet/rudder axes, auto-sheet shows its two
surface-control axes, and mode 5 shows the requested world-space heading.

Controller candidates are predicted quasi-statically. The prediction includes
hull angular velocity but not the candidate surface's own angular motion. The
articulated plant recalculates the actual load from each physical surface body's
velocity, so prediction and plant load can briefly differ during fast movement.
Neutral direct control compensates the sail's measured current yaw moment rather
than its target-angle prediction, including while the boom crosses downwind.

## Code layout

- `game.py` owns runtime parameters, mode selection, and the Pygame loop.
- `simulation.py` receives configuration explicitly and owns controller-to-plant
  adaptation for the articulated plant.
- `input.py` converts keyboard and gamepad state to controller intent.
- `render.py` receives boat parameters explicitly and owns Pygame drawing.
- `physics/dynamic.py` and `controllers/assisted.py` are compact reference/lab
  implementations rather than runtime branches.

## Model tuning

Generate a simulated speed polar for the articulated game plant:

```sh
uv run --extra plots sailing-polar
```

This holds a series of headings with the arcade controller, lets each run settle,
and writes `polar.png`. The reported quantities are implemented in
`src/simple_sailing_game/polar.py`, including actual heading and heading error;
adjust the durations or heading samples there when iterating quickly.

The physical acceptance tests treat the configured boat as a 4 m, 250 kg
displacement dinghy in 6 m/s wind. Their deliberately broad reference envelope
is:

| Behavior | Accepted range |
| --- | --- |
| In irons | Below 25°; a 15° attempt makes no sustained upwind progress |
| 30° close-hauled | 0.6–1.4 m/s, under 12° leeway |
| 90° beam reach | 2–3 m/s |
| 120° broad reach | 2.2–3.2 m/s |
| Heading hold | Under 8° error at sampled headings |
| Coast-down | Monotonic; 2 m/s falls below 1.5 m/s in 5 seconds |
| Tack | Both directions complete within 15 seconds with forward speed retained |
| Released turn | Yaw rate falls by at least 75% within 3 seconds, with under 5° further heading drift |
| Downwind stability | After turning onto downwind and releasing steering, settled heading variation and release-heading error stay below the 1° control sampling resolution |

The simpler coefficient plot remains available:

```sh
uv run --extra plots sailing-plot
```

Pygame and Pymunk are the only core dependencies. NumPy belongs to the optimizer
extra and Matplotlib to the plots extra. To run only the optimizer lab tests:

```sh
uv run --extra optimizer python -m unittest tests.test_control_model -v
```

## Tests

Run the complete suite from the repository root:

```sh
uv run --all-extras python -m unittest discover -v
```

The suite covers force equations, actuator behavior, the controllers, both
plants, timestep convergence, course progression, and representative long-running
sailing maneuvers.
