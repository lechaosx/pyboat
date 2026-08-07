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
| `1` | Manual sail/rudder direction control |
| `2` | Automatic sheet with direct rudder control |
| `3` | Improved arcade forward/turn control (default) |
| `4` | Original arcade forward/turn control |
| `5` | Original arcade heading and yaw-rate control |
| `6` | Original arcade control with automatic bow/stern selection |
| `7` | Gradient controller with global-optimum helper |
| `8` | Global-optimum target controller |
| `9` | Raw gradient controller without the global helper |
| `Space` | Reverse modifier in original arcade mode |
| `P` | Toggle lumped/articulated physics |
| `R` | Reset boat and current lap |
| `M` | Toggle guided-gradient/manual modes |
| `C` | Disable the sailor in the original modes |

Modes 1–3 are the main manual-to-arcade progression. Modes 4–9 remain
selectable as controller experiments so their behavior can be compared.

Keyboard input depends on the selected controller:

- Gradient and global-optimum controllers: `W/S` request forward/reverse force
  and `A/D` request yaw.
- Manual: `WASD` points the rudder and the arrow keys point the sail relative to
  the boat. Each arrow marks the surface's forward direction, so a forward arrow
  centers the drawn surface aft. Unreachable aft directions clamp to a mechanical
  angle limit. Releasing all keys releases the corresponding surface.
- Auto-sheet: hold `W` for powered trim; `A/D` directly position the rudder.
- Heading arcade modes: `W/A/S/D` continuously request
  bow/port/stern/starboard relative to the boat. Mode 5 points the bow toward
  that direction. Mode 6 automatically selects the bow or stern and reverses
  the turn while backing.
- Move-intent arcade modes: `W/S` directly request forward/reverse power and
  `A/D` request turn. Mode 3 eases the sheet during large turns and reduces its
  yaw-rate request at low speed; mode 4 preserves the original behavior.
- Original arcade: hold `Space` while providing a WASD direction to back up
  without changing the requested heading. `Space` alone does not apply power.

With a gamepad, mode 5 always points the bow toward the left stick; hold the south
face button to reverse the stick command. Mode 6 selects forward motion for the
forward half and reversed steering in sternway for the rear half. Modes 3 and 4
convert the stick to boat-relative forward/turn components. Auto-sheet mode uses
the left stick's vertical axis for power and horizontal axis for rudder position.
Manual mode uses the left stick to point the rudder and the right stick to point
the sail in world space. A centered stick releases its surface.

## Physics models

The `lumped` model, implemented in
`src/simple_sailing_game/physics/dynamic.py`, is the normal game and sailing
education plant. One body carries the complete boat mass while the sail and
rudder angles are integrated separately. It is direct, predictable, and easy to
follow.

The optional `articulated` model, implemented in
`src/simple_sailing_game/physics/coupled.py`, gives the hull, sail/boom, and
rudder/tiller their selected masses. Pivots and rotary limits connect the
surfaces to the hull, fluid loads act on their bodies, and actuator torques act
equally and oppositely on each surface and the hull. This makes appendage motion
react back on the boat.

Every controller can use either plant. Both consume the physical torques produced
by the assisted and manual controllers.

The sail and underwater foils have independently tunable profile-drag
coefficients. Keel force acts at `keel_position_m`, so moving that point fore or
aft produces a yaw moment. Hull resistance is primarily longitudinal; a smaller
lateral coefficient damps the hull while the keel remains the main source of
lateral resistance.

## Controllers

The original optimizer remains as a coding experiment in
`src/simple_sailing_game/controllers/assisted.py`. Its three selectable modes
either target the sampled global optimum, follow the local objective gradient,
or guide that local gradient past poor local optima using the sampled result.

The original controller in `src/simple_sailing_game/controllers/arcade.py`
first trims the sail on the wind-selected side. Mode 5 always turns the bow
toward the keyboard or stick direction; its separate reverse button
selects a backwinded sail without changing that heading or supplying power on its
own. Mode 6 chooses forward or sternway from the requested half-plane and
reverses its yaw command in sternway. Mode 4 takes forward/turn intent directly
and blends sail trim continuously as forward power crosses zero. All three
select an achievable rudder moment, including compensation for the sail's yaw
moment. Auto-sheet mode uses its own power/rudder intent instead.

The improved controller in
`src/simple_sailing_game/controllers/improved_arcade.py` wraps the original
forward/turn allocator. It reduces sail power during large turns and scales the
requested yaw rate with speed through water. The old controllers are unchanged
and remain selectable for comparison.

Manual control converts each requested physical surface direction into a bounded
angle target. The sailor applies a damped steering torque while a direction is
held and no torque when that surface's input is neutral.

The on-screen intent display follows those meanings: optimizer and modes 3/4 show
boat-relative forward/turn vectors, manual shows sail/rudder direction arrows,
auto-sheet shows its two surface-control axes, and modes 5/6 show the requested
world-space heading. In mode 6 the arrow remains an input display, so it does not
predict the inverted sternway turn.

Controller candidates are predicted quasi-statically. The prediction includes
hull angular velocity but not the candidate surface's own angular motion. The
articulated plant recalculates the actual load from each physical surface body's
velocity, so prediction and plant load can briefly differ during fast movement.

## Code layout

- `game.py` owns runtime parameters, controller tuning, mode selection, and the
  Pygame loop.
- `simulation.py` receives configuration explicitly and owns controller-to-plant
  adaptation and stepping.
- `input.py` converts keyboard and gamepad state to controller intent.
- `render.py` receives boat parameters explicitly and owns Pygame drawing.

## Model tuning

Generate a simulated speed polar for both plants:

```sh
uv run sailing-polar
```

This holds a series of headings with the arcade controller, lets each run settle,
and writes `polar.png`. The reported quantities are implemented in
`src/simple_sailing_game/polar.py`; adjust the durations or heading samples there
when iterating quickly.

The simpler coefficient plot remains available:

```sh
uv run sailing-plot
```

## Tests

Run the complete suite from the repository root:

```sh
uv run python -m unittest discover -v
```

The suite covers force equations, actuator behavior, the controllers, both
plants, timestep convergence, course progression, and representative long-running
sailing maneuvers.
