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

## Hosting

- **[Explicit]** GitHub Pages hosts the game frontend.
- **[Explicit]** This repository deploys one backend on the Raspberry Pi.
  Other people may independently host servers and optional frontends.
- **[Explicit]** The backend address is a configurable hostname or IPv4 address
  covered by a matching browser-trusted certificate.
- **[Explicit]** A minimal NixOS host supplies generic rootless Podman and SSH
  deployment access. Application services and certificate renewal belong to
  this repository and deploy independently of the host configuration.
- **[Explicit]** Podman Compose invokes Podman directly through `podman-compose`,
  without a container API socket.
- **[Implicit]** Deployment selects an image by digest from GHCR and retains it
  locally for subsequent starts, so booting never selects a newer release.
- **[Implicit]** The ARM64 container shares host networking for direct UDP
  ingress. WebTransport terminates TLS in the game server.
- **[Implicit]** The frontend selects its backend address at build time so
  static hosting and game transport can use independent origins.
- **[Explicit]** Trusted certificate issuance and renewal are automatic.
- **[Explicit]** acme.sh manages certificates in a separate container. The game
  polls exported certificate contents and adopts valid replacements without
  restarting.
- **[Implicit]** Let's Encrypt runs on the backend host so private keys remain
  there throughout issuance and renewal.
- **[Explicit]** Frontend pushes to `master` deploy automatically and independently
  of backend releases. Graphics and UI updates preserve multiplayer
  compatibility with supported servers.
- **[Explicit]** Backend deployment requires a manual pipeline action that
  acknowledges discarding the game in progress.
- **[Explicit]** Certificate renewal preserves the running match and connections.
- **[Explicit]** Backend updates, host configuration updates, service
  restarts, and reboots remain deliberate.

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
- **[Implicit]** Project code owns intent, actuator effort limits, fluid loads,
  game rules, damage, and wreck policy. Rapier owns integration, joint actuation,
  collision response, and spatial queries.
- **[Explicit]** Rendered and collision hull geometry agree.
- **[Explicit]** Destruction removes the controller while retaining bodies,
  momentum, fluid forces, and collisions. The owner spectates the wreck.

## Procedural contracts

- **[Explicit]** Environment models are deterministic, stateless, locally
  plausible queries rather than global climate, fluid, storm-lifecycle, or basin solves.
- **[Explicit]** The world's physical models follow the structure of the
  governing physics, with its dependencies and published constants, so they
  largely calibrate themselves. Each aims for the most realism per complexity.
- **[Explicit]** Departures from that physics are deliberate compromises,
  acceptable where the world lacks the physical cause or its caricatured
  proportions make the physics read wrongly. Each is named beside its code with
  what it stands in for.
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
- **[Explicit]** Seeded noise and cell hashing that WGSL mirrors uses 32-bit
  integer arithmetic, which WGSL supports natively.
- **[Implicit]** Seeded fields mix both words of the 64-bit match seed into
  their keys, so worlds stay distinct across the full seed range.
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
- **[Explicit]** Each part of the view adapts to the light falling on the
  scene around it, pooled over the eye's adapting field: sunlight and
  moonlight through cloud, storm, and landform shadows at their incidence,
  clear-sky and canopy light, and the night sky. The eye sees each surface's
  reflectance against that light, so coasts, snow beside water, and boats
  leave no halos; the compromise ignores the glare of bright surfaces such as
  snow, to which real adaptation also responds. Full dark adaptation under a
  clear moonless sky bounds it.
- **[Explicit]** Adaptation stays independent of surface color. Its field is a
  fixed visual angle of the view, so a wider view pools more of the world, as
  an eye looking from higher would: small and medium cloud shadows keep their
  contrast while storms, large decks, and the terminator partly adapt.
- **[Implicit]** The view subtends the same visual angle on every device, that
  of a full-HD desktop display at arm's length.
- **[Implicit]** Adaptation is retinal: each part of the screen adapts
  gradually to the adapting field there and keeps its state in place as the
  view pans, so a part panned from daylight into night starts dark while a
  part still in daylight keeps its look.
- **[Implicit]** The adapting field pools the mean light over its extent, as
  CIE defines adapting luminance and as light scattered within the eye adds,
  so a field half in daylight adapts to about half of daylight.
- **[Implicit]** Adaptation over time runs in logarithmic brightness so equal
  brightness ratios take the same time.
- **[Implicit]** Adaptation to darkness is slower than adaptation to bright light.
- **[Implicit]** The eye adapts to the color of the light falling on the scene,
  pooled over the same adapting field as its brightness, and never to surface
  colors.
- **[Implicit]** Chromatic adaptation follows CIECAM16: its cone gains carry
  the adapting light's color toward the display's white by the degree of
  adaptation that the adapting luminance sets.
- **[Implicit]** Chromatic adaptation takes about a minute, as measured for
  color appearance, rather than the seconds of brightness adaptation.
- **[Explicit]** Scene light keeps the physical ratios of daylight, twilight,
  moonlight, and starlight, and the eye alone compresses them: an adapted
  part of the view has the brightness of CIECAM02's photopic brightness law at
  every light level. That compromise keeps moonlit and moonless views readable
  on a display, where real dim vision would leave them nearly black.
- **[Explicit]** The display shows the brightness and color the eye model
  perceives as display brightness and color, so what the adapted eye sees as
  neutral shows as the display's white, alike for SDR and HDR below diffuse
  white; HDR only adds headroom above it. The viewer's own viewing conditions are unknown and stay
  outside the model.
- **[Explicit]** Unlit overlays, such as boats and the windsock, take the
  light the view is adapted to where they are.
- **[Explicit]** A fullscreen pass composes terrain and materials at each
  rendered world position and reconstructs environment fields there from a
  per-frame grid anchored to world coordinates, with spacing matched to the
  fields' finest detail. Physics samples where needed; a view-center weather
  sample never supplies the surface scene.
- **[Implicit]** The scene's environment grid covers the view alone.
- **[Implicit]** Cloud shadow texels read what the weather under each cloud
  sets for it from a coarser cloud weather grid, whose window covers every such
  read by construction.
- **[Implicit]** Ground-sky exchange reads its regional conditions from the
  cloud weather grid, whose window also covers the view.
- **[Implicit]** The scene's environment grid stores seeded noise and the terms
  inseparable from it, such as latitude-dependent pressure scaling; analytic
  terms applied afterwards, such as temperature climatology and solar phase,
  stay per pixel.
- **[Implicit]** Per-frame field grids have fixed texel budgets; windows too
  wide for one coarsen its spacing along each overflowing axis instead of
  allocating more memory.
- **[Implicit]** Each occluder layer also has a total texel budget; a layer
  over it coarsens its finer axis first.
- **[Implicit]** An occluder layer keeps its previous spacing until a finer one
  fits well within its budgets, so a window crossing a threshold while the
  camera pans changes lattice once.
- **[Explicit]** One surface normal combines authoritative relief with visual
  material detail before celestial lighting.
- **[Explicit]** Water rendering evaluates current, wave height, and wave slope
  from shared local causes. Fine surface breakup changes appearance alone, evolves
  without field-wide velocity, and has fixed cost independent of world size.

### Celestial and cloud light

- **[Explicit]** Sun and Moon are independent directional sources. Diffuse light
  combines clear sky, regional canopy underlight, and the light ground and sky
  exchange, and illuminates surfaces and visible clouds; lower clouds do not
  supply ambient light.
- **[Explicit]** Ambient light is evaluated per pixel rather than read from
  field grids.
- **[Explicit]** Regional canopy fraction and cloudy-sky optical thickness come
  from broad existing weather causes at the receiver, independently of overhead
  texture and directional shadow samples. Taken under each deck, the regional
  fraction governs only deck and overcast cover; cumulus cover follows local
  convective triggers at each cloud's own position. Ambient aerosol/scattering
  remains regional; direct Sun/Moon colors retain local humidity, rain, and
  aerosol response.
- **[Explicit]** Clear-sky diffuse light fades continuously through twilight
  as the planet's shadow rises through the air above each receiver, for the
  Sun and the Moon alike. The air's scale height keeps Earth's ratio to the
  planet's radius, so twilight spans the same depression of the light on every
  world size.
- **[Implicit]** Light scattered more than once into the planet's shadow keeps
  a fainter twilight after the shadow has risen through most of the air.
- **[Explicit]** Lunar diffuse light follows the Moon's phase and eclipse.
- **[Implicit]** A solar eclipse dims clear-sky light with the uncovered share
  of the Sun's disc, as it dims the beam, down to the light measured under
  totality.
- **[Implicit]** The eclipsed Moon reflects sunlight that the spectral
  clear-sky model reddens along twice the horizontal path, at the luminance
  of the Moon's own colour.
- **[Explicit]** Thin canopy approaches clear sky, moderate canopy scatters
  available celestial light, and thick canopy reflects most of it back to space.
- **[Explicit]** High ice cloud's diffuse effects belong in canopy and sky
  diffusion; its direct shadow comes from the upper field.
- **[Implicit]** Clear-sky light on level ground follows a physical clear-sky
  model of each light's height, and its split into direct and diffuse light
  also follows the regional turbidity. Moonlight is sunlight scaled by the full
  Moon's ratio to sunlight, the Moon's phase law, and its eclipse, and splits
  the same way.
- **[Implicit]** Clear-sky light's amount and colour follow the light's height
  above the receiver's local horizontal, as on Earth, whose horizon dips little
  at hill and cloud heights. Its direct part on level ground is each beam's
  light on a surface facing it, projected on that horizontal; the rest
  arrives diffuse.
- **[Implicit]** Each beam reaches a receiver as on Earth. Below the receiver's
  local horizontal it crosses the air along its grazing path, so it dims and
  reddens continuously until its disc sets behind the horizon the receiver's
  height has on Earth, lighting only surfaces tilted toward it.
- **[Implicit]** Land relief takes each beam by Lambert's law and the sky's
  light by an isotropic sky's view factor. Landform-scale relief is
  exaggerated threefold for the depth cues a top-down view lacks; metre-scale
  micro relief stays at true scale. Water keeps its own relief grade and glint.
- **[Implicit]** A pixel takes the mean relief light of the land it covers:
  the relief its footprint resolves sets the mean slope, and finer terrain and
  material relief spreads slopes around it. Authoritative elevation and its
  gradient stay unfiltered; only lighting takes the split.
- **[Implicit]** A receiver has one horizon for its atmosphere, the one its
  height has on Earth: each light's attenuation and visibility, its shadows'
  sampled direction, and the canopy's interception of its beam follow it. The
  small world's lower horizon bounds only shadow reach and grid windows.
- **[Implicit]** Sunlight above the atmosphere has the Sun's physical colour,
  a 5778 K blackbody's, and moonlight and the night sky take their colours from
  it.
- **[Implicit]** A spectral clear-sky model at three wavelengths gives each
  beam both its brightness and its colour: the luminance it passes sets the
  brightness, so a low light's invisible near-infrared power does not count,
  and its colour, scaled to the luminance of the source's own colour, sets hue.
- **[Implicit]** The clear sky's diffuse light takes its brightness from a
  broadband clear-sky model and its colour from the spectral one.
- **[Implicit]** The canopy's mean optical thickness is a physical cloud
  optical depth, from thin decks through storm cover, and every canopy light
  term, deck shadow, and ground-sky exchange reads that one depth.
- **[Implicit]** The canopy is a non-absorbing two-stream layer. It passes the
  clear sky's diffuse light and each beam's scattered part down and reflects
  ground light back up; each beam's unscattered part reaches the ground
  through the upper field's deck shadows alone.
- **[Implicit]** Clear-sky light reaches the canopy's level face split as on
  level ground: its direct part, which vanishes as the light
  reaches the layer's level, lights the layer from above, and the rest arrives
  diffuse.
- **[Implicit]** A light below the canopy's level lights the layer's underside
  with the beam that reaches the layer's height, which the layer reflects down.
  That beam has crossed the canopy's level once already on the far side, where
  the regional canopy's cover and depth stop it.
- **[Implicit]** Cloud layers keep real absolute heights on the small world.
- **[Implicit]** Light that ground and sky exchange follows regional means, so
  it lifts every shadow in a region alike.
- **[Implicit]** Regional albedo comes from broad ground causes, smooth over
  the kilometres a cloud base averages.
- **[Implicit]** The sky's reflectance from below follows the cover and optical
  depth of the clouds that cast shadows.
- **[Explicit]** A single world clock drives a 24-hour day, 365-day year, and
  independent 28-day lunar phase cycle, with northern winter solstice at season
  zero. Sun/Moon geometry determines illumination, tides, and eclipses; finite
  discs, observer parallax, an inclined lunar orbit, and a fixed node yield
  altitude-dependent solar overlap and shifting paired eclipse seasons. Combined
  geometry repeats after 28 years.
- **[Explicit]** Cloud and landform shadow projection uses physical altitude,
  spherical curvature, and connected celestial direction at every zoom.
  Projected positions retain unbounded cylindrical coordinates; receiver-local
  geometry permits cloud sunlight after surface sunset.
- **[Explicit]** Landform shadows have finite-disc penumbrae at every zoom.
- **[Implicit]** Landform shadows read one per-frame horizon grid per light,
  covering the view, whose texels march the landform grid toward that light.
- **[Implicit]** A horizon texel keeps the receiver heights between which the
  landform's cover of the disc goes from none to all, across every blocker, so
  the cover changes continuously as blockers overtake each other; each pixel
  compares its own height with them.
- **[Explicit]** Cloud shadows sample each light once per pixel; their softness
  comes from the cloud field.
- **[Explicit]** Each cloud type's octaves fade out at the width of the light's
  disc at the slant distance to that type, so the field holds no detail the
  disc would blur away.
- **[Explicit]** Cloud shadow rims span at least the disc's penumbra.
- **[Implicit]** The Moon's cloud shadows fade out at each receiver where the
  Moon supplies less than a just-noticeable share of the receiver's clear-sky
  light. The Moon's cloud shadow grid is not evaluated while that share stays
  negligible everywhere in view.
- **[Explicit]** Every shadow type removes the same share of direct light for the
  same occlusion: a landform blocking the whole disc is as dark as the densest cloud.
- **[Explicit]** Under a low light, horizon dimming and reddening of direct
  light alone reduce shadow contrast, alike for every shadow type.
- **[Explicit]** All shadow types follow the light as far down as it reaches
  each receiver; their reach comes from the world's curvature, so it stays
  bounded on any world size.
- **[Implicit]** Where the Moon's cloud shadows fade out, its direct light keeps
  the region's mean share of the beam that cloud passes, so cover stays about as
  dark as where the shadows show.
- **[Implicit]** Raised land, whose horizon lies below the sea-level one, keeps
  its shadows after sea-level sunset. Shadow windows bound receivers up to the
  highest landform the elevation scale allows.
- **[Explicit]** Landform shadow reach grows as the light lowers.
- **[Implicit]** Below 45°, the landform march reaches where the ray toward the
  light from the lowest receiver that sees it has climbed the high-light reach
  above sea level. Each horizon texel takes the reach from its own light, so
  landform shadows stay world-anchored while the camera pans.
- **[Implicit]** The march steps from a sixteenth of the high-light reach out to
  the march's reach, with steps that widen in proportion to distance beyond a
  fixed shortest step.
- **[Explicit]** Upper-field shadows follow one darkness curve over cloud bodies
  and on the ground.
- **[Implicit]** That curve is extinction along each cloud type's slant path
  through its own height.
- **[Explicit]** The upper field is a set of cloud types, each a continuous
  global periodic field at its own height. Renderer-local integrated view-center
  wind translates each type's latent texture one-to-one, cumulus with the low
  wind, decks with the upper wind, and cirrus faster, while weather stays at
  world positions; ground shadows stay world-anchored.
- **[Explicit]** Cumulus evolve over real cell lifetimes, independently of the
  wind.
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
- **[Explicit]** Each light's cloud shadow and broad landform elevation are
  reconstructed from per-frame grids; each light's cloud shadow comes from one
  grid on a plane above every shadow receiver.
- **[Explicit]** Grid spacing follows each field's finest detail, not the pixel
  footprint; landform grids also resolve the finite-disc penumbra.
- **[Explicit]** Grid windows bound every finite-disc shadow tap from the light
  geometry and every landform sample from the shadow reach, so coverage holds by
  construction and the scene reads these fields only from grids.
- **[Implicit]** Cloud shadow grids move with the cumulus transport, so cumulus
  texture is never resampled, while decks and cirrus are resampled on that
  lattice; landform grids stay fixed to the world.
- **[Implicit]** Each cloud shadow grid's shortest resolved wavelength changes
  smoothly with zoom.
- **[Explicit]** Resizes take effect on the next frame at the latest dimensions
  by reconfiguring the surface; per-frame field grid textures keep their size.
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
- **[Implicit]** A graphics quality tier is a set of override constants on the
  scene pipeline, so lower tiers carry no runtime branches and changing tier
  rebuilds that pipeline.
- **[Implicit]** Nonrecoverable WebGPU errors stop production animation and close
  transport, clearing queued state; transient surface errors remain recoverable.

### Terrain cache

- **[Explicit]** Static terrain inputs persist between frames in a world-anchored
  cache; a frame refills only texels the view newly exposes or an input change
  invalidates.
- **[Implicit]** The cache holds the circle-embedded octaves of the broad terrain
  noises, from which the surface bake and direct evaluation compose terrain.
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

### Baked surface

- **[Explicit]** The static surface persists between frames in a world-anchored
  clipmap: terrain with land ice, the dry, snowless, unseasoned land
  composition with the factors its wetness and seasons change, and the sea-ice
  plate noises and relief.
- **[Explicit]** The scene applies per pixel only what changes with time,
  season, or weather: tide, sea ice, snow, autumn and dormancy colour, rain,
  waves, and all lighting, clouds, and shadows.
- **[Implicit]** Sea-ice slabs and fractures, whose visibility follows the
  pixel footprint, are evaluated per pixel.
- **[Implicit]** The bake keeps the land ice's own colour and relief beside the
  composition, so snow covers only the land share of partial land ice without
  evaluating ice per pixel.
- **[Implicit]** Texels with land ice hold its detail in the channels of the
  autumn colour change, which the far colder annual climate of land ice rules
  out.
- **[Implicit]** A frame bakes only texels the view newly exposes or an input
  change invalidates.
- **[Explicit]** Level spacings form one fixed geometric ladder whose ratio is a
  single constant, so another spacing needs no other change.
- **[Explicit]** Bake memory follows the scene size at about one texel per
  pixel, within a cap that the device class and its reported memory set; above
  the cap the bake coarsens.
- **[Implicit]** The clipmap keeps two levels at about one texel per pixel: the
  finest rung whose camera-centred window holds the view, and the next coarser
  one as standby.
- **[Implicit]** The scene reads one level at a time, without blending between
  levels.
- **[Implicit]** The bake fades material octaves too short for a level's
  lattice.
- **[Implicit]** The bake reads broad terrain octaves from the terrain cache.
- **[Implicit]** A view no level holds, as after a teleport, first bakes a
  level of a quarter of the density and reads it until the full level holds
  the view.
- **[Explicit]** A zoom in reads the coarser level until the finer one holds
  the view; any level that holds the view serves while the read level bakes.
- **[Implicit]** Each frame's bake is bounded by a share of a level and by a
  fixed texel count.
- **[Implicit]** The standby bakes paced to the next rung change the footprint's
  rate of change predicts, so a gentle zoom spreads it over many frames.
- **[Implicit]** Bake textures that run out of memory are allocated again
  with half the memory; once the smallest allocation fails, every position
  evaluates directly.
- **[Explicit]** Baked shorelines need no close match with direct evaluation;
  tests guard only against gross breakage.
- **[Explicit]** Lab tuning rebakes the surface.
- **[Implicit]** The bake is the same at every graphics tier; tiers gate only
  detail evaluated per pixel.
- **[Implicit]** A rung change through the pixel footprint alone, with the view
  unchanged, keeps the finer level in place of a new standby, so a resolution
  controller stepping across a rung does not rebake.
- **[Implicit]** Bake textures follow a resize once the new size has held for a
  short settle period, then rebake from the view; until then the existing
  textures serve, with direct evaluation wherever no level holds the view.
- **[Implicit]** Positions without valid baked texels compose the surface
  directly.
- **[Implicit]** Open water too deep for waves or bottom tint to reach its bed
  reads no bake.
- **[Implicit]** The bake holds sea-ice detail only where the coldest season
  freezes the water.

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
- **[Implicit]** The eye's adaptation is downstream of the lab's light and
  layer gates, which leave it unchanged. Lab tuning, the cloud and landform
  shadow switches included, changes the light the eye adapts to.
- **[Explicit]** The lab advances all due fixed ticks and presents the newest
  state, using fractional progress when rendering faster than ticks.
- **[Implicit]** Lab and capture clocks share terminal bounds that preserve
  fixed-tick precision.

## Transport and prediction

- **[Explicit]** The target multiplayer compatibility rule is
  `server.major == frontend.major && server.minor <= frontend.minor`, ignoring
  patch versions, including for major version zero. Frontend releases preserve
  compatible message decoding, world generation, and prediction for every
  supported server minor.
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
- **[Implicit]** The protocol version also gates shared simulation compatibility,
  including the world generated from a match seed and collision geometry, so
  clients predict the same physical world as their server.
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
- **[Implicit]** Still captures use settled exposure and, unless asked for an
  HDR output peak, SDR output; their sidecar records the exposure at the view's
  center so they can be compared with an adapted interactive view.
- **[Explicit]** Capture requires a hardware Vulkan GPU.
- **[Explicit]** Still-view rendering cost is judged on one fixed set of
  scenarios shared by headless capture and the browser playground.
- **[Explicit]** Rendering cost during camera motion is judged on fixed camera
  paths that only headless capture runs, as shipped.
- **[Explicit]** Performance measurements time the shipped production shaders.
- **[Explicit]** The browser playground runs the scenario bench on devices that
  headless capture cannot reach, such as phones.
- **[Explicit]** A lab-only shader override takes the game's own cache and bake
  paths but evaluates every content branch they normally skip, bounding the
  worst-case per-pixel cost of play. Production leaves it off.

## Open questions

| Area | Unresolved choice |
| --- | --- |
| Snapshot format | Whether measured scale justifies batching or application-level deltas |
| Convective outflow | Whether rain-cooled cold pools need a mesoscale pressure/temperature/lifetime/gust-front model |
| Shadow consistency | Whether the consistent shadow look holds up by eye against the lab's legacy shadow look before release |
| Level switches | Whether switching baked levels needs blending, judged by eye |
| Gameplay zoom | Whether gameplay zoom anchors at a level's sharp end, and whether rung thresholds need hysteresis |
| Edge smoothing | Whether shorelines and the sea-ice edge get smoothing as a look option |
| Tidal wet band | How the wet band follows flat tidal coasts |
| Thick cumulus rims | How the sharpest thick-cumulus rims, about 0.9× the disc's penumbra even when evaluated per pixel, come to span it |
| Forced bench | How to bring the forced bound down: the per-frame bake budget and the cost of the forced water, sea-ice, and cloud branches |
| Stratus calibration | Whether thick stratus matches measured transmittance, given how uncertain the conversion from measured cloud thickness to optical depth is |
| Grazing deck light | Whether decks need sunlight entering through their breaks and edges while the Sun is near the canopy's level, where a plane-parallel layer passes almost none |
