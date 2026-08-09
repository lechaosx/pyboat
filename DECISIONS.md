# Technical decisions

This is the living record of decisions that affect the multiplayer architecture.
Update an entry when implementation or evidence changes it.

## Simulation authority and timing

- The server owns one shared Pymunk world and is authoritative for movement,
  collisions, and future combat outcomes.
- The server advances at a fixed 60 Hz. Rendering and network arrival do not alter
  the simulation step.
- Clients send control intent, not transforms. Server snapshots acknowledge only
  the receiving player's input sequence.

## Client latency compensation

- Boats outside the local interaction bubble use bounded kinematic extrapolation.
  Extrapolation stops after 250 ms and corrections are smoothed over 100 ms.
- The local player and boats within 8 m are recreated in one client-side Pymunk
  space. The local boat uses current input; other boats hold their latest
  server-reported input.
- Each authoritative snapshot resets that local physics cluster and it is replayed
  to render time. The server remains authoritative when predictions disagree.
- The client emits a sequenced user command every fixed tick. The server consumes
  one command per simulation tick and acknowledges it only after simulation.
  Reconciliation restores authoritative state and replays every unacknowledged
  command for exactly one fixed timestep, capped at 250 ms.
- Bubble correction offsets exist only in rendering and decay over 100 ms. Angular
  offsets use the shortest path across the ±π boundary; physical bodies correct
  immediately and never receive the visual offsets.
- The bubble starts before contact rather than at contact so the local solver has
  both hulls when the collision impulse begins.

## Per-player snapshots and spatial interest

- Snapshots are assembled per player.
- Boats within 8 m include motion and control intent for local physics prediction.
- Boats from 8 m through 24 m include visual motion but omit control intent.
- Boats beyond 24 m are omitted. The current radius covers the Python client's
  viewport plus a margin; a later variable viewport will need to communicate its
  interest bounds to the server.
- The server remembers which boats each connection has seen. Clients retain an
  omitted boat's last known position as an arrow, and the server sends a one-time
  tombstone if that boat disconnects.
- Server candidate lookup uses the spatial index already maintained by Pymunk,
  followed by an exact radial distance check. The client linearly scans only its
  already-culled snapshot.

## Boat geometry

- The rendered and physical hull use one outline defined in sailing coordinates.
  Pymunk may reorder its polygon vertices internally, but the collision and both
  Python renderers consume the same points.

## Ramming damage and wrecks

- Ramming damage is authoritative and comes from the first collision impulse.
  Contacts at or below 100 N·s do no damage; stronger impulses convert to health
  loss at 8 N·s per point before local hull strength is applied.
- The bow triangle has 2× strength, the middle hull has 1× strength, and the aft
  quarter has 0.6× strength. Each participant uses its own local contact point, so
  the same collision can damage the boats differently.
- A boat has 100 health. Recent damage is replicated with its server tick for
  brief client hit feedback.
- At zero health, the owner connection closes normally. The boat remains in the
  authoritative world as a wreck with articulated physics, fluid forces, and
  collisions, but no controller or actuator torque. Nearby clients predict the
  same controllerless wreck physics.
- Wreck lifetime and cleanup are intentionally undecided.

## Prototype transport

- The Python prototype uses JSON messages over WebSocket for easy inspection and
  browser compatibility.
- The simulation and protocol data are kept independent of rendering. Binary
  encoding or a different realtime transport should be considered only after
  measurements show JSON/WebSocket is the limiting factor.

## Network testing

- The Python client can inject round-trip latency and one-way jitter. Artificial
  round-trip latency is divided equally between outgoing inputs and incoming
  snapshots while preserving message order.
