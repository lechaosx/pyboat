# Architecture decisions

These choices constrain implementation. Algorithms, constants, schemas, and
calibration provenance belong beside their code; accepted behavior belongs in
tests. [PRODUCT.md](PRODUCT.md) owns player experience and future scope.
Use [source navigation](MAINTAINERS.md#source-navigation) to find implementation
and verification details.

**[Explicit]** means requested or clearly endorsed by the user.
**[Implicit]** means selected by an agent; passive acceptance does not change it.

## Runtime boundaries

- **[Explicit]** Rust gameplay runs natively on the dedicated server and as
  WebAssembly in the browser, sharing the same simulation.
- **[Implicit]** `simulation` owns runtime-independent physics, control, and
  canonical world geometry. `game` owns game policy, tuning, control composition,
  and procedural sampling. Transport serializes the canonical geometry type.
- **[Explicit]** Rapier 2D supplies native and WebAssembly rigid-body physics.
- **[Explicit]** The browser renderer uses Rust `wgpu` with WebGPU as its initial backend.
- **[Explicit]** Rust/WASM owns transport, decoding, replication, prediction,
  reconciliation, physics, and rendering. TypeScript/HTML/CSS own DOM presentation,
  exchanging commands, discrete events, and a small derived UI view with Rust.
- **[Explicit]** JavaScript tooling runs on Node.js for ecosystem compatibility.
- **[Implicit]** Development follows the latest Node.js LTS line.
- **[Implicit]** npm manages JavaScript dependencies and scripts.
- **[Implicit]** TypeScript tooling tests use Node's built-in runner and native type stripping.
- **[Explicit]** Server and browser orchestration use asynchronous I/O wherever work may suspend.

## Authority and coordinates

- **[Explicit]** The dedicated server is authoritative; clients predict and reconcile.
- **[Explicit]** One validated cylindrical geometry governs simulation,
  prediction, reconciliation, render placement, interest, and replication.
  Longitude wraps around an equatorial reference origin; north-south position is
  unbounded, and minimum-image displacement wraps only longitude.
- **[Explicit]** Climate uses mirror-repeating latitude; celestial geometry uses
  continuous latitude phase to connect each descending band to the far hemisphere.
  Terrain and seeded fields retain the unbounded north-south coordinate. The
  thermally lagged daylight response reverses in descending bands and vanishes
  continuously at poles and in polar night.
- **[Explicit]** Canonical normalized east-west position is longitude. Local
  solar phase adds longitude to global day phase; season and lunar longitude stay global.
- **[Implicit]** Positive X is east/right and positive Y is north/up; input
  adapters convert down-positive device coordinates at their boundaries.
- **[Explicit]** Runtime spatial coordinates remain single precision. Quantized
  boat-scale detail millions of metres from the origin is an accepted limit of
  unbounded travel.
- **[Explicit]** Cylindrical collision handling puts complete articulated boats
  in common planar contact charts selected by deterministic swept proximity.
  Chart changes preserve logical positions; solver contacts alone cause damage.
  Shared motion bounds and substeps protect the supported hull collision domain.
- **[Implicit]** One fixed-step world advances all boats and wrecks. Runtime
  adapters gather work before a synchronous tick and publish afterward;
  simulation never awaits external work.
- **[Implicit]** Project code owns fluid loads, control, game rules, damage, and
  wreck policy. Rapier owns integration, joints, collision response, and spatial queries.
- **[Explicit]** Rendered and collision hull geometry agree.
- **[Explicit]** Destruction removes the controller while retaining bodies,
  momentum, fluid forces, and collisions. The owner spectates the wreck.

## Procedural contracts

- **[Explicit]** Environment models are deterministic, stateless, locally
  plausible queries rather than global climate, fluid, storm-lifecycle, or basin solves.
- **[Implicit]** `game` is authoritative for terrain, atmosphere, tides, shelter,
  and depth; native and WASM sample match seed, tick, and world position.
- **[Explicit]** Clients receive seed and geometry as procedural-world data.
  Snapshots synchronize boats rather than sampled fields or model parameters.
- **[Explicit]** Every seeded spatial field joins in value and gradient across
  longitude and continues without repetition north-south. Coordinate wrapping
  alone cannot make an ordinary noise field periodic.
- **[Explicit]** Noise basis is a fixed semantic choice per field and octave,
  independent of geometry. Broad terrain/tide use longitude-circle embeddings;
  evolving weather adds noise time; fine relief and material detail use periodic
  triangular simplex. Preserve local metre scales when dimensions change;
  changing octave partition requires explicit, profiled retuning.
- **[Explicit]** Generators remain portable between Rust and WGSL through
  reproducible hashing, coordinates, constants, and mirrorable arithmetic.
  Shared queries agree within narrow numeric tolerance and preserve categorical
  boundaries outside it; bitwise CPU/GPU floating-point identity is unnecessary.
- **[Implicit]** WGSL mirrors authoritative terrain and short environment
  derivations; Rust governs when a rendering approximation differs.

## Terrain and physical environment

### Terrain

- **[Explicit]** Terrain is a continuous multiscale seeded field in physical
  metres; reefs and rocks are field features. Open-sea connectivity is measured
  statistically across seeds, not structurally guaranteed. Metre-scale physical
  relief may affect depth and grounding; finer detail is appearance-only.
- **[Explicit]** Land and sea relief, outcrops, and material cover share
  geological causes across the shoreline and into inland terrain.
- **[Explicit]** Terrain materials compete as continuous physical cover
  capacities rather than categorical IDs or inferred boundary distances.
  Vegetation starts above the astronomical intertidal band; snow overlays color,
  detail, and roughness.

### Atmosphere and climate

- **[Explicit]** Primary climate/atmosphere is separate from derived weather.
  Latitude, season, and local solar time provide the baseline; seeded fields
  supply anomalies. Derived values have no override path.
- **[Explicit]** Wind derives from shared atmospheric causes rather than an
  independently generated vector field.
- **[Explicit]** Latitude climatology and meridional climate-pressure scale follow
  world height; zonal climate scale and synoptic/local system sizes stay fixed.
  Midlatitude variance transfer and zonal coherence preserve total transient wind energy.
- **[Explicit]** Authoritative wind shelter samples land near requested positions.
  Production surface rendering uses unsheltered wind; the lab may show approximate shelter.
- **[Implicit]** Shelter and long terrain shadows share island-scale blockers;
  fine depth relief does not multiply each shelter ray's cost.

### Ice and ocean

- **[Explicit]** Tide response models unresolved basin dynamics independently of
  terrain. Bathymetry affects depth, while astronomical tide affects authority,
  prediction, and the rendered shoreline.
- **[Explicit]** Climate-derived ice extends the authoritative solid surface
  used by depth and grounding. WGSL renders that same immovable body with a sharp
  water boundary, and the calibrated polar climate remains frozen through summer.
- **[Explicit]** Ocean motion enters the same water-relative load path on the
  authoritative server and predicting client.
- **[Implicit]** Ocean forcing remains planar: horizontal orbital motion affects
  sailing, while heave, roll, capsize, refraction, and diffraction are outside the model.
- **[Implicit]** Rendering takes ocean wave modes and their clock phases from the
  authoritative Rust modes, computed once per frame in double precision; shaders
  evaluate only position-dependent wave terms.

## Rendering and lighting

### Surface rendering

- **[Explicit]** Lighting and composition use linear color, followed by exposure,
  tone mapping, and display encoding shared by gameplay and the playground.
- **[Explicit]** Bounded exposure applies to the entire view and follows summed
  celestial illumination at its center. Adaptation stays independent of zoom and
  surface color.
- **[Implicit]** Exposure adapts in logarithmic brightness so equal brightness
  ratios have the same response.
- **[Implicit]** Adaptation to darkness is slower than adaptation to bright light.
- **[Explicit]** A fullscreen pass composes terrain at each rendered world
  position, reading broad terrain octaves from the terrain cache, and
  reconstructs environment fields there from a per-frame grid anchored to world
  coordinates, with spacing matched to the fields' finest detail. Physics
  samples where needed; a view-center weather sample never supplies the
  surface scene.
- **[Implicit]** Environment field grids store seeded noise and the terms
  inseparable from it, such as latitude-dependent pressure scaling; analytic
  terms applied afterwards, such as temperature climatology and solar phase,
  stay per pixel.
- **[Implicit]** Per-frame field grids have fixed texel budgets; windows too
  wide for one coarsen its spacing instead of allocating more memory.
- **[Explicit]** One surface normal combines authoritative relief with visual
  material detail before celestial lighting.
- **[Explicit]** Water rendering evaluates current, wave height, and wave slope
  from shared local causes. Fine surface breakup changes appearance alone, evolves
  without field-wide velocity, and has fixed cost independent of world size.

### Celestial and cloud light

- **[Explicit]** Sun and Moon are independent directional sources. Diffuse light
  combines clear sky with regional canopy underlight and illuminates surfaces
  and visible clouds; lower clouds do not supply ambient light.
- **[Explicit]** Ambient light is evaluated per pixel rather than read from
  field grids.
- **[Explicit]** Regional canopy fraction and cloudy-sky optical thickness come
  from broad existing weather causes at the receiver, independently of overhead
  texture and directional shadow samples. That fraction also governs detailed
  canopy coverage. Ambient aerosol/scattering remains regional; direct Sun/Moon
  colors retain local humidity, rain, and aerosol response.
- **[Explicit]** Diffuse solar and lunar light have daylight, lower twilight, and
  weaker upper-afterglow bands. Lunar bands follow phase and eclipse; thin canopy
  approaches clear sky, moderate canopy scatters available celestial light, and
  thick canopy absorbs it. High ice-cloud effects belong in canopy/sky diffusion.
- **[Explicit]** A single world clock drives a 24-hour day, 365-day year, and
  independent 28-day lunar phase cycle, with northern winter solstice at season
  zero. Sun/Moon geometry determines illumination, tides, and eclipses; finite
  discs, observer parallax, an inclined lunar orbit, and a fixed node yield
  altitude-dependent solar overlap and shifting paired eclipse seasons. Combined
  geometry repeats after 28 years.
- **[Explicit]** Cloud and landform shadow projection uses physical altitude,
  spherical curvature, connected celestial direction, and finite-disc penumbrae
  at every zoom. Projected positions retain unbounded cylindrical coordinates;
  receiver-local geometry permits cloud sunlight after surface sunset.
- **[Explicit]** Lower clouds and canopy are continuous global periodic fields.
  Each layer's amount drives its body, optical depth, and shadows.
  Renderer-local integrated view-center wind translates latent texture
  one-to-one while weather stays at world positions; ground shadows stay world-anchored.
- **[Explicit]** At wide views, where per-frame grids coarsen, clouds may soften
  and small clouds may vanish.

### Clocks and renderer lifecycle

- **[Explicit]** Host clocks reduce integer ticks plus fractional presentation
  into bounded shader time coordinates at exactly 60 ticks per second. Simulation
  advances only at integer ticks. Double-precision frame timing and cloud
  displacement survive reconciliation and world/clock changes; uploaded `f32`
  cloud metres accept quantization after prolonged uninterrupted transport.
- **[Implicit]** Animation callbacks keep input, simulation, and presentation
  clocks advancing while GPU submissions are bounded to prevent stale-view
  backlog. The browser owns display synchronization and presentation buffering.
- **[Explicit]** A frame-rate cap skips only rendering; input, simulation, and
  prediction run every display frame.
- **[Explicit]** Lower-cloud bodies and optical depth, canopy optical depth, and
  broad landform elevation are reconstructed from per-frame grids.
- **[Explicit]** Grid spacing follows each field's finest detail and the
  finite-disc penumbra, not the pixel footprint.
- **[Explicit]** Grid windows bound every finite-disc shadow tap from the light
  geometry and every landform sample from the shadow reach, so coverage holds by
  construction and the scene reads these fields only from grids.
- **[Implicit]** Lower-cloud and canopy grids move with their layer's cloud
  transport, so translating latent texture is never resampled; landform grids
  stay fixed to the world.
- **[Explicit]** Resizes take effect on the next frame at the latest dimensions
  by reconfiguring the surface; field textures keep their size.
- **[Explicit]** Canvas backing resolution follows CSS size at the device pixel
  ratio up to a chosen cap, preserving CSS aspect ratio within device texture
  limits.
- **[Explicit]** The scene renders below the canvas backing resolution at a
  chosen scale.
- **[Explicit]** A spatial upscale fills the canvas from the scene.
- **[Implicit]** The upscale uses neither temporal accumulation nor optional GPU
  features, so it runs on mobile WebGPU without history artifacts.
- **[Implicit]** Boats and other gameplay overlays draw at canvas backing
  resolution after the upscale.
- **[Implicit]** Nonrecoverable WebGPU errors stop production animation and close
  transport, clearing queued state; transient surface errors remain recoverable.

### Terrain cache

- **[Explicit]** Static terrain inputs persist between frames in a world-anchored
  cache; a frame refills only texels the view newly exposes or an input change
  invalidates.
- **[Implicit]** The cache holds the circle-embedded octaves of the broad terrain
  noises, from which the scene composes terrain per pixel with the same
  composition as direct evaluation.
- **[Implicit]** Cache levels form a clipmap of camera-centred, toroidally
  addressed windows whose spacing doubles per level.
- **[Implicit]** Each level holds only the octaves with enough texels per
  wavelength to bound its reconstruction error.
- **[Implicit]** The scene reads the finest level whose window holds the view.
- **[Implicit]** Cached terrain places shorelines and shallows within a small
  fraction of a pixel of direct evaluation.
- **[Implicit]** A frame completes the active level and spends a small budget on
  its two neighbours.
- **[Implicit]** No level fills while none holds the view.
- **[Implicit]** Positions without valid cache texels evaluate the noises
  directly.

## Playground boundary

- **[Explicit]** Gameplay and the lab share pure WGSL mechanisms and data types
  through dependency-explicit components, with separate production and live-tuning interfaces.
- **[Explicit]** Production ships only reachable production shader components.
  Feature-gated diagnostics compile lazily by dependency closure; related views
  use overrides and the same field functions as production.
- **[Explicit]** The lab samples live production state through Rust/WASM.
  Diagnostics may approximate shelter but never feed derived state into primary
  inputs or physics. Visibility gates omit composed color/shadow effects while
  retaining operands and downstream calculations.
- **[Explicit]** The lab advances all due fixed ticks and presents the newest
  state, using fractional progress when rendering faster than ticks.
- **[Implicit]** Lab and capture clocks share terminal bounds that preserve
  fixed-tick precision.

## Transport and prediction

- **[Explicit]** Versioned Postcard binary messages travel over encrypted
  WebTransport. Development pins a short-lived certificate hash; deployment
  uses a browser-trusted certificate.
- **[Implicit]** Welcome establishes identity, seed, and validated shared geometry
  before authoritative registration; a failed send does not create a player.
- **[Explicit]** Browser replication waits for Welcome, retaining the newest
  pending snapshot. Environmental render motion starts with the first
  authoritative snapshot; late messages after disconnection cannot restart replication.
- **[Implicit]** Bounded latest-biased queues and concurrent independent stream
  reads prevent stale backlog and head-of-line blocking. Sequenced commands and
  interest updates cannot regress state; acknowledgements cover only tick-applied input.
- **[Implicit]** Failed viewport sends remain retryable until superseded.
- **[Implicit]** Individual stream resets leave later streams usable.
- **[Implicit]** Protocol/version failure closes the session and discards queued replicated state.
- **[Implicit]** Match boat and session capacity is bounded, including wrecks,
  to bound simulation, complete snapshots, and rendering without a distance cap.
  Excess sessions close before registration.
- **[Explicit]** Source-style prediction includes local/nearby boat interaction
  before contact. Bubble transitions and authoritative corrections remain visually smooth.
- **[Implicit]** Reconciliation restores acknowledged authority and replays bounded
  unacknowledged fixed-tick input, correcting physics immediately and smoothing
  render pose/velocity. Nearby boats share prediction; distant boats use bounded
  kinematic extrapolation with minimum-image render displacement.
- **[Explicit]** Snapshots are per recipient because nearby boats need prediction
  data. Viewport half-diagonal plus margin sets interest with no smaller fixed
  world-distance cap; proximity wraps longitude only.

## Visual verification

- **[Explicit]** Procedural visual work uses the interactive playground and
  deterministic headless capture, keeping generated images outside version control.
  Captures include a sidecar derived from the
  same explicit world inputs and fractional render clock; fresh one-frame
  captures have zero renderer-local cloud displacement.
- **[Implicit]** Still captures use settled exposure and SDR output; their sidecar
  records exposure so they can be compared with an adapted interactive view.
- **[Explicit]** Capture requires a hardware Vulkan GPU.
- **[Explicit]** Rendering cost is judged on one fixed set of scenarios shared
  by headless capture and the browser playground.
- **[Explicit]** Performance measurements time the shipped production shaders.
- **[Explicit]** The browser playground runs the scenario bench on devices that
  headless capture cannot reach, such as phones.
- **[Explicit]** A lab-only shader override evaluates normally skipped work
  while selecting the shipped result, bounding worst-case per-pixel cost.
  Production leaves it off.

## Open questions

| Area | Unresolved choice |
| --- | --- |
| Snapshot format | Whether measured scale justifies batching or application-level deltas |
| Convective outflow | Whether rain-cooled cold pools need a mesoscale pressure/temperature/lifetime/gust-front model |
| Terrain cache scope | Whether finer levels near boat zoom should also cache the coarser 2D terrain octaves and material noise |
