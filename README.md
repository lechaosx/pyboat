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

The Rust browser demo additionally requires Rust 1.87 or newer with the
`wasm32-unknown-unknown` target, `wasm-pack`, and Bun. The Nix environment includes
all of them.

## Run the Rust browser demo

```sh
nix develop
bun install
bun run dev
```

Open the Vite URL shown in the terminal in a WebGPU-capable browser. The page runs
the real Rust/Rapier shared simulation at 60 Hz and renders two articulated boats
and a moving water grid with Rust `wgpu`. Use `W/S` for forward or reverse power
and `A/D` to turn. The first connected gamepad uses its left stick for
world-relative movement. The coral boat is a passive physical practice target;
both boats receive fluid forces and collide in the same Rapier world. The HUD
reports speed through water, apparent-wind speed, and point of sail. Multiplayer
remains in the Python prototype.

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
players are visible directly; off-screen or recently out-of-range players remain
indicated by directional arrows. Collisions cause damage based on the impact and
where the hull was struck. When the local boat is destroyed, its client closes;
the other players continue to see the drifting wreck.

The server listens on `127.0.0.1:8765` by default. Use `server --host` and
`server --port` to change the listener, and `client --uri` to connect clients to
another machine.

Pyboat is a prototype: the Python client and server currently establish the
gameplay and networking behavior, while the browser currently visualizes a single
locally simulated boat.
