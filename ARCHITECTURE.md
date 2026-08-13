# Architecture decisions

This file records implementation, simulation, networking, and operational
decisions.

- **[Explicit]** means the user directly requested or affirmatively confirmed the
  decision.
- **[Implicit]** means an agent selected the decision. Later acceptance of the
  resulting behavior does not make that underlying choice explicit.

## Reference implementation and boundaries

- **[Explicit]** The current gameplay reference stack consists of Python clients
  with a simple renderer and a Python dedicated server.
- **[Implicit]** The initial browser vertical slice ran one locally controlled boat
  and a passive practice boat in the same Rust shared simulation before protocol
  and multiplayer state were introduced. That local demo remains a native test
  harness rather than the current browser entry point.
- **[Implicit]** Simulation and protocol data are independent of rendering, so a
  later browser client can reuse the same behavior without copying the Python
  renderer.
- **[Implicit]** The current Python implementation is the behavioral reference
  until another client passes equivalent networking and physics tests.
- **[Explicit]** Rust/Python client-server interoperability and exact numerical
  physics parity are not migration requirements. The target Rust stack may change
  its protocol and behavior independently of the retained Pymunk implementation.
- **[Explicit]** Once the Rust implementation surpasses the Python version, the
  retained Python implementation and its documentation references will be
  removed.
- **[Implicit]** During migration, the Python client retains its historical
  death/disconnect exit behavior while the Rust browser implements continued
  wreck spectating and disconnected cruising.
- **[Implicit]** Migration proceeds behavior by behavior through a
  runtime-independent `pyboat-simulation` Rust crate. Each migrated behavior has
  Rust tests for its accepted physical invariants and gameplay behavior before
  browser or native runtime adapters consume it. Hull construction, the
  articulated boat assembly, and
  authoritative motion restoration, fixed-step advancement, and actuator torque
  application, passive bearing damping, and flat-plate sail load application are
  the first migrated behaviors, followed by stalled underwater-foil loads on the
  articulated rudder and hull-mounted keel, anisotropic hull resistance, and
  surface-target actuator control, followed by arcade surface-effect sampling and
  target selection, controlled fixed-step advancement, and shared-world stepping
  with hull impulse and local-contact reporting, articulated-boat removal, and
  maintained radial spatial queries.
- **[Implicit]** The runtime-independent `pyboat-game` crate owns the concrete
  dinghy parameters, deterministic procedural-environment sampling, and
  computed-yaw-rate control composition used by both the Rust server and browser.
  `pyboat-simulation` remains parameterized and independent of game-specific
  tuning.
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
- **[Implicit]** `wasm-pack` builds the Rust browser crate, Vite bundles the page,
  and Bun runs workspace commands. TypeScript starts the page and schedules frames
  while Rust owns WebGPU initialization, networking, replicated state, fixed-tick
  input emission, and scene rendering.
- **[Implicit]** `pyboat-protocol` owns typed runtime-independent wire messages,
  while the native `pyboat-server` uses Tokio and tokio-tungstenite for async
  WebSocket connection handling.
- **[Implicit]** The Rust server owns one shared authoritative Rapier match. Each
  completed WebSocket connection creates a distinct articulated boat, validated
  sequenced input drives computed-yaw-rate arcade control, live disconnects remove
  that boat from the physics world, and destroyed boats remain as controllerless
  wrecks.
- **[Explicit]** A newly connected boat spawns on a random bearing just outside
  the radial extent of all other players and points downwind at its spawn point.
- **[Implicit]** The first boat's deterministic safe-water position becomes the
  match spawn origin. Each later boat checks seed-derived bearings at 8 m beyond
  the fleet's radial extent around that origin, including wrecks, and expands the
  search outward only when that ring has no position at least 1.5 m deep.
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
- **[Implicit]** TypeScript samples keyboard and browser gamepad state once per
  animation frame. Rust clamps keyboard intent and converts the gamepad's
  world-relative left-stick vector into boat-relative arcade intent.
- **[Implicit]** Rust derives speed through water, apparent-wind speed, and point
  of sail from predicted and reconciled local state for the browser HUD.
  TypeScript formats this small read-only view and updates the DOM without
  receiving raw snapshots.
- **[Explicit]** Browser arcade input follows the standalone Python game's device
  mapping, except that the browser adds no software gamepad deadzone. A displaced
  first connected gamepad overrides keyboard input, while an exactly centered or
  absent gamepad leaves keyboard input active.
- **[Explicit]** Server and browser runtime work should use asynchronous I/O and
  orchestration wherever work can suspend.
- **[Implicit]** The native server treats Ctrl-C as an asynchronous shutdown
  signal. Shutdown publishes a WebSocket close frame with code 1001 and the
  reason `Server shutting down` to each established client, then returns the
  authoritative serving loop after allowing up to one second for connection
  handlers to flush those frames. Handlers do not wait for a peer close response.
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
- **[Implicit]** The Rust shared simulation inserts all articulated boats into one
  Rapier world, accepts one environment sample per boat, applies every boat's
  loads before advancing that world once, and returns active hull contact pairs
  with Rapier's resolved impulse magnitude and averaged hull-local contact points
  for game rules such as damage.
- **[Implicit]** Each Rust server process creates one match seed at startup. A
  pure function of that seed, simulation tick, and world position evaluates 48
  divergence-free Fourier modes with seeded Gaussian coefficients. Modes are
  evaluated directly rather than interpolating random time-cell values. Terrain
  sampled along the resulting upwind direction then modifies the surface wind.
  The current procedural environment leaves water current at zero.
- **[Explicit]** Static terrain is one continuous seeded density field inspired
  by Minecraft's composition of progressively smaller shaping fields. Reefs and
  rocks remain part of this field rather than becoming separately placed or
  replicated objects. Horizontal distance and elevation both use uncompressed
  real metres.
- **[Explicit]** Terrain has no continent-scale density layer that can enclose
  players in regional seas. Three 8,192–2,048 m cluster octaves and five
  1,024–64 m island-detail octaves produce sparse large islands in a connected,
  dominant ocean.
- **[Implicit]** The production base elevation scale is `3×`. Positive and
  negative tails of the existing 8,192–2,048 m cluster density independently
  amplify signed relief: positive `0.32–0.58` density produces rugged major
  islands up to `60×`, while negative `0.50–0.74` density produces rarer trenches
  up to `90×`. Because relief derives from the same broad field as island shape,
  it crosses sea level continuously without a separate cliff-biome noise field.
- **[Implicit]** Four 32–4 m reef octaves have separate roles. Between -4 and
  -0.1 m elevation, a smooth mask applies up to 0.6 m of signed relief to the
  submerged seabed. A narrower -2 to -0.1 m mask adds up to 2 m only where the
  reef field exceeds its 0.65–0.85 upper-tail spline, producing sparse emergent
  rocks without adding reef-scale noise to the parent island silhouette.
- **[Implicit]** NOAA BlueTopo 4 m grids for rocky Maine and the Florida Keys
  calibrate the coastal relief spectrum in the same -4 to 1 m elevation band as
  the game. Observed 90th-percentile relief provides upper bounds at 32 m and
  128 m baselines rather than a shoreline-shape target. Statistical samples also
  limit aggregate 256 m shoreline fragmentation and require a 4 m-deep transect
  through some, but fewer than one quarter, of mixed reef neighborhoods.
- **[Implicit]** Each sampled simulation environment includes water depth derived
  from that elevation. `pyboat-simulation` treats unspecified depth as infinite,
  preserving terrain-independent harnesses and the accepted deep-water behavior.
- **[Implicit]** The browser derives water depth, clearance below the 0.6 m keel,
  and bottom support fraction from the same locally predicted environment used
  for grounding. These navigation readouts require no protocol fields.
- **[Explicit]** At a fixed position and time, unsheltered wind direction is
  uniform over the full circle and speed has a Weibull marginal with `k = 2` and
  `λ = 9 m/s`. Smooth spatial and per-tick temporal evolution includes gusts.
- **[Explicit]** Wind around a roughly 2 km island has a recognizable prevailing
  direction except near a calm, while wind across the very large world varies
  smoothly among broad weather systems without one universal direction.
- **[Implicit]** The spectral model assigns 90% of component variance to
  32,768–131,072 m weather modes, 8% to 4,096–32,768 m regional modes, and 2% to
  64–512 m gust modes. One seed-derived 6 m/s transport vector translates the
  complete field without deforming it. Gust structures vary over approximately
  10.7–85 seconds, while the dominant weather structures pass over hours.
- **[Explicit]** Match-wide weather state is distinct from its presentation.
  Seed, tick, and position determine identical wind, coarse cloud cover, and
  rain intensity for every participant. Fine cloud geometry may be
  generated independently per player but must look similar under the same local
  weather state.
- **[Explicit]** The renderer samples the shared seeded weather at the local boat
  and supplies its sea-level, low-cloud, and high-cloud winds, moisture,
  instability, low and high cloudiness, and rain intensity as parameters to local procedural
  rendering. That one state applies uniformly across the viewport, including at
  extreme debug zoom.
- **[Explicit]** Visible clouds combine wind advection with time-varying noise so
  they move and evolve without a world-sized fluid simulation. Clouds do not
  participate in visibility or any other gameplay rule.
- **[Explicit]** Every cloud family combines a family-specific isotropic detail
  field with a lower-frequency formation mask. Detail coordinates never rotate;
  mask coordinates align directly to the low-frequency layer-wind direction and
  may rotate to reorganize the broad formation without rotating individual cloud
  texture.
- **[Explicit]** One continuous parameter splits total layer advection between
  detail and mask coordinates. Both fields continue evolving through spacetime
  noise independently of that split.
- **[Implicit]** Production assigns all spatial advection to the formation mask,
  making clouds form at their leading edge and dissipate at their trailing edge.
- **[Explicit]** Cloud motion and formation use only the lowest-frequency wind
  components so regional and gust-scale changes do not abruptly redirect them.
- **[Explicit]** Sea-level, low-cloud, and high-cloud winds are coherent samples
  of one seeded weather system rather than independent fields. The common
  synoptic flow dominates all three; shorter-scale variation increases toward
  sea level, while average wind speed increases with altitude. Terrain shelter
  affects the final wind used by boat physics, not the winds aloft.
- **[Explicit]** Common cloud formations emerge from continuous weather
  parameters rather than a discrete cloud-type selector.
- **[Implicit]** Shared cloud cover is five octaves of smooth value noise over
  4,096–256 m in the transported weather coordinates. A `0.05–0.15` body edge and
  `0.15–0.75` interior mapping leave more than half of representative samples
  clear, 10–40% cloudy, and fewer than 1% above 0.85 storm density. That field also
  supplies continuous moisture. Four 16,384–2,048 m octaves supply independent
  instability, and four 32,768–4,096 m octaves supply sparse high-cloud potential.
  Rain intensity rises through `0.7–0.95` from cloudiness weighted 65–100% by
  instability.
- **[Explicit]** Near-land wind uses a simplified deterministic model of lee
  shelter over the sea. Match seed, tick, and position remain sufficient for every
  runtime to compute the same terrain-modified wind without transferring samples.
- **[Implicit]** The shared sampler checks terrain 16, 32, 64, 128, 256, and 512 m
  upwind. Positive elevation becomes a progressively stronger obstruction through
  1.5 m, its effect recovers with a 256 m distance scale, and the strongest sample
  can reduce speed by at most 70%. This first model preserves wind direction.
- **[Explicit]** The match seed is the only procedural world or environment datum
  transferred from server to client. Authoritative snapshots still synchronize
  changing boat state; they do not carry sampled environment fields.
- **[Implicit]** Server and browser physics use the same shared Rust deterministic
  functions and tuning to derive matching parameters from the seed, simulation
  tick, and world position. Browser terrain rendering mirrors the static elevation
  algorithm in WGSL rather than transferring samples.
- **[Implicit]** The server samples the environment independently at each boat's
  position before every authoritative tick. The browser uses the same seed, tick,
  and position sampling for each predicted boat and for the local sailing HUD.
- **[Implicit]** The browser derives one local apparent-wind vector from seeded
  true wind and predicted boat velocity. The WebGPU scene renders only its
  inflated fraction as a striped windsock attached to the local bow. Five equal
  bands map linearly to 0–15 knots, including a partial final band; remote boats
  receive no wind indicator.
- **[Explicit]** Browser terrain is not polygonized. One fullscreen quad supplies
  world positions, and the WebGPU fragment shader evaluates seeded elevation per
  pixel at every zoom level. Physics samples depth only where boats require it.
- **[Implicit]** WGSL mirrors Rust's cluster, island-detail, and coastal-reef
  density fields and emulates their 64-bit seed hashing with pairs of
  32-bit integers. The shader owns all terrain presentation: it continuously
  blends shallow, open, and deep-water colors from sea level through 4 m depth,
  switches distinctly to land at sea level, and propagates analytic world-space
  derivatives through noise, density splines, masks, and peaks. Those derivatives
  provide height-field lighting and slope-based rock exposure. The rock-slope
  threshold decreases with altitude; broad altitude fields then select alpine
  rock and sparse snow. Seeded fields grade rock, grass, and forest internally,
  while beach, vegetation, rock, and snow retain sharp material boundaries.
  Broad geology varies beach width continuously from sea level, so sand does not
  appear as disconnected inland patches.
- **[Implicit]** `pyboat-game::procedural_weather` composes unsheltered surface
  wind, low- and high-cloud winds, moisture, instability, low and high cloudiness,
  and rain intensity. High-cloud wind is the dominant 32,768–131,072 m synoptic
  band at 118% speed. Low-cloud wind blends that band with the complete surface
  field at 108% speed. WGSL combines family-specific isotropic bases with direct
  wind-aligned, lower-frequency masks for cumulus, stratus, cirrus, squall, and
  storm formations. Mask ridges organize elongated families while their base
  fields retain distinct fine texture. The model evaluates 46 noise octaves per
  terrain pixel, down from 99 in the fixed-axis and directional-filter model.
  Density, optical depth,
  darkness, and relief remain separate so thin cloud stays transparent, dense
  rain cloud darkens continuously, and fine internal detail survives inside the
  silhouette. Instability weights cumulus against stratus; instability with rain
  strengthens storm cells. The browser integrates separate camera-local low- and
  high-cloud offsets, avoiding a global flow simulation. Cloud shading applies
  only to the fullscreen world surface, so boats remain equally visible to
  clients with different visual seeds.
- **[Implicit]** Cloud shadows evaluate the local cloud field once more at the
  point where the terrain-to-sun ray crosses the weather-derived cloud layer.
  Low-cloud height rises from 650 m with instability and rain, while high cloud
  uses 6,200 m; mixed conditions interpolate one effective projection height.
  Terrain elevation shortens that projection. Separate low/high optical depths
  make the translucent surface attenuation follow the projected cloud family.
- **[Implicit]** A DOM overlay derives a segmented architectural-style 1–2–5
  metric scale from the current vertical view distance and CSS viewport height.
  It does not add world geometry or affect simulation and networking.
- **[Explicit]** Procedural weather and terrain tuning has a separate web app,
  not an in-game mode. It imports the production WGSL shader and renders its
  fullscreen world quad directly without multiplayer, simulation, or a server.
- **[Implicit]** Three additional scene-uniform vectors carry cloud-noise,
  cloud-material, and terrain tuning. The game and native showcase always supply
  production defaults; only the playground edits them. This keeps one shader as
  the rendering source of truth while preserving normal gameplay behavior.
- **[Implicit]** The retained local browser test harness advances its controlled
  and passive practice boats through the shared-world API, exercising collision
  response independently of networking.
- **[Implicit]** Each Rust simulation tick applies sail and rudder actuator
  torques equally and oppositely to the controlled surface and hull. Rapier uses
  eight solver iterations with joint warm-starting so sustained actuator loads
  keep the articulated surfaces within their configured limits.
- **[Implicit]** Rust surface target control uses torque-limited proportional
  control with critical damping derived from each surface assembly's hinge
  inertia. A sheet target may pull the sail inward but suppresses actuator torque
  that would push it outward; explicit sail targets remain bidirectional.
- **[Implicit]** Rust arcade control samples quasi-static sail and rudder effects
  across their allowed angles using hull translation and angular velocity. It
  preserves wind-side hysteresis, auto-sheet trim, backed-sail reverse, direct
  rudder control, and fixed or water-speed-derived yaw-rate steering from the
  accepted Python controller.
- **[Implicit]** One Rust controlled-advance operation accepts a mode-specific
  command, selects surface targets, derives actuator torques, applies all physical
  loads, and advances exactly one fixed tick. It returns the selected targets and
  active maximum yaw rate for runtime telemetry.
- **[Implicit]** The authoritative browser client limits input catch-up to 250 ms,
  emits commands only in complete 60 Hz ticks, and advances its nearby prediction
  world once for each emitted command.
- **[Implicit]** The Rust server's Tokio interval advances one synchronous physics
  tick and publishes its resulting snapshot at 60 Hz. Each connection retains
  only the newest pending snapshot, preventing a slow client from accumulating
  stale latency.
- **[Explicit]** Passive sail and rudder bearing damping uses Rapier force-based
  revolute-joint motors targeting zero relative angular velocity. Damping is
  validated by energy loss, common-rotation preservation, and angular-momentum
  conservation rather than matching Pymunk's numerical decay curve.
- **[Implicit]** The Rust sail load uses the symmetric flat-plate coefficients
  `Cl = sin(2α)` and `Cd = Cd0 + 1 - cos(2α)`. The pure force calculation uses
  apparent flow and dynamic pressure without depending on a physics runtime.
- **[Implicit]** Rust keel and rudder loads use a symmetric streamlined lift
  curve: lift grows linearly to the configured stall point, then transitions
  toward separated flat-plate cross-flow while drag retains the flat-plate law.
- **[Implicit]** Each Rust tick samples apparent water at the articulated
  rudder's moving center of effort and at the hull-mounted keel position, then
  applies each streamlined foil load to its supporting rigid body at that point.
- **[Implicit]** Rust hull resistance remains quadratic and anisotropic. Half of
  the configured reference area is sampled at each end of the hull so local
  lateral flow creates a physically opposing yaw torque.
- **[Implicit]** A boat whose sampled water depth is below its configured draft
  transfers a proportional fraction of its weight onto the seabed. When the
  external force and yaw wrench fit inside the static-friction envelope, Rapier
  fixes the hull until that demand breaks the hold. Otherwise capped kinetic
  friction acts at the bow and stern against ground-relative translation and yaw
  without overshooting rest. The current dinghy uses a 0.6 m draft with effective
  static and kinetic friction coefficients of 1.0 and 0.6.
- **[Implicit]** Each Rust tick replaces the preceding tick's user-applied loads,
  samples apparent wind at the moving sail center of effort, and applies the sail
  force to the articulated sail at that world-space point.
- **[Implicit]** Clients send sequenced control commands rather than transforms.
  Server snapshots acknowledge only commands that have affected an authoritative
  simulation tick.
- **[Implicit]** Rapier enables `enhanced-determinism` in the shared simulation
  crate. SIMD and parallel features remain disabled until profiling demonstrates
  a need and native/WebAssembly equivalence tests cover the selected feature set.

## Transport and protocol

- **[Explicit]** Compact JSON over WebSocket remains the transport until
  measurement identifies it as a bottleneck. Binary encoding is introduced only
  in response to that evidence.
- **[Implicit]** `pyboat-protocol` defines the Rust stack's wire contract. Its
  `welcome` message includes the match's `matchSeed`, which the retained Python
  server does not send; cross-runtime client-server pairings are unsupported.
- **[Implicit]** The Rust server limits client WebSocket messages and individual
  frames to 1 KiB. Current client messages contain only compact input and viewport
  commands, so larger traffic is rejected before JSON decoding.
- **[Implicit]** A TCP connection must complete its WebSocket upgrade within five
  seconds. Incomplete handshakes are closed without consuming a player identity,
  spawn position, or authoritative match slot.
- **[Implicit]** After upgrade, the connection task waits for the authoritative
  loop to register its player and shutdown mailbox before sending `welcome`.
  Receiving `welcome` therefore guarantees that controlled shutdown includes the
  connection, and any queued snapshot follows the welcome message.
- **[Implicit]** The Rust/WASM client uses `web_sys::WebSocket`, decodes protocol
  messages in Rust, and retains only the newest pending snapshot between rendered
  frames. Welcome identity and match seed are applied immediately and are not part
  of that latest-only mailbox.
- **[Implicit]** The typed Rust snapshot envelope contains `tick`, `boats`, and
  `removedPlayerIds`. Its boat type contains physical motion, health and wreck
  state, optional nearby intent, and optional local acknowledgement fields.
- **[Implicit]** Rust server snapshots leave `removedPlayerIds` empty, and the
  Rust browser treats `boats` as the complete current interest set.
- **[Explicit]** Snapshots are assembled per receiving player because nearby boats
  require more prediction data than distant boats.
- **[Implicit]** The browser sends a compact viewport-radius message after welcome
  and whenever its canvas aspect ratio changes. The radius covers the viewport's
  half-diagonal plus a 2.5 m boat-geometry margin.

## Client prediction and reconciliation

- **[Explicit]** Client prediction includes physical interaction between the local
  boat and nearby boats rather than only the local transform.
- **[Explicit]** Steering responsiveness uses Source-style prediction and
  reconciliation.
- **[Implicit]** The client emits one sequenced command per fixed tick. The server
  consumes one queued command per simulation tick, and the client restores the
  acknowledged state before replaying each unacknowledged command for one fixed
  timestep.
- **[Implicit]** Each Rust server player retains at most the newest 15 pending
  commands, matching 250 ms at 60 Hz. Overflow discards the oldest unapplied
  commands so a burst cannot create an unbounded stale-control backlog.
- **[Implicit]** The client retains and replays only the newest 15 unacknowledged
  commands, capping prediction history at 250 ms to bound memory, replay work, and
  runaway prediction during a snapshot stall.
- **[Implicit]** A browser connection error or close stops network input emission
  but continues fixed-tick local prediction from the last replicated scene. The
  local boat remains controllable when alive; remote boats become passive wrecks
  with zero health. A clean close reason remains visible in the connection status.
- **[Implicit]** The client recreates the local boat and boats within 8 m in one
  Rapier prediction space. The local boat uses unacknowledged commands; nearby
  boats hold their latest server-reported command.
- **[Explicit]** The interaction bubble begins shortly before contact rather than
  activating only after collision begins.
- **[Implicit]** Boats outside the physics bubble use kinematic extrapolation,
  capped at 250 ms.
- **[Explicit]** Entering and leaving the nearby physics bubble and accepting new
  authoritative motion do not snap a boat's rendered pose or make its kinematic
  motion visibly jerky.
- **[Implicit]** Authoritative corrections affect physics immediately, while a
  render-only cubic correction converges position, hull angle, sail angle, rudder
  angle, and their visual velocities over 100 ms. It preserves pose and velocity
  at both ends, and angular offsets follow the shortest path across the ±π boundary.
- **[Explicit]** The current prediction and reconciliation behavior has been
  manually accepted as feeling great and is the networking responsiveness
  baseline. Contrary measurements or playtest evidence are the basis for changing
  it.

## Spatial interest and awareness

- **[Explicit]** Boats sufficiently far outside the viewport do not need to be
  included in snapshots.
- **[Implicit]** Boats within 8 m include full motion and control data. Beyond 8 m,
  boats through the receiving client's requested viewport radius include visual
  motion without control data. The server clamps that radius to 8–24 m and uses
  24 m for clients that do not send viewport updates.
- **[Explicit]** Server interest management requires a maintained spatial query
  system.
- **[Implicit]** Server candidate lookup uses the physics world's maintained
  broad-phase spatial index followed by an exact radial distance check: Pymunk in
  the Python reference and Rapier in the Rust server. Clients linearly scan only
  their already-culled snapshots.
- **[Explicit]** Directional arrows to other ships are a debug feature and are not
  required in the Rust browser.
- **[Implicit]** Each Rust snapshot's `boats` array is the receiver's complete
  current interest set. The browser discards an omitted boat immediately, so the
  server does not track per-connection seen IDs or reliable removal tombstones.

## Boat geometry

- **[Explicit]** Rendered hull geometry and collision geometry must agree.
- **[Implicit]** One hull outline in sailing coordinates supplies Pymunk collision
  geometry, moment of inertia, and both Python renderers. Pymunk may reorder
  polygon vertices internally.

## Damage and wreck simulation

- **[Implicit]** Ramming damage uses the first authoritative collision impulse and
  applies damage independently to both boats using each hull's local contact point.
- **[Implicit]** The Rust server tracks active contact pairs by stable player ID so
  a sustained contact damages only once and live-player index compaction cannot
  suppress a later collision.
- **[Implicit]** Snapshots replicate health and the most recent damage tick for
  client status and brief hit feedback.
- **[Explicit]** Reaching zero health keeps the destroyed boat owner connected as
  a spectator centered on that wreck.
- **[Explicit]** A destroyed boat remains in authoritative and nearby client
  physics as a wreck with hull, sail, rudder, momentum, fluid forces, and
  collisions but no controller.
- **[Implicit]** Controllerless wrecks receive passive aerodynamic, hydrodynamic,
  hull, and joint forces with zero actuator torque.
- **[Implicit]** Nearby browser prediction also gives wrecks zero actuator torque,
  preventing stale replicated intent from controlling them.
- **[Implicit]** A browser spectating its local wreck emits no further input
  commands while continuing to predict the wreck's passive motion between
  snapshots.

## Network testing

- **[Implicit]** The Python client can inject round-trip latency and one-way jitter
  for playtesting. Artificial round-trip latency is split equally between outgoing
  commands and incoming snapshots while preserving message order.
- **[Implicit]** Automated integration tests use real loopback WebSocket
  connections in addition to deterministic simulation and protocol tests.
- **[Implicit]** Rust loopback tests open real WebSocket connections against an
  ephemeral local port and verify concurrent connection identity, sequenced input
  acknowledgement, bounded input backlog, message size, handshake time and failed
  handshake identity isolation, authoritative motion, spatial interest payloads,
  viewport-radius bounds, disconnect cleanup, and the code and reason of
  protocol-clean signaled server shutdown. Shutdown coverage includes dropping an
  isolated server runtime as soon as its serving future returns, ensuring close
  frames are flushed before return.
- **[Implicit]** Native browser-crate tests verify replicated state, input
  sequencing, nearby physical prediction, distant extrapolation, bubble-transition
  continuity, reconciliation, damage feedback, and controllerless wreck prediction
  independently of browser APIs.
- **[Implicit]** A browser-owned native loopback test drives `MultiplayerClient`
  through the real Rust WebSocket server, deliberately skips intermediate
  snapshots, and verifies fixed-tick input acknowledgement and bounded prediction.

## Open architecture questions (not decisions)

- Whether wreck cleanup is time-based, distance-based, capacity-based, or driven by
  salvage gameplay.
- Whether measured scale eventually justifies binary snapshots, batching, UDP, or
  another transport.
