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
simulation. It consumes the same compact-JSON sequenced input as the Python
prototype and publishes real boat snapshots at 60 Hz. Snapshots include nearby
interaction state through 8 m and visual motion through each client's requested
viewport radius, bounded to 8–24 m and defaulting to 24 m. Pass a different listen
address as the final argument, for example
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
HUD reports speed through water, apparent-wind speed, and point of sail.

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

The server listens on `127.0.0.1:8765` by default. Use `server --host` and
`server --port` to change the listener, and `client --uri` to connect clients to
another machine.

Pyboat is a prototype: the Python client remains the prediction and reconciliation
reference, while the Rust server and browser now provide an authoritative
multiplayer path with nearby physical prediction and reconciliation.
