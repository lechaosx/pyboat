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
| `1` | Gradient controller with global-optimum helper |
| `2` | Manual sail/rudder direction control |
| `3` | Automatic sheet with direct rudder control |
| `4` | Arcade heading and yaw-rate control |
| `5` | Arcade control with automatic bow/stern selection |
| `6` | Global-optimum target controller |
| `7` | Raw gradient controller without the global helper |
| `8` | Arcade control with direct forward/turn intent |
| `Space` | Reverse modifier in original arcade mode |
| `P` | Toggle dynamic/coupled physics |
| `R` | Reset boat and current lap |
| `M` | Toggle guided-gradient/manual modes |
| `C` | Disable the sailor in the original modes |

Keyboard input depends on the selected controller:

- Gradient and global-optimum controllers: `W/S` request forward/reverse force
  and `A/D` request yaw.
- Manual: `WASD` points the rudder and the arrow keys point the sail relative to
  the boat. Each arrow marks the surface's forward direction, so a forward arrow
  centers the drawn surface aft. Unreachable aft directions clamp to a mechanical
  angle limit. Releasing all keys releases the corresponding surface.
- Auto-sheet: hold `W` for powered trim; `A/D` directly position the rudder.
- Heading arcade modes: `W/A/S/D` continuously request
  bow/port/stern/starboard relative to the boat. Mode 4 points the bow toward
  that direction. Mode 5 automatically selects the bow or stern and reverses
  the turn while backing.
- Move-intent arcade: `W/S` directly request forward/reverse power and `A/D`
  request turn, matching the guided-gradient input.
- Original arcade: hold `Space` while providing a WASD direction to back up
  without changing the requested heading. `Space` alone does not apply power.

With a gamepad, mode 4 always points the bow toward the left stick; hold the south
face button to reverse the stick command. Mode 5 selects forward motion for the
forward half and reversed steering in sternway for the rear half. Mode 8 converts
the stick to boat-relative forward/turn components; forward power crosses zero
at pure port/starboard input. Auto-sheet mode uses the left stick's vertical axis
for power and horizontal axis for rudder position. Manual mode uses the
left stick to point the rudder and the right stick to point the sail in world
space. A centered stick releases its surface.

## Physics models

The `dynamic` model in `src/simple_sailing_game/physics/dynamic.py` is the
original implementation. Wind and water loads rotate
massive sail and rudder actuators against bearing damping and bounded sailor
torques. It preserves the original controller and its control-quality landscape.

The `coupled` model in `src/simple_sailing_game/physics/coupled.py` gives the
hull, sail/boom, and rudder/tiller their selected masses. Pivots and rotary
limits connect the surfaces to the hull, fluid loads act on the corresponding
bodies, and actuator torques act equally and oppositely on the surface and hull.
This allows moving appendages to react back on the boat.

Every controller can use either plant. Both consume the physical torques produced
by the assisted and manual controllers.

## Controllers

The original optimizer remains in
`src/simple_sailing_game/controllers/assisted.py`. Its three selectable modes
either target the sampled global optimum, follow the local objective gradient,
or guide that local gradient past poor local optima using the sampled result.

The additional controller in `src/simple_sailing_game/controllers/arcade.py`
first trims the sail on the wind-selected side. Mode 4 always turns the bow
toward the keyboard or stick direction; its separate reverse button
selects a backwinded sail without changing that heading or supplying power on its
own. Mode 5 chooses forward or sternway from the requested half-plane and
reverses its yaw command in sternway. Mode 8 takes forward/turn intent directly
and blends sail trim continuously as forward power crosses zero. All three
select an achievable rudder moment, including compensation for the sail's yaw
moment. Auto-sheet mode uses its own power/rudder intent instead.

Manual control converts each requested physical surface direction into a bounded
angle target. The sailor applies a damped steering torque while a direction is
held and no torque when that surface's input is neutral.

The on-screen intent display follows those meanings: optimizer and mode 8 show
boat-relative forward/turn vectors, manual shows sail/rudder direction arrows,
auto-sheet shows its two surface-control axes, and modes 4/5 show the requested
world-space heading. In mode 5 the arrow remains an input display, so it does not
predict the inverted sternway turn.

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

The suite covers force equations, actuator behavior, the controllers, all three
plants, timestep convergence, course progression, and representative long-running
sailing maneuvers.
