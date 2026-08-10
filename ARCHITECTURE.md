# Architecture decisions

This file records implementation, simulation, networking, and operational
decisions.

- **[Explicit]** means the user directly requested or affirmatively confirmed the
  decision.
- **[Implicit]** means an agent selected the decision. Later acceptance of the
  resulting behavior does not make that underlying choice explicit.

## Reference implementation and boundaries

- **[Explicit]** The current reference stack consists of Python clients with a
  simple renderer and a Python dedicated server. Browser rendering is deferred.
- **[Implicit]** Simulation and protocol data are independent of rendering, so a
  later browser client can reuse the same behavior without copying the Python
  renderer.
- **[Implicit]** The current Python implementation is the behavioral reference
  until another client passes equivalent networking and physics tests.
- **[Implicit]** Migration proceeds behavior by behavior through a
  runtime-independent `pyboat-simulation` Rust crate. Each migrated behavior has
  a Rust test against the Python reference before browser or native runtime
  adapters consume it. Hull construction, the articulated boat assembly, and
  authoritative motion restoration, fixed-step advancement, and actuator torque
  application are the first migrated behaviors.
- **[Explicit]** The Rust simulation may differ from Pymunk implementation details
  and exact numerical trajectories when the result is physically correct.
  Equivalence is judged by physical invariants and accepted gameplay behavior.

## Target browser and server stack

- **[Explicit]** The target gameplay runtime is written in Rust. It compiles to
  WebAssembly for the browser client, while the dedicated server uses the same
  simulation code natively.
- **[Explicit]** The browser renderer is written in Rust using `wgpu`.
- **[Explicit]** The initial browser build targets WebGPU only and does not include
  `wgpu`'s WebGL compatibility backend, prioritizing a smaller download and one
  renderer capability model over support for browsers without WebGPU.
- **[Explicit]** Rapier 2D provides rigid-body physics for the target Rust
  simulation on both native and WebAssembly builds.
- **[Implicit]** Rapier owns collision detection, contact resolution, and spatial
  queries. Project simulation code owns boat forces, control, damage, wreck
  behavior, and other game rules.
- **[Explicit]** The browser gameplay client owns its WebSocket connection,
  protocol codec, replicated world state, prediction and reconciliation, physics,
  and rendering in Rust/WebAssembly. Gameplay packets do not pass through a
  handwritten TypeScript transport or parser.
- **[Explicit]** TypeScript with HTML and CSS owns browser presentation such as
  menus and the DOM-based HUD. It exchanges commands, a small derived UI view
  model, and discrete UI events with the Rust/WebAssembly client rather than raw
  gameplay snapshots.
- **[Explicit]** Server and browser runtime work should use asynchronous I/O and
  orchestration wherever work can suspend.
- **[Implicit]** The fixed-step simulation itself remains synchronous so a tick
  never waits on networking, timers, persistence, rendering, or UI work.
- **[Implicit]** The target runtime uses stable Rust. Nightly features are adopted
  only when a concrete implementation need justifies their compatibility cost.

## Simulation authority and timing

- **[Explicit]** The dedicated server is authoritative, while clients compensate
  for latency without exposing the correction to the player where practical.
- **[Implicit]** The server owns one shared Pymunk space for all live boats and
  wrecks, allowing authoritative physical interaction.
- **[Implicit]** The server advances at a fixed 60 Hz. Rendering and packet arrival
  do not change the physics timestep.
- **[Implicit]** The Rust simulation owns a zero-gravity Rapier `PhysicsWorld` and
  exposes one synchronous 60 Hz advance operation. Runtime adapters perform
  asynchronous work before and after, never during, that operation.
- **[Implicit]** Each Rust simulation tick applies sail and rudder actuator
  torques equally and oppositely to the controlled surface and hull. Rapier uses
  eight solver iterations with joint warm-starting so sustained actuator loads
  keep the articulated surfaces within their configured limits.
- **[Implicit]** Clients send sequenced control commands rather than transforms.
  Server snapshots acknowledge only commands that have affected an authoritative
  simulation tick.
- **[Implicit]** Rapier enables `enhanced-determinism` in the shared simulation
  crate. SIMD and parallel features remain disabled until profiling demonstrates
  a need and native/WebAssembly equivalence tests cover the selected feature set.

## Transport and protocol

- **[Implicit]** The Python prototype uses compact JSON messages over WebSocket
  because they are inspectable and browser-compatible.
- **[Implicit]** JSON and WebSocket remain the prototype transport until
  measurement identifies transport overhead as a limiting factor; binary encoding
  or another realtime transport is contingent on that evidence.
- **[Explicit]** Snapshots are assembled per receiving player because nearby boats
  require more prediction data than distant boats.

## Client prediction and reconciliation

- **[Explicit]** Client prediction includes physical interaction between the local
  boat and nearby boats rather than only the local transform.
- **[Explicit]** Steering responsiveness uses Source-style prediction and
  reconciliation.
- **[Implicit]** The client emits one sequenced command per fixed tick. The server
  consumes one queued command per simulation tick, and the client restores the
  acknowledged state before replaying each unacknowledged command for one fixed
  timestep.
- **[Implicit]** Replay is capped at 250 ms to bound work and runaway prediction.
- **[Implicit]** The client recreates the local boat and boats within 8 m in one
  Pymunk prediction space. The local boat uses unacknowledged commands; nearby
  boats hold their latest server-reported command.
- **[Explicit]** The interaction bubble begins shortly before contact rather than
  activating only after collision begins.
- **[Implicit]** Boats outside the physics bubble use kinematic extrapolation,
  capped at 250 ms.
- **[Implicit]** Authoritative corrections affect physics immediately, while
  render-only position, hull angle, sail angle, and rudder angle offsets decay over
  100 ms. Angular offsets follow the shortest path across the ±π boundary.
- **[Explicit]** The current prediction and reconciliation behavior has been
  manually accepted as feeling great and is the networking responsiveness
  baseline. Contrary measurements or playtest evidence are the basis for changing
  it.

## Spatial interest and awareness

- **[Explicit]** Boats sufficiently far outside the viewport do not need to be
  included in snapshots.
- **[Implicit]** Boats within 8 m include full motion and control data, boats from
  8 m through 24 m include visual motion without control data, and boats beyond
  24 m are omitted.
- **[Explicit]** Server interest management requires a maintained spatial query
  system.
- **[Implicit]** Server candidate lookup uses Pymunk's maintained broad-phase
  spatial index followed by an exact radial distance check. Clients linearly scan
  only their already-culled snapshots.
- **[Explicit]** An omitted boat retains an arrow pointing toward its last known
  position.
- **[Implicit]** The server remembers which boats each connection has seen and
  sends a one-time tombstone only when a previously seen boat disconnects. This
  preserves arrows without sending all distant player IDs continuously.

## Boat geometry

- **[Explicit]** Rendered hull geometry and collision geometry must agree.
- **[Implicit]** One hull outline in sailing coordinates supplies Pymunk collision
  geometry, moment of inertia, and both Python renderers. Pymunk may reorder
  polygon vertices internally.

## Damage and wreck simulation

- **[Implicit]** Ramming damage uses the first authoritative collision impulse and
  applies damage independently to both boats using each hull's local contact point.
- **[Implicit]** Snapshots replicate health and the most recent damage tick for
  client status and brief hit feedback.
- **[Explicit]** Reaching zero health causes the destroyed boat owner's client to
  close normally.
- **[Explicit]** A destroyed boat remains in authoritative and nearby client
  physics as a wreck with hull, sail, rudder, momentum, fluid forces, and
  collisions but no controller.
- **[Implicit]** Controllerless wrecks receive passive aerodynamic, hydrodynamic,
  hull, and joint forces with zero actuator torque.

## Network testing

- **[Implicit]** The Python client can inject round-trip latency and one-way jitter
  for playtesting. Artificial round-trip latency is split equally between outgoing
  commands and incoming snapshots while preserving message order.
- **[Implicit]** Automated integration tests use real loopback WebSocket
  connections in addition to deterministic simulation and protocol tests.

## Open architecture questions (not decisions)

- How a variable-size future viewport communicates interest bounds to the server.
- Whether wreck cleanup is time-based, distance-based, capacity-based, or driven by
  salvage gameplay.
- Whether measured scale eventually justifies binary snapshots, batching, UDP, or
  another transport.
- Whether sail and rudder bearing damping should use Rapier joint motors or
  explicit project-owned torques.
- Which Python simulation behavior should be migrated after actuator torque
  application.
