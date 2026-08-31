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
- **[Explicit]** Each match has a cylindrical `WorldGeometry` with independently
  selectable longitude circumference and pole-to-pole span in metres. It defines
  canonical coordinates in `[-width/2, width/2) x R`; minimum-image displacement
  wraps only longitude. Latitude is `180 * y / height` inside the geographic span
  and clamps to the exact poles beyond it. Authoritative simulation, client
  prediction, reconciliation, rendering placement, spatial interest, and
  replication use that shared geometry. The origin is the equator and reference
  meridian.
- **[Explicit]** Rapier steps finite planar charts of the cylinder. Each tick uses
  per-hull swept extents in longitude-wrapped fixed-size spatial columns and
  unbounded north-south rows, plus exact
  minimum-image swept proximity to form deterministic contact components, then
  places every complete articulated boat assembly in a common chart. Shared
  simulation supports hull beams of at least 1.5 m and bounds each hull's linear
  speed plus angular extremity speed to 700 m/s, over an order of magnitude above
  the tuned vessel's reachable speed. Non-finite motion becomes rest and
  over-limit linear and angular motion scale together at the shared simulation
  boundary, so authority and prediction apply the same rule. Thirty-two substeps
  therefore resolve opposing supported hull extrema to half the minimum beam
  without relying on dynamic-body CCD. Damage comes only from pair-specific
  solver contacts. The same cylindrical index narrows interest queries. Chart
  changes move whole assemblies without changing their canonical logical
  position; no one-sided collision ghosts participate in the solver.
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
- **[Explicit]** Every seeded spatial field joins without a value or gradient seam
  across the longitude period. Canonicalizing sample coordinates alone does not
  make an ordinary noise field periodic.
- **[Implicit]** Existing terrain, weather, tide, material, and cloud noise retains
  its calibrated north-south procedural period beneath the polar exterior. That
  internal repetition is not coordinate equivalence: physics, visibility,
  latitude, climate, and rendering position never wrap north-south, and opaque
  authoritative ice hides the repeated substrate beyond the poles.
- **[Explicit]** Noise basis is a fixed semantic property of each field and
  octave, independent of world geometry. Broad terrain and tide response use a
  four-dimensional circle embedding with radii `width / 2π` and `height / 2π`,
  preserving the requested local metre scale without stretching either axis.
  Evolving scalar weather uses longitude-circle, linear-latitude, noise-time
  simplex. Fine physical terrain and material spectra use periodic
  two-dimensional triangular simplex. Changing the
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
- **[Explicit]** Broad unthresholded tide response uses double-precision
  circle-embedded four-dimensional simplex noise with analytic chain-rule
  world-space gradients. Evolving scalar weather uses portable single-precision
  four-dimensional simplex with analytic horizontal gradients; the host reduces
  the full tick into bounded time coordinates before shader evaluation.

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
  weather scales. Latitude pressure climatology and the climate-pressure band's
  meridional wavelength scale with world height; its zonal wavelength remains
  fixed at 19.2 km. Synoptic and local pressure retain fixed 3.2 km and 960 m
  wavelengths, while a fixed 256 m field supplies gust occurrence rather than
  resolved pressure. A smooth midlatitude envelope
  transfers variance between broad and synoptic bands without changing total
  transient gradient energy, and stretches those bands zonally while preserving
  an analytic pressure gradient. Band amplitudes, relative variance, anisotropy,
  evolution rate, and gust timing are gameplay calibration rather than scaled
  Earth values. The gust magnitude approximation follows ECMWF's split between
  surface-stress/stability gustiness and convective downward momentum transport:
  sustained surface wind substitutes for unavailable friction velocity, column
  instability substitutes for boundary-layer stability, and moist lift gates
  60% of positive upper-to-surface speed shear.
- **[Explicit]** Deterministic terrain shelter weakens authoritative wind in the
  lee of land over nearby water. Rust evaluates shelter only for requested
  environment positions, including boat positions. Production rendering uses
  unsheltered wind and does not probe terrain across the view; the playground
  retains a debug-only approximate shelter field.
- **[Explicit]** Atmosphere uses a compact match-seeded scalar spectral pressure
  potential whose analytic gradient drives surface wind. Coriolis turns that
  flow in opposite directions between hemispheres and fades smoothly to
  down-gradient flow at the equator. Broad pressure drives upper flow,
  horizontal local weather-anomaly temperature gradient contributes thermal
  wind and frontal lift without turning its latitude-dependent statistical
  amplitude envelope into a physical front, and vertical shear is derived. Each
  pressure band and the gust-occurrence field directly evaluate simplex noise
  over longitude, latitude, and a tick-derived time coordinate. Systems
  evolve and move locally without a field-wide velocity, stored history, weather
  grid, or storm entity. Longitude is circle-embedded for a continuous seam
  while latitude remains linear between the finite poles and does not tile
  vertically.
- **[Explicit]** Surface rendering derives signed latitude, local solar time,
  climate, evolving surface temperature and moisture, snow and solid ice,
  astronomical tide, and celestial lighting at every rendered world position.
  Its procedural inputs are match seed, geometry, global day and season phases,
  lunar longitude, and tick; the view-center Rust sample is diagnostic and never
  drives the surface scene.
- **[Explicit]** The host reduces the full integer tick plus fractional render
  progress into bounded scalar-weather, pressure, and current noise-time
  coordinates before upload. Simulation state advances only on integer ticks;
  faster rendering presents the interval toward the next tick without WGSL
  converting total time to `f32` or interpolating temporal snapshots. A seeded
  phase on each field and octave prevents unrelated spectra from changing in
  lockstep.
- **[Explicit]** Rust and WGSL evaluate the same climate, synoptic, and local
  simplex pressure potential plus a separate scalar gust pulse directly from
  match seed, position, world geometry, and bounded time coordinates. Resolved
  surface wind directs mechanical gusts; moist convection can mix faster upper
  wind downward. Global cloudiness, pressure diagnostics, and rendered wind
  therefore query their canonical causes analytically at world position.
  Spatial temperature and moisture use the same spatiotemporal query model in
  Rust and WGSL rather than accepting center-derived weather. No evolving scalar
  or vector field receives a global spatial transport offset.
- **[Explicit]** Lower clouds and the weather canopy are continuous,
  longitude-periodic global fields. Match seed, global cloud time, and the
  existing spatial atmosphere determine their latent structure, occurrence,
  coverage, and character without tracked bodies, compact kernels, birth
  cohorts, or temporal anchors. Weather is evaluated at the sampled cloud point:
  surface positions supply visible bodies and ambient conditions, while
  directional shadows use each light ray's intersection with the cloud layer.
  Weather remains stationary while the only per-client cloud input, the
  integrated view-center system-wind translation, offsets each layer's latent
  noise. Each latent field normalizes its evolution coordinate by its own base
  wavelength. Changing wind therefore curves cloud texture paths without
  stretching them, instantaneous translation speed matches wind speed one-to-one,
  and advecting texture adopts the stationary weather regime it enters.
- **[Explicit]** The lower field continuously drives visible amount, optical
  depth, analytic body normals, and projected shadows at a fixed 900-metre
  height. Body size, occurrence, detail character, blob shape, contrast, edge,
  normal strength, optical depth, opacity, and shadow darkness remain playground
  tunings. Cloud amount, normals, optical depth, and shadows at a sampled world
  position are independent of camera zoom and pixel footprint; zoom changes only
  which positions are sampled. Scalar shadow queries omit body-gradient work.
- **[Explicit]** The unseen 3600-metre weather canopy uses a broader continuous
  spatiotemporal latent noise field. Its regional sky fraction and mean optical
  thickness are algebraic outputs of the shared weather derivation from
  independent moisture, temperature, and pressure fields. A surface atmosphere
  query retains the broad components needed by the canopy; a projected-shadow
  query samples only those broad components at the cloud-layer intersection.
  Sky fraction sets target coverage while the translated latent texture
  determines individual bodies and openings. Its translation follows the
  integrated view-center upper wind one-to-one, and its continuous optical depth
  supplies connected projected Sun and Moon shadows.
- **[Explicit]** Directional cloud-shadow rendering stores lower-cloud and
  canopy optical depth in distinct logical layers of one frame-local sparse
  world-space tile atlas. A GPU discovery pass requests only projected tiles
  reached by the current viewport and active celestial lights. Tiles have fixed
  texel dimensions while their world-space texel size equals the view's
  world-space pixel footprint, so shadow resolution follows zoom exactly as
  direct fragment evaluation did. Evaluated gutters preserve filtered seams,
  and a missing or capacity-limited tile falls back to the canonical procedural
  query. The atlas has no temporal persistence, does not become simulation
  state, and is not used for visible lower-cloud bodies or ambient sky light.
- **[Explicit]** Rain remains a global weather field queried at actual world
  positions. It does not condition either visual cloud field and does not move
  with a client's cloud translation, so rendered precipitation is independent
  of individual visible cloud bodies and canopy openings.
- **[Explicit]** Ambient sky occlusion is a globally coherent estimate of the
  average cloud fraction and optical thickness of the whole overhead sky dome.
  It derives continuously from the lowest-frequency existing temperature,
  moisture, and pressure conditions and their condensation and lift response,
  rather than from an independent noise field or a sampled canopy pixel. Its
  spatial lookup stays at the receiving surface point and is never projected
  along a Sun or Moon direction; celestial state affects the resulting sky-dome
  radiance, not the estimated canopy fraction. Its spatial variation remains
  materially lower-frequency than the directional canopy, so nearby points can
  lie on opposite sides of a directional cloud shadow while retaining nearly
  identical ambient light. Broad humid low-pressure weather can therefore
  darken ambient light while a local canopy opening still admits direct Sun or
  Moon light.
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

### Ocean dynamics

- **[Explicit]** Authoritative currents include large local gyres, coastal flow,
  and reversing tidal flow without a uniform prevailing vector. Gyre wavelengths
  scale with world circumference, and Coriolis reverses rotational circulation
  between hemispheres with a smooth equatorial fade. World-scale net transport
  remains zero, and local current enters the same water-relative load path on the
  server and predicting client.
- **[Explicit]** A longitude-periodic scalar streamfunction directly evaluates
  seeded simplex noise over longitude, latitude, and a tick-derived time
  coordinate, then combines it with weak evolving pressure coupling. Broad
  landform capacity steers it around island groups, latitude closes it at the
  polar boundary, and a separate scalar potential uses spatial tide response and
  phase gradients. Both vanish where there is no navigable water. This is an
  analytic local approximation, not a stored basin solve. Basin-scale wavelength
  follows world circumference; spectrum, strength, pressure coupling, and
  evolution rate are provisional gameplay values without hydrodynamic
  calibration.
- **[Explicit]** Shared gameplay waves are deterministic from match state and
  local conditions, affect boat-relative water motion, and have no persistent
  drift. Exceptional crests emerge from interference rather than an event flag.
- **[Implicit]** Sixteen fixed-cost modes split into locally wind-weighted wind
  sea and upper-wind-weighted swell. Integer longitudinal wave numbers preserve
  the cylinder seam while continuous north-south wave numbers preserve requested
  local scale. Deep-water dispersion advances phase from every bit of the
  absolute tick; significant height grows with wind and is limited by the
  shallow-water breaking ratio. First-order horizontal orbital velocity enters
  `Environment.true_water_velocity_m_s`; a quadratic bound-wave response
  sharpens severe crests without adding mean horizontal motion. Vertical heave,
  roll, capsize, refraction, and diffraction remain outside the planar physics
  model.
- **[Explicit]** The renderer evaluates seeded current and significant wave
  height directly at each queried world position from the same local causes as
  the authoritative model. Each fragment evaluates a fixed sixteen-mode seeded
  wave spectrum from that height and local wind direction; world-space footprint
  attenuation removes unresolved wavelengths. The final water material uses its
  slope, while the playground exposes separate wave-normal, current, and
  significant-height diagnostics. Fine capillary breakup remains appearance-only.

### Browser rendering and playground

- **[Explicit]** The playground advances its simulation clocks at 60 fixed ticks
  per real second. A render executes every tick due at that moment and presents
  only the newest state, skipping intermediate presentations when rendering is
  slower than the tick rate. Faster rendering uses fractional progress toward
  the next tick.
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
- **[Explicit]** Unfrozen water adds an appearance-only normal field from four
  fixed, metre-scale octaves of longitude-periodic, linear-latitude,
  spatiotemporal simplex. It evolves locally without a field-wide velocity. Its
  octave count and evaluation cost do not depend on world size. The fragment
  footprint filters unresolved octaves, and the resulting normal and low
  roughness enter the shared diffuse and directional surface-light model. This
  field changes neither authoritative water height nor boat physics.
- **[Explicit]** Visible precipitation contacts the world-space open-water
  surface. Individual contacts retain physical scale and filter out when the
  fragment footprint cannot resolve them. Rendering does not replace unresolved
  contacts with atmospheric haze or screen-space falling streaks.
- **[Implicit]** A fixed 3-by-3 search over periodic 1.5-metre cells renders a
  deterministic representative subset of precipitation contacts. Each cell has
  a jittered impact and 0.75-second lifecycle derived from a bounded periodic
  effect clock. Rain rings expand from 1.8 to 22 centimetres; cold precipitation
  produces smaller melting dimples. The contact path exits before its search in
  dry weather or above a 30-centimetre fragment footprint.
- **[Implicit]** WGSL mirrors the authoritative terrain and short environment
  derivations. Rust remains authoritative when a rendering approximation differs.
- **[Implicit]** Authoritative wind shelter probes the same island-scale landform
  used for long terrain shadows. Metre-scale terrain remains authoritative for
  depth and grounding but does not multiply the cost of every shelter ray or act
  as an aerodynamically significant blocker by itself.
- **[Explicit]** Gameplay and the procedural playground compose shared WGSL data
  types and pure rendering mechanisms from dependency-explicit source components.
  Gameplay binds scene state and fixed production tuning; the lab supplies its
  live tuning through a separate interface.
- **[Explicit]** Playground color visibility preserves two orthogonal
  compositional axes: color-producing material layers and direct Sun, direct
  Moon, diffuse clear sky, or diffuse regional-canopy light. A row or column gate
  omits its Cartesian color products without disabling operands or downstream
  computation. Independent shadow gates omit terrain, visible-cloud self, low
  cloud, or canopy attenuation from the composed color without disabling their
  inputs or computation.
- **[Explicit]** The shader source shipped to players contains only the production
  entry points, bindings, and their reachable components. Playground diagnostic
  sources and interfaces are feature-gated from production artifacts and compile
  as separate lazy modules containing only the selected diagnostic layer's
  dependency closure. Related scalar views share diagnostic modules selected by
  pipeline overrides while reading the same primary and derived field functions
  as production rendering.
- **[Explicit]** Celestial state includes geographic latitude with normalized
  day and season phases plus independently advancing lunar longitude.
  Rendering-grade astronomy derives Sun and Moon directions first, then infers
  lunar illumination phase and tidal separation from their geometry. A world
  day lasts 24 hours, a calendar month lasts 28 days, and twelve months form a
  336-day year. The lunar orbital rate produces one Sun-relative phase cycle per
  month. Corresponding dates retain the same phase while seasonal Sun and Moon
  positions vary between months and repeat after one year. Season phase zero is
  the northern winter solstice. The Moon follows a circular orbit inclined
  5.145 degrees to the ecliptic at a fixed node and an Earth-like 60.3 planetary
  radii. Observer parallax and finite Sun/Moon discs derive local solar overlap;
  finite umbra and penumbra discs derive lunar eclipses. The fixed node produces
  two paired eclipse seasons, including total lunar eclipses, while preserving
  exact annual repetition.
- **[Explicit]** Sun and Moon remain independent directional light sources whose
  direct contributions use their respective direction, intensity, color, and
  cloud and terrain transmittance. Ambient light separately combines a clear-sky
  dome, blue by day, with a cloud-dome source weighted by broad overhead cloud
  fraction. Both dome sources scatter the same Sun and Moon source colors used
  for direct light; increasing cloud optical thickness reduces cloud-dome
  radiance toward darkness. Both direct and ambient terms light the surface and
  visible lower clouds.
  Solar overlap attenuates direct and diffuse daylight. Lunar penumbra attenuates
  moonlight slightly; increasing umbral coverage replaces blocked direct
  sunlight with dim orange-red light filtered through the atmosphere, and the
  resulting moon color enters direct and diffuse surface and cloud lighting.
  Uneclipsed moonlight independently reddens near the horizon according to the
  observer's local aerosol and moisture scattering.
- **[Explicit]** Cloud-shadow displacement follows physical layer height and
  light direction at every zoom. Rays intersect a spherical shell using world
  width as circumference and the clamped physical latitude derived from the
  pole-to-pole span; the non-geographic polar apron retains its local planar
  offset. Sun and Moon projections are evaluated independently and fade with
  their respective direct-light contribution at the horizon. Grazing shadows
  remain finite. The apparent angular radius of the sun and moon supplies a
  distance-proportional penumbra for landform shadows.
- **[Explicit]** North-south position maps linearly from -90 degrees at the
  finite southern pole through the equator to +90 degrees at the finite northern
  pole. Latitude remains clamped beyond those coordinates, so neither climate
  nor seasonal forcing has a north-south seam.
- **[Implicit]** World and view coordinates use positive X for east/right and
  positive Y for north/up. Input adapters convert down-positive device
  coordinates at their boundaries.
- **[Explicit]** Canonical normalized east-west position is longitude. Local
  solar phase is the wrapped sum of global day phase and normalized longitude;
  season phase and lunar longitude remain global.
- **[Explicit]** Playground diagnostics may approximate authoritative shelter but
  never feed derived results back into primary inputs or gameplay physics.
- **[Explicit]** The playground always samples live production state through the
  browser's Rust/WASM boundary. World controls set seed, geometry, and clock
  rates; model and shader controls only tune or enable mechanisms already present
  in that world. The view-center sample supplies the per-client visual cloud
  fields and their winds; other procedural surface state remains independently
  derived at each rendered position. The overlay marks the raw center, repeated
  reference meridians and longitude seams, the equator, and both finite poles.
  Raw camera coordinates remain unbounded north-south.
- **[Implicit]** Playground longitude-circumference and pole-to-pole-span controls
  cover 8 km to 256 km independently in 1 km steps, with equal 64 km defaults.

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
  bounded kinematic extrapolation. Extrapolated longitude wraps canonically,
  while render placement and correction offsets use cylindrical minimum-image
  displacement relative to the local boat.
- **[Explicit]** Boats sufficiently outside the viewport may be omitted from a
  recipient's snapshots. Server visibility and physics-bubble membership use
  longitude minimum-image displacement and ordinary north-south distance.

## Operations

- **[Explicit]** Server and browser orchestration use asynchronous I/O wherever
  work may suspend.
- **[Explicit]** Visual procedural-world work uses the interactive playground and
  deterministic headless capture rather than a separate native showcase or a
  checked-in image collection.
- **[Explicit]** Headless capture renders one deterministic procedural world from
  explicit seed, geometry, camera, cloud-time, and world-time inputs and writes a
  derived-condition summary beside it.
- **[Explicit]** Capture requires a hardware Vulkan GPU and has no
  software-rendering path.

## Open questions

| Area | Unresolved choice |
| --- | --- |
| Wreck lifecycle | Time-, distance-, capacity-, or salvage-driven cleanup |
| Snapshot format | Whether measured scale eventually justifies batching or application-level deltas |
| Convective outflow | Whether rain-cooled cold pools need their own mesoscale pressure, temperature, lifetime, and radial gust-front mechanism |
