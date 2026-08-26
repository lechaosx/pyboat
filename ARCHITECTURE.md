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
  and control behavior and canonical world-coordinate geometry. `game` owns
  concrete game policy, tuning, control composition, and procedural environment
  sampling shared by native and browser runtimes. Transport serializes the
  canonical geometry type rather than owning a parallel wire representation.
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
- **[Explicit]** Each match has a rectangular `WorldGeometry` with independently
  selectable width and height in metres. It defines canonical wrapped coordinates
  in `[-width/2, width/2) x [-height/2, height/2)` and minimum-image displacement
  independently on each axis. Authoritative simulation, client prediction,
  reconciliation, rendering placement, spatial interest, and replication use
  that shared geometry, so positions remain canonical and interaction crosses
  either seam and their corners. The origin is the equator and reference
  meridian.
- **[Explicit]** Rapier steps finite planar charts of the torus. Each tick uses
  per-hull swept extents in wrapped fixed-size spatial cells and exact
  minimum-image swept proximity to form deterministic contact components, then
  places every complete articulated boat assembly in a common chart. Shared
  simulation supports hull beams of at least 1.5 m and bounds each hull's linear
  speed plus angular extremity speed to 700 m/s, over an order of magnitude above
  the tuned vessel's reachable speed. Non-finite motion becomes rest and
  over-limit linear and angular motion scale together at the shared simulation
  boundary, so authority and prediction apply the same rule. Thirty-two substeps
  therefore resolve opposing supported hull extrema to half the minimum beam
  without relying on dynamic-body CCD. Damage comes only from pair-specific
  solver contacts. The same wrapped index narrows interest queries. Chart changes
  move whole assemblies without changing their canonical logical position; no
  one-sided collision ghosts participate in the solver.
- **[Implicit]** Geometry dimensions use `f32` and accept exactly representable
  values from 8,000 through 256,000 metres on each axis. Exact half-period
  minimum-image ties select the positive half-period on either axis. The
  production default is a 64,000 m square, while server construction accepts any
  validated rectangle.
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

- **[Explicit]** Environment models are locally plausible deterministic,
  stateless queries rather than global fluid, climate, storm-lifecycle, or
  ocean-basin simulations.

### Terrain

- **[Explicit]** Static terrain is a continuous seeded density field composed at
  multiple scales in uncompressed metres. Reefs and rocks are field features, not
  replicated objects. Land occupancy stays sparse enough for open water to remain
  overwhelmingly connected across representative seeds; this is a measured
  statistical outcome rather than a structural guarantee. Rough physical
  phenomena may retain a one-metre wavelength band, while genuinely smooth
  surfaces remain low-frequency; finer structure is appearance-only and cannot
  change depth, collision, or the shoreline.
- **[Explicit]** Large-scale archipelago suitability uses three octaves of
  four-dimensional simplex-gradient noise sampled through metre-scale circle
  embeddings of both world axes. Independent island morphology uses the same
  basis for its first three octaves and periodic two-dimensional triangular
  simplex for its two finer octaves. A high sea-level threshold keeps the land
  excursion set below the regime where it commonly divides the sea.
- **[Explicit]** Coastal appearance is the expression of shared latent physical
  fields rather than independent material masks. Broad substrate rockiness and
  coastal ruggedness cross the shoreline: rocky and reef-forming regions drive
  land relief, submerged relief, and outcrops, while low-rock depositional regions
  accumulate broad sand or pebble beaches and low-deposition regions allow
  vegetation to reach the water. Local detail varies each expression without
  changing its regional cause. Material zones span the configured astronomical
  tidal range: low tide exposes wet rock or sediment, and ordinary vegetation
  begins above the permanent intertidal band. Below the shoreline, a monotonic
  shelf-to-slope transfer of the signed landform field preserves its level sets,
  so shelf geometry follows the coast and its physical width emerges from the
  field gradient before depth reaches the ocean basin. Fine signed relief shapes
  the inner reef, intertidal rock, and sparse emergent peaks. A non-overlapping
  outer rocky-shelf band adds portable multi-scale simplex relief that the shared
  ruggedness field transforms toward connected ridges and cuts; depositional
  shelves remain comparatively smooth.
- **[Explicit]** Inland physical relief uses a portable simplex field spanning
  hill to one-metre scale. The shared broad coastal-ruggedness field continuously
  blends that relief from smooth rolling ground into ridged rock, correlating
  cliff-forming coasts with the terrain behind them without a separate mountain
  mask.
- **[Explicit]** Terrain appearance retains slope, elevation, coastal geology,
  deposition, tidal exposure, vegetation, canopy, and snow as continuous cover
  capacities. Rock exposure is their continuous union; sediment, wet ground,
  and vegetation consume the remaining surface in turn, and snow overlays the
  resulting color, detail gradient, and roughness. No categorical material ID or
  distance from an inferred boundary participates.
- **[Implicit]** Cover response uses the existing hill-to-eight-metre physical
  relief but excludes metre-scale micro-relief, preventing broad contour zoning
  without turning cover into aliased speckle.
- **[Explicit]** Broad coastal substrate and ruggedness use circle-embedded
  four-dimensional simplex. Shoal and finer physical relief use periodic
  two-dimensional triangular simplex.

### Shared procedural fields

- **[Implicit]** `game` is authoritative for deterministic terrain,
  atmosphere, tide, shelter, and physical depth. Native and WebAssembly runtimes
  sample from match seed, tick, and world position rather than transferring
  sampled fields through the protocol.
- **[Explicit]** The match seed and world geometry are the procedural-world data
  sent to a client. Snapshots synchronize boats, not sampled environment fields
  or model parameters.
- **[Explicit]** Every seeded spatial field participating in the toroidal world
  is evaluated periodically across both independently sized axes without a seam
  in its value or gradient. Canonicalizing sample coordinates alone does not make
  an ordinary noise field periodic.
- **[Explicit]** Noise basis is a fixed semantic property of each field and
  octave, independent of world geometry. Broad terrain, macro transported scalar
  weather, and tide response use a four-dimensional circle embedding with radii
  `width / 2π` and `height / 2π`, preserving the requested local metre scale
  without stretching either axis. Fine physical terrain and material spectra
  use periodic two-dimensional triangular simplex. Changing the
  supported geometry range never silently reclassifies an octave; any partition
  change is an explicit, profiled retuning decision.
- **[Explicit]** Procedural generator implementations remain portable between
  Rust and WGSL even when a field is currently evaluated on only one side. Each
  field may select the noise family and variant appropriate to its phenomenon,
  but uses reproducible integer hashing, coordinate conventions, constants, and
  `f32` operations that the other runtime can mirror. Shared evaluations agree
  within a narrow numeric tolerance and preserve categorical boundaries outside
  that tolerance; bitwise floating-point identity across CPU and WebGPU is not a
  portability requirement.
- **[Explicit]** Long-duration transported scalar weather macro octaves and
  broad unthresholded tide response use double-precision circle-embedded
  four-dimensional simplex noise with analytic chain-rule world-space gradients;
  the finest water-vapor octave remains periodic two-dimensional simplex.
  Transported phases reduce on the torus before evaluation, preserving long-time
  precision without repeating time.

### Atmosphere and climate

- **[Explicit]** Primary atmosphere and climate state is distinct from derived
  weather. The seeded surface-air field is a weather anomaly; analytic climate,
  signed latitude, season, and local solar time supply its baseline in both Rust
  and WGSL. Derived values have no override path.
- **[Explicit]** Smooth latitude-knot curves fit observed daily land-temperature
  means, medians, seasonal and diurnal variability, and representative warm and
  cold tail probabilities.
- **[Explicit]** Specific humidity follows climatic saturation capacity plus a
  seeded anomaly.
- **[Explicit]** Absolute 500 hPa temperature has muted seasonality, a shared
  surface air-mass anomaly, independent variability, and no diurnal cycle.
- **[Explicit]** Water uses separate ocean-moderated climatology.
- **[Explicit]** At a fixed place and time, unsheltered wind speed follows a
  Weibull distribution with `k = 2` and `lambda = 9 m/s`.
- **[Explicit]** Wind direction and speed vary continuously from gust to broad
  weather scales.
- **[Explicit]** Deterministic terrain shelter weakens wind in the lee of land
  over nearby water.
- **[Explicit]** Atmosphere uses a scalar spectral pressure potential whose
  analytic gradient drives mostly isobaric surface wind. Broad pressure drives
  upper flow, horizontal temperature gradient contributes thermal wind, and
  vertical shear is derived. Pressure wavelength bands and scalar atmosphere
  layers translate at distinct but correlated seeded velocities, so their
  superposition evolves without stored history; there is no weather grid or
  storm entity.
- **[Implicit]** Pressure directions are quantized to integer wavevectors of the
  rectangular torus so the potential and its analytic gradient join continuously
  on both seams.
- **[Explicit]** Surface rendering derives signed latitude, local solar time,
  climate, transported surface temperature and moisture, snow and solid ice,
  astronomical tide, and celestial lighting at every rendered world position.
  Its procedural inputs are match seed, geometry, global day/season/lunar phase,
  and tick; the view-center Rust sample is diagnostic and never drives the
  surface scene.
- **[Implicit]** The host reduces the tick to bounded toroidal transport offsets
  before upload so WGSL never converts total time to `f32`.
- **[Implicit]** Cloud formation bilinearly samples a fixed 16 by 16 periodic
  lattice of the authoritative analytic pressure field. The host regenerates
  the transient lattice from match state; it is not simulation state or a storm grid.
  This keeps fragment cost and world-size tuning predictable without evaluating
  all 48 spectral pressure modes per pixel. As a temporary cloud-only exception,
  layer winds remain view-sampled inputs to cloud transport. Spatial temperature
  and moisture are evaluated in WGSL rather than accepting center-derived weather.
- **[Explicit]** Each procedural cloud layer is a stateless, match-seeded periodic
  latent field. Adjacent deterministic temporal anchors are
  transported for only one bounded interval by the layer wind and combined with
  a smootherstep-timed orthogonal blend, so any absolute tick is directly
  queryable without accumulated simulation error or midpoint variance loss.
  Low cloud uses complete unsheltered surface wind and upper cloud uses broad
  synoptic wind sampled for the view; player identity does not affect either
  field.
- **[Implicit]** Cloud octaves use circle-embedded four-dimensional simplex noise,
  avoiding the finite-precision phase joins that thresholding can expose in the
  cheaper periodic two-dimensional kernel. The current renderer
  uses one frozen local velocity for each finite flow map, a 30-minute low-layer
  anchor interval, and a 60-minute upper-layer interval. Unresolved octaves fade
  against the fragment's world-space pixel footprint. Spatial transported
  temperature and moisture derive cloud formation at the queried position. A
  continuous positive excess of the latent field over the local formation
  threshold is cloud thickness, while non-positive excess is clear air.
  Low-layer height and relief and both layers' optical depth therefore vary with
  thickness, including in projected shadows. A query derives humidity,
  instability, coverage, rain, and cloud type from one local weather evaluation
  before sampling cloud detail.
- **[Explicit]** Surface climate and astronomical tide are stateless analytic
  models. Broad tide response and phase fields represent unresolved basin
  dynamics independently of terrain; bathymetry affects resulting water depth,
  not tide generation. Tide contributes to authoritative and predicted depth as
  well as the rendered shoreline.
- **[Explicit]** Stateless equilibrium snow exaggerates a temperature-index
  snowpack driven by several-day freezing opportunity, relative humidity, cold
  precipitation, elevation, and above-freezing melt; sea ice is not an input.
- **[Implicit]** Ground temperature attenuates the daily cycle and current air
  anomaly while retaining the seasonal cycle.
- **[Implicit]** Transported absolute water vapor follows seasonal climate, and
  local saturation supplies relative humidity.
- **[Implicit]** Perennial land ice requires a colder annual climate or
  sufficient elevation.
- **[Explicit]** Solid ice extends the authoritative solid-surface field used for
  depth and grounding. WGSL mirrors that surface, blends it into land, and
  preserves a sharp water-facing boundary.
- **[Explicit]** Solid ice derives from climate rather than an authored latitude
  front, island-distance field, or propagation solve. The calibrated polar
  climate remains frozen through summer.
- **[Implicit]** Seasonal water temperature combines with a broad periodic
  maritime heat anomaly derived from an existing landform field, producing a
  small, variable cap independently of water depth.
- **[Explicit]** Frozen water renders the authoritative immovable solid body
  directly. Its opaque material distinguishes plate interiors, fractures,
  shores, and bedrock-controlled shallows without creating independent movable
  floes or a second collision surface.
- **[Implicit]** Plate interiors come from a seeded periodic jittered Voronoi
  field with approximately 192-metre cells and a fixed nine-candidate search.
  Nearest-cell identity supplies a constant plate tilt; the nearest/second-nearest
  gap supplies angular fractures whose visibility follows existing broad ice
  variation. Fragment-footprint filtering removes the network before it aliases.

### Browser rendering and playground

- **[Explicit]** Browser terrain is not polygonized. A fullscreen pass evaluates
  seeded elevation at each pixel and zoom level; physics samples depth only where
  boats require it.
- **[Explicit]** A single procedural surface normal combines the authoritative
  heightfield gradient with material-specific visual detail before celestial
  lighting. Appearance-only material spectra may extend below thirty centimetres.
- **[Implicit]** Appearance-only material spectra use the portable 24-direction
  periodic two-dimensional simplex kernel with non-binary octave spacing; each
  octave chooses integer cycle counts for both world axes and fades out against
  the fragment's world-space pixel footprint before it can alias. Material
  relief remains single-sample, without parallax or per-material ray marching.
  Bounded broad-relief samples provide cast shadows without displaced geometry.
  In clear shallow water, filtered material reflectance produces scalar contrast
  through the water palette according to round-trip optical transmittance.
  Reflective substrates remain visible farther, but every substrate approaches a
  visible bottom signal as water depth approaches zero. Underwater rendering
  adds neither a separate visual-detail height field nor bottom-normal lighting.
- **[Implicit]** Unfrozen water adds an appearance-only normal field from four
  fixed, metre-scale octaves of the same periodic circle-simplex and finite-anchor
  transport used by other moving visual fields. Its seeded direction, octave
  count, refresh horizon, and evaluation cost do not depend on world size. The
  fragment footprint filters unresolved octaves, and the resulting normal and
  low roughness enter the shared diffuse and directional surface-light model.
  This field changes neither authoritative water height nor boat physics.
- **[Explicit]** Visible precipitation contacts the world-space open-water
  surface. Individual contacts retain physical scale and filter out when the
  fragment footprint cannot resolve them; spatial precipitation then reduces
  distant surface contrast through atmospheric haze. No screen-space falling
  streak represents rain.
- **[Implicit]** A fixed 3-by-3 search over periodic 1.5-metre cells renders a
  deterministic representative subset of precipitation contacts. Each cell has
  a jittered impact and 0.75-second lifecycle derived from the bounded low-cloud
  anchor clock. Rain rings expand from 1.8 to 22 centimetres; cold precipitation
  produces smaller melting dimples. The contact path exits before its search in
  dry weather or above a 30-centimetre fragment footprint.
- **[Implicit]** WGSL mirrors the authoritative terrain and short environment
  derivations. Rust remains authoritative when a rendering approximation differs.
- **[Implicit]** Authoritative wind shelter probes the same island-scale landform
  used for long terrain shadows. Metre-scale terrain remains authoritative for
  depth and grounding but does not multiply the cost of every shelter ray or act
  as an aerodynamically significant blocker by itself.
- **[Explicit]** Gameplay and the procedural playground share production shader
  functions. Gameplay binds scene state and fixed production tuning; the lab adds
  live tuning and diagnostic state without adding that interface to production.
- **[Explicit]** Celestial state includes geographic latitude with normalized
  day, season, and moon phases. Rendering-grade astronomy derives light and tide;
  season phase zero is the northern winter solstice while its approximate month
  presentation labels the first interval January.
- **[Explicit]** Direct sun and moon intensity and color follow each source's
  elevation and orientation, while diffuse atmospheric light remains a separate
  RGB contribution. Both terms light the surface and visible lower clouds.
  Atmospheric moisture and cover tint diffuse light; overcast attenuates it
  moderately while rain under deep upper cover supplies stronger storm
  attenuation.
- **[Explicit]** Cloud-shadow displacement follows physical layer height and
  light direction at every zoom. Rays intersect a spherical shell using world
  width as circumference and world height as latitude scale; grazing shadows
  remain finite and fade toward the horizon. The apparent angular radius of the
  sun and moon supplies a physical minimum filter footprint for cloud shadows
  and a distance-proportional penumbra for landform shadows.
- **[Explicit]** Canonical normalized north-south position maps linearly from
  -90 degrees at the lower branch through the equator to the limiting +90-degree
  upper edge. The identified latitude edge is one polar loop, and opposite
  hemisphere seasons may be discontinuous there.
- **[Implicit]** World and view coordinates use positive X for east/right and
  positive Y for north/up. Input adapters convert down-positive device
  coordinates at their boundaries.
- **[Explicit]** Canonical normalized east-west position is longitude. Local
  solar phase is the wrapped sum of global day phase and normalized longitude;
  season and lunar phase remain global.
- **[Explicit]** Playground diagnostics may approximate authoritative shelter but
  never feed derived results back into primary inputs or gameplay physics.
- **[Explicit]** The playground always samples live production state through the
  browser's Rust/WASM boundary and retains manual scenarios as primary-cause
  overrides. The view-center sample is diagnostic; the production shader derives
  each rendered position independently. The overlay marks the raw center,
  reference meridian, equator, and seams. Procedural fields repeat over the
  selected rectangle while seasonal latitude retains the accepted polar-loop
  discontinuity, and raw camera coordinates remain unbounded.
- **[Explicit]** The geometry diagnostic reports each representative octave's
  fixed basis, requested wavelength, effective per-axis wavelength, and relative
  error.
- **[Implicit]** Playground world-width and world-height controls cover 8 km to
  256 km independently in 1 km steps, with the 64 km square match default.

## Transport

- **[Explicit]** The stack uses versioned, Postcard-encoded binary messages over
  WebTransport.
- **[Implicit]** The versioned welcome message carries player identity, match
  seed, and validated world geometry. Server and browser retain the same
  geometry for authoritative and predicted match inputs.
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
  bounded kinematic extrapolation. Extrapolated positions wrap canonically, while
  render placement and correction offsets use the minimum image relative to the
  local boat.
- **[Explicit]** Boats sufficiently outside the viewport may be omitted from a
  recipient's snapshots. Server visibility and physics-bubble membership use
  toroidal minimum-image distance.

## Operations

- **[Explicit]** Server and browser orchestration use asynchronous I/O wherever
  work may suspend.
- **[Explicit]** Visual procedural-world work uses the interactive playground and
  deterministic headless capture rather than a separate native showcase or a
  checked-in image collection.
- **[Explicit]** Headless capture renders the named scenarios with readable
  condition summaries.
- **[Explicit]** Capture requires a hardware Vulkan GPU and has no
  software-rendering path.

## Open questions

| Area | Unresolved choice |
| --- | --- |
| Wreck lifecycle | Time-, distance-, capacity-, or salvage-driven cleanup |
| Snapshot format | Whether measured scale eventually justifies batching or application-level deltas |
