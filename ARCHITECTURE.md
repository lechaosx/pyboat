# Architecture decisions

This record contains technical choices that constrain future implementation. Code
and tests remain the source of truth for algorithms, constants, and schemas.

| Mark | Meaning |
| --- | --- |
| **[Explicit]** | The user requested or clearly endorsed the choice, including after it was initially implicit |
| **[Implicit]** | An agent selected the choice |

## Runtime boundaries

- **[Explicit]** Rust is the gameplay runtime. The same simulation compiles
  to WebAssembly for the browser and runs natively on the dedicated server.
- **[Implicit]** The runtime-independent `simulation` crate owns reusable physics
  and control behavior. `game` owns concrete game policy, tuning, control
  composition, and procedural environment sampling shared by native and browser
  runtimes.
- **[Explicit]** Rapier 2D provides target rigid-body physics on native and
  WebAssembly builds.
- **[Explicit]** The browser renderer is Rust `wgpu`, initially WebGPU-only rather
  than carrying a WebGL compatibility backend.
- **[Explicit]** Rust/WASM owns gameplay transport, protocol decoding, replicated
  state, prediction, reconciliation, physics, and rendering. TypeScript, HTML,
  and CSS own menus and DOM presentation and receive only commands, discrete
  events, and a small derived UI view.

## Authority and simulation

- **[Explicit]** The dedicated server is authoritative. Clients predict locally
  and reconcile corrections without exposing them where practical.
- **[Implicit]** Each match advances all boats and wrecks together in one
  fixed-step physics world. Runtime adapters gather asynchronous work
  before a synchronous tick and publish results afterward; simulation never
  awaits I/O, timers, persistence, rendering, or UI.
- **[Implicit]** Project code owns fluid loads, articulated control, game rules,
  damage, and wreck behavior. Rapier owns rigid-body integration, joints,
  collision response, and spatial queries.
- **[Explicit]** Rendered hull geometry and collision geometry agree.
- **[Explicit]** A destroyed boat retains its bodies, momentum, fluid forces, and
  collisions but has no controller. Its owner stays connected as a spectator
  centered on it.

## Procedural environment and rendering

- **[Explicit]** Static terrain is a continuous seeded density field composed at
  multiple scales in uncompressed metres. Reefs and rocks are field features, not
  replicated objects. Land occupancy stays sparse enough for open water to remain
  overwhelmingly connected across representative seeds; this is a measured
  statistical outcome rather than a structural guarantee.
- **[Implicit]** Large-scale archipelago suitability uses three octaves of
  24-direction two-dimensional simplex-gradient noise. Its triangular lattice
  gives threshold contours substantially less directional structure than square
  value noise while requiring only three lattice contributions per octave. A
  high sea-level threshold keeps the land excursion set below the regime where
  it commonly divides the sea.
- **[Implicit]** Independent island morphology uses five octaves of the same
  portable simplex-gradient kernel from kilometre to local-coast scale. This
  adds fractal contour detail without the extra field evaluations and increased
  fragmentation of domain warping.
- **[Explicit]** Coastal appearance is the expression of shared latent physical
  fields rather than independent material masks. Broad substrate rockiness and
  coastal ruggedness cross the shoreline: rocky and reef-forming regions drive
  land relief, submerged relief, and outcrops, while low-rock depositional regions
  accumulate broad sand or pebble beaches and low-deposition regions allow
  vegetation to reach the water. Local detail varies each expression without
  changing its regional cause. Material zones span the configured astronomical
  tidal range: low tide exposes wet rock or sediment, and ordinary vegetation
  begins above the permanent intertidal band. The same geology stretches the
  shallow signed landform before its transition to the ocean basin, giving
  rocky, reef-forming, and depositional coasts broader physical shelves while
  low-support steep coasts can remain narrow. Fine signed relief shapes the
  inner reef, intertidal rock, and sparse emergent peaks. A non-overlapping outer
  rocky-shelf band adds portable multi-scale simplex relief that the shared
  ruggedness field transforms toward connected ridges and cuts; depositional
  shelves remain comparatively smooth.
- **[Explicit]** Inland physical relief uses a portable simplex field spanning
  hill to boat scale. The shared broad coastal-ruggedness field continuously
  blends that relief from smooth rolling ground into ridged rock, correlating
  cliff-forming coasts with the terrain behind them without a separate mountain
  mask.
- **[Implicit]** `game` is authoritative for deterministic terrain,
  atmosphere, tide, shelter, and physical depth. Native and WebAssembly runtimes
  sample from match seed, tick, and world position rather than transferring
  sampled fields through the protocol.
- **[Explicit]** The match seed is the only procedural-world datum sent to a
  client. Snapshots synchronize boats, not sampled environment fields or model
  parameters.
- **[Explicit]** Procedural generator implementations remain portable between
  Rust and WGSL even when a field is currently evaluated on only one side. Each
  field may select the noise family and variant appropriate to its phenomenon,
  but uses reproducible integer hashing, coordinate conventions, constants, and
  `f32` operations that the other runtime can mirror. Shared evaluations agree
  within a narrow numeric tolerance and preserve categorical boundaries outside
  that tolerance; bitwise floating-point identity across CPU and WebGPU is not a
  portability requirement.
- **[Explicit]** Primary atmosphere and climate state is distinct from derived
  weather. The seeded surface-air field is a weather anomaly; analytic climate,
  season, and time of day supply its baseline in both Rust and WGSL before
  humidity, instability, clouds, and rain are derived. Upper air is an absolute
  500 hPa temperature generated from a climate baseline with muted seasonality,
  a shared surface air-mass anomaly, and independent upper-level variability;
  it has no diurnal cycle. Derived values have no override path.
- **[Explicit]** Atmosphere uses a scalar spectral pressure potential whose
  analytic gradient drives mostly isobaric surface wind. Broad pressure drives
  upper flow, horizontal temperature gradient contributes thermal wind, and
  vertical shear is derived. Pressure wavelength bands and scalar atmosphere
  layers translate at distinct but correlated seeded velocities, so their
  superposition evolves without stored history; there is no weather grid or
  storm entity.
- **[Explicit]** Rendering samples primary weather at the local boat and applies
  it uniformly across the viewport. WGSL recomputes short derivations rather than
  accepting derived-weather or integrated cloud-offset inputs.
- **[Explicit]** Each procedural cloud layer uses one coherent three-dimensional
  space-time gradient field: horizontal coordinates translate with the layer's
  wind while the time coordinate evolves its shape. The field never rotates.
  Low cloud follows complete unsheltered surface wind, while upper cloud follows
  broad synoptic wind; their seeds and transport remain independent. The visible
  low field is decorative, is capped at one-quarter wisp coverage, may cast local
  shadows, and contributes little to ambient overcast. The invisible upper field
  carries meteorological cloud coverage through broad shadows and lighting.
  Humidity, lift, instability, shear, and rain control cloud family, evolution,
  optical depth, relief, layer height, and shadows. Fine spatial scales evolve
  faster than broad scales. There is no separate formation mask or whole-pattern
  lifetime crossfade.
- **[Explicit]** Surface climate and astronomical tide are stateless analytic
  models. Broad tide response and phase fields represent unresolved basin
  dynamics independently of terrain; bathymetry affects resulting water depth,
  not tide generation. Tide contributes to authoritative and predicted depth as
  well as the rendered shoreline.
- **[Explicit]** Solid ice extends the authoritative solid-surface field used for
  depth and grounding. WGSL mirrors that surface, blends it into land, and
  preserves a sharp water-facing boundary. Persistent polar cold expands this
  surface through progressively deeper water and can close the sea.
- **[Explicit]** Browser terrain is not polygonized. A fullscreen pass evaluates
  seeded elevation at each pixel and zoom level; physics samples depth only at
  positions where boats require it. A single procedural surface normal combines
  the authoritative heightfield gradient with material-specific visual detail
  before celestial lighting. Material relief remains single-sample, without
  parallax or per-material ray marching. Bounded broad-relief samples provide
  cast shadows without displaced geometry.
- **[Implicit]** WGSL mirrors the authoritative terrain and short environment
  derivations. Rust remains authoritative when a rendering approximation differs.
- **[Explicit]** Gameplay and the procedural playground share production shader
  functions. Gameplay binds scene state and fixed production tuning; the lab adds
  live tuning and diagnostic state without adding that interface to production.
- **[Explicit]** Celestial state includes geographic latitude with normalized
  day, season, and moon phases. Rendering-grade astronomy derives light and tide;
  ambient illumination remains separate from spatial direct-light shadows. Cloud
  shadow displacement follows physical layer height and light direction rather
  than a readability-compressed offset.
- **[Explicit]** Playground diagnostics may approximate authoritative shelter but
  never feed derived results back into primary inputs or gameplay physics.

## Transport

- **[Explicit]** The stack uses versioned, Postcard-encoded binary messages over
  WebTransport.
- **[Explicit]** WebTransport is always encrypted. Local development pins a
  short-lived self-signed server certificate through its SHA-256 hash; deployed
  environments provide a browser-trusted certificate.
- **[Implicit]** Network queues are bounded and latest-biased so delayed clients
  cannot accumulate stale commands or snapshots. Commands are sequenced, and an
  acknowledgement covers only input already used by an authoritative tick.
- **[Explicit]** Snapshots are assembled for each recipient because nearby boats
  need prediction data that distant visible boats do not.

## Prediction and interest

- **[Explicit]** Source-style prediction and reconciliation provides responsive
  steering and includes physical interaction between the local boat and nearby
  boats.
- **[Implicit]** The client restores acknowledged authoritative state, replays a
  bounded history of unacknowledged fixed-tick commands, and applies corrections
  to physics immediately while smoothing render pose and velocity.
- **[Explicit]** The nearby physics bubble starts before contact. Entering or
  leaving it, and accepting authoritative corrections, must not visibly snap or
  jerk boats.
- **[Implicit]** Nearby boats share the local prediction world; distant boats use
  bounded kinematic extrapolation.
- **[Explicit]** Boats sufficiently outside the viewport may be omitted from a
  recipient's snapshots, and server lookup uses a maintained spatial query
  system.

## Operations

- **[Explicit]** Server and browser orchestration use asynchronous I/O wherever
  work may suspend.
- **[Explicit]** Visual procedural-world work uses the interactive playground and
  deterministic headless capture rather than a separate native showcase or a
  checked-in image collection.

## Open questions

| Area | Unresolved choice |
| --- | --- |
| Wreck lifecycle | Time-, distance-, capacity-, or salvage-driven cleanup |
| Snapshot format | Whether measured scale eventually justifies batching or application-level deltas |
