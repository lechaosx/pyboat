# Pyboat

Pyboat is an early multiplayer, top-down naval combat game built around readable,
semi-realistic sailing physics. The current Python prototype lets several players
sail, ram one another, take location-dependent damage, and leave physical wrecks
behind when destroyed.

The longer-term product aims for the accessibility of Agar.io and the satisfying
naval combat of *Assassin's Creed IV: Black Flag*, with persistent matches,
automatic weapons, varied ships, progression, and a procedural world. Those
features are not all implemented yet.

## Project guide

- [Product decisions](PRODUCT.md) describe the intended game and player
  experience.
- [Architecture decisions](ARCHITECTURE.md) describe the chosen technical design.
- [Maintainer guide](MAINTAINERS.md) covers development, tests, physics, and the
  repository layout.
- [Engineering guidelines](AGENTS.md) define how changes should be made and
  documented.

## Requirements

Pyboat requires Python 3.10 or newer. Set up a Python environment and install the
project with whichever environment and package-management tools you prefer.

The commands below use [`uv`](https://docs.astral.sh/uv/). To prepare that
environment from the repository root:

```sh
uv sync
```

[Nix](https://nixos.org/) is optional. If installed, it provides the development
tools, including `uv`:

```sh
nix develop
```

The Rust browser client additionally requires Rust 1.87 or newer with the
`wasm32-unknown-unknown` target, `wasm-pack`, and Bun. The Nix environment includes
all of them.

## Run the Rust connection server

```sh
cargo run -p pyboat-server
```

The Rust server listens on `ws://127.0.0.1:8765`, accepts concurrent WebSocket
connections, and creates one boat per client in a shared authoritative Rapier
simulation. It consumes compact-JSON sequenced input, applies location-dependent
ramming damage, preserves destroyed boats as physical wrecks, and publishes real
boat snapshots at 60 Hz. New boats spawn at a random bearing just beyond the
existing fleet in water at least 1.5 m deep and point downwind. Each server run
uses one match seed for wind that changes smoothly across the world and over time,
can blow in any direction, and includes gusts. Unsheltered wind speed follows a
`k = 2`, `λ = 9 m/s` Weibull distribution, so near-calms are possible and strong
winds become progressively rarer. One prevailing direction dominates around an
island, while distant regions belong to different broad weather systems. Islands
shelter nearby downwind water, with wind strength recovering over several hundred
metres.
Snapshots include nearby interaction state through 8 m and visual motion through
each client's requested viewport radius, bounded to 8–24 m and defaulting to 24 m.
Pass a different listen address as the final argument, for example
`cargo run -p pyboat-server -- 0.0.0.0:9000`.

## Run the Rust browser client

Start the Rust server in one terminal:

```sh
cargo run -p pyboat-server
```

Then start the browser workspace in another:

```sh
nix develop
bun install
bun run dev
```

Open the Vite URL shown in the terminal in a WebGPU-capable browser. The Rust/WASM
client connects to port `8765` on the page hostname, sends sequenced controls, and
reports its canvas-sized interest radius before rendering predicted articulated
boats with Rust `wgpu`. Use `W/S` for forward or reverse power and `A/D` to turn.
The first connected gamepad uses its left stick for world-relative movement. The
browser predicts the server's seeded weather locally. Players share the same
wind, moisture, instability, cloudiness, and rain intensity at a position, while
fine cloud shapes are generated locally and remain presentation-only. The browser
applies the weather at the local boat uniformly across the view. Layered stratus,
cellular cumulus, sparse cirrus wisps, and dense storm cells blend naturally from
those conditions. Mostly low, crisp cloud bodies follow only the slowly changing
large-scale wind and evolve without covering the world in grey. The HUD
reports health, speed through water, apparent-wind speed, and point of sail using
the wind at the local boat. One striped windsock on the local boat's bow shows
apparent-wind direction and strength: each visible band represents about 3 knots,
up to all five at 15 knots. The HUD also reports water depth, under-keel clearance, and whether the
hull is clear, dragging, or grounded. The mouse wheel zooms in or out without a
gameplay limit; click the mouse wheel to restore the original view. The lower-left
scale bar follows the zoom in real metric units. The same match seed produces a
connected, vastly dominant ocean with sparse clusters of large islands. Smaller
density fields shape coastlines, reefs, and sharp outcrops without placing terrain objects. Water
color changes continuously with depth before a distinct shoreline transition.
Gentle shore is beach, while steep shore and inland cliffs are bare rock; the rest
of each island is green. Broad geology varies by island, so some coasts are all
cliff, some alternate cliffs and beaches, and others remain beach-dominant. Cliffs
continue down their submerged flanks, and rare deep trenches break up the seafloor.
Boats slow progressively when their 0.6 m draft loses bottom clearance. Static
ground friction prevents creep after they settle, while sufficiently extreme
force can break that hold and slide the boat against lower kinetic friction.

Recently hit boats and wrecks receive distinct hull colors. When the local boat is
destroyed, the player continues spectating the drifting wreck. If the server
connection ends, a living local boat remains controllable through local prediction
while remote boats appear and behave as wrecks.

## Run the standalone game

```sh
uv run game
```

The standalone game is a four-buoy time trial. Pass the numbered buoys in order;
the active buoy is magenta. Press `R` to reset the boat and current lap while
retaining the best completed lap.

Choose a control mode with the number keys:

| Key | Mode |
| --- | --- |
| `1` | Computed yaw-rate arcade control (default) |
| `2` | Automatic sail trim with direct rudder control |
| `3` | Fixed yaw-rate arcade control |

In the arcade modes, `W/S` controls forward or reverse power and `A/D` turns. In
automatic-trim mode, `W` powers the sail and `A/D` controls the rudder directly. A
gamepad uses its left stick for world-relative movement.

## Run multiplayer

Start the authoritative server:

```sh
uv run server
```

Then start a client in each additional terminal:

```sh
uv run client
```

Multiplayer uses `W/S` for forward or reverse power and `A/D` to turn. Nearby
players are visible directly. Collisions cause damage based on the impact and where
the hull was struck. When the local boat is destroyed, its client closes; the other
players continue to see the drifting wreck.

This retained Python reference still exits on death or server disconnect. The Rust
browser intentionally differs during migration by continuing wreck spectating and
disconnected local cruising. The Python server also retains constant wind and does
not send the match seed required by the Rust browser. Rust/Python client-server
interoperability is not maintained during the migration.

The server listens on `127.0.0.1:8765` by default. Use `server --host` and
`server --port` to change the listener, and `client --uri` to connect clients to
another machine.

Pyboat is a prototype: the Python client remains the prediction and reconciliation
reference, while the Rust server and browser now provide an authoritative
multiplayer path with nearby physical prediction and reconciliation.
