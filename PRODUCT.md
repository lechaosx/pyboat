# Product decisions

This record contains choices that constrain the player experience, including
choices not implemented yet. [README.md](README.md) is the sole summary of
current playable behavior; this file does not track delivery status.

| Mark | Meaning |
| --- | --- |
| **[Explicit]** | The user requested or clearly endorsed the choice, including after it was initially implicit |
| **[Implicit]** | An agent selected the choice |

## Experience and scope

- **[Explicit]** The product is an accessible, multiplayer, top-down naval combat game
  with semi-realistic sailing. It adapts the shared-world immediacy of Agar.io and
  the satisfying naval combat of *Assassin's Creed IV: Black Flag*.
- **[Explicit]** The game experience is primary. Helping players learn sailing may
  be a supporting goal, but it does not define the product.
- **[Explicit]** Matches are persistent and accept players while capacity
  remains; they are not self-contained rounds.
- **[Explicit]** Combat includes ramming, cannons, exploding barrels, and other
  weapons. Weapons are mostly automatic, leaving steering and two custom actions
  analogous to Agar.io's boost and split.
- **[Explicit]** The game includes ship classes, customization, progression, and
  metaprogression. Matchmaking accounts for skill, equipment tier, or
  metaprogression.
- **[Explicit]** Advanced AI uses state-driven behavior, understands its ship's
  resources and capabilities, and can perform maneuvers.

## World

- **[Explicit]** The playable world is an infinite cylinder. Its selectable
  width is a wrapped longitude circumference, while its selectable height is the
  finite pole-to-pole geographic span; their numeric defaults are equal. Boats
  can sail longitude laps and interact across that seam.
- **[Explicit]** North-south position is unbounded and does not wrap. Latitude
  mirror-repeats between -90 and +90 degrees at successive poles, following
  `0, +90, 0, -90, 0` as north-south position advances. Climate uses that
  latitude for seasons. Its daily temperature response follows the thermally
  lagged first harmonic of local daylight, which reverses on each descending band
  and vanishes continuously at a pole. Celestial lighting treats each descending
  band as the connected far hemisphere, remaining seamless at the pole without
  transforming terrain or weather coordinates. Every seeded spatial field
  continues unchanged and does not repeat. The camera therefore cannot reveal a
  world edge.
- **[Explicit]** A statistically connected, dominant sea contains natural-looking
  sparse groups of large procedural islands. Land may be more fractal and rare
  enclosed water may occur, but the world must not become a patchwork of equal
  land and water or commonly isolate players from the open sea.
- **[Explicit]** Large islands retain coherent silhouettes while smaller relief
  forms coasts, reefs, navigable channels, and occasional rocks without making
  the parent landforms look noisy or assembled from separate objects.
- **[Explicit]** Coast character varies from beach-dominant to mixed and
  beachless. Gentle shores can form connected beaches, cliffs continue below the
  waterline, and landform-derived shallow shelves give way to deep ocean basins
  with rare trenches. Sediment beds remain smooth except for sparse rock
  outcrops, while rock substrate varies from flat to rugged. Outcrops may remain
  submerged or cross the waterline onto sandy, rocky, or pebbly beaches,
  including beaches over rocky shallows.
- **[Explicit]** Depth affects play: shallow water can slow and ground boats,
  rocks can damage them, small boats commonly find reef passages unavailable to
  larger ships, and spawns begin in safe water. Grounding has static hold and
  lower sliding resistance rather than perpetual creep.
- **[Explicit]** Weather, wind, currents, waves, storms, seasons, and tides are
  procedural and deterministic from match state across space and time.
- **[Explicit]** Ocean circulation includes large local gyres, coastal flows,
  and reversing tidal currents without an explicit uniform prevailing-current
  direction. Coriolis reverses rotational circulation between hemispheres and
  fades near the equator. Integrated world-scale transport remains zero.
- **[Explicit]** Shared waves affect boat-relative water motion. Severe seas can
  produce exceptional crests through constructive interference rather than
  spawning arbitrary rogue-wave events.
- **[Explicit]** Storm-like conditions emerge continuously rather than appearing
  as placed events.
- **[Implicit]** The default world is humid and varied enough that severe rain is
  normally active somewhere at world scale, without guaranteeing or placing a
  storm.
- **[Explicit]** Unsheltered wind can blow in any direction, varies smoothly from
  gust to broad weather system, and weakens in the lee of land over nearby water.
  Latitude climatology and the meridional scale of broad climate-pressure bands
  follow world height, while their zonal scale and all synoptic and local
  pressure-system sizes remain fixed. Broad and synoptic pressure dominate
  pressure anomaly; restrained local pressure is secondary. Procedural gust
  pulses modulate a surface-wind/stability contribution and, during moist
  convection, downward transport of faster upper wind. Broad and synoptic
  systems become zonally coherent at midlatitudes without changing total
  transient wind energy.
  Coriolis reverses pressure-driven turning between hemispheres. Evolving weather
  has no uniform translation direction; system motion is local and its
  world-scale mean is zero.
- **[Explicit]** Sun and moon state drives tides whose height changes depth,
  grounding, navigability, and shoreline position.
- **[Explicit]** Seasonal ground and water temperature, snow, vegetation, and
  solid ice derive analytically without stored per-location history.
- **[Explicit]** Day and night visibly change air and ground temperature upstream
  of humidity, instability, and precipitation. Latitude climate caricatures
  observed daily land-temperature distributions, including their seasonal,
  day/night, and extreme variation.
- **[Explicit]** Snow responds to freezing opportunity, humidity, cold
  precipitation, elevation, and melt independently of sea ice. It can therefore
  reach temperate land beyond the smaller sea-ice cap, while exceptional warmth
  can clear land beside frozen water and cold precipitation can cover land beside
  open water.
- **[Explicit]** Perennial land ice is reserved for colder annual conditions or
  elevation so ordinary polar lowlands still change with season.
- **[Explicit]** Solid ice is immovable land for boats. It blends into exposed
  land but ends at water with a sharp, readable boundary. The edge emerges from
  physical climate and procedural environment rather than an authored latitude
  contour or island-distance field. The pole itself remains fully frozen while
  each hemisphere's cap advances in winter and retreats in summer.

## Presentation

- **[Explicit]** Procedural graphics communicate physical scale and environment,
  including wave lines around islands and moving ships.
- **[Explicit]** Water transitions continuously with depth and distinctly at the
  moving shoreline. Clear shallow water reveals bottom, outcrop, and reef
  structure through depth-attenuated, water-tinted reflectance and relief while
  retaining a cohesive color. Open water has moving, metre-scale surface
  structure driven by shared local significant wave height, wind direction, and
  seeded phase evolution. Its normals respond to diffuse sky light and
  directional celestial light, remain stable at boat zoom, and filter away at
  wider scales. Fine capillary breakup remains appearance-only.
- **[Explicit]** Land has recognizable beach, vegetation, rock, snow, and ice
  materials.
- **[Explicit]** The same authoritative, immovable solid-ice surface covers land
  and polar water rather than using a decorative or independently moving floe
  layer. Frozen water is opaque and has flat, angular plate-scale structure,
  fine relief, and a bedrock-derived shallow and shore response instead of a
  uniform or see-through fill.
- **[Explicit]** Terrain shading reads as relief. Rock exposure responds to slope
  and altitude, beaches span the intertidal zone where geology permits them, and
  ordinary vegetation begins above the permanent tidal range so low tide exposes
  plausible wet rock or sediment. Temperate vegetation may change seasonally
  without making tropical vegetation do the same. Rock, sediment, wet ground,
  grass, forest, and snow compete as continuous covers derived from their
  physical causes. Their ecotones therefore differ without classified zones,
  authored boundary distances, or one shared decorative edge treatment; true
  shoreline and solid-ice phase boundaries remain readable. Relief lighting and
  bounded landform shadows across terrain and water add depth without moving
  shorelines or collision. Material
  detail remains stable across boat-scale zoom rather than aliasing or revealing
  its procedural sampling lattice.
- **[Explicit]** Clouds are weather-shaped visual layers that never conceal
  players or affect gameplay. One cloud-amount scalar drives each layer's body,
  optical depth, and shadows. Clear-sky and weather-canopy ambient light are
  blended per surface pixel by a smooth regional estimate of how much surrounding
  sky is canopy rather than clear. This ground-level sky estimate is neither a
  directly overhead cloud sample nor projected along the Sun or Moon direction;
  celestial state instead determines the light available to the sky dome.
  Visible lower clouds do not contribute to ambient light.
- **[Explicit]** Small visible clouds form sparsely under suitable humid,
  unstable, non-storm conditions at a fixed low altitude. They add local
  shadows, shift with camera-relative geometric parallax, and make system-scale
  wind direction readable across at most about one quarter of the view.
- **[Explicit]** Near sunset, the world's curvature can leave visible lower
  clouds in sunlight after the surface is in shadow. The weather canopy filters
  that grazing light before it produces edge and internal scattering in their
  visible material.
- **[Explicit]** An unseen weather canopy aggregates all cloud types into one
  amount and optical-depth field, from local convective structure to broad
  layered weather. It casts the primary weather shadows and contributes diffuse
  light alongside the clear sky. Directional shadows evaluate weather where the
  Sun or Moon ray intersects the canopy layer. The canopy-light endpoint uses
  the smooth regional mean optical thickness of the cloudy portion of the
  visible sky, rather than the local overhead texture sample, to transform
  clear-sky diffuse light and scatter receiver-local, eclipse-aware Sun and Moon
  light from the canopy's underside. Thin canopy approaches clear-sky ambient,
  moderate canopy can glow warmly at sunset or softly under moonlight, and thick
  canopy attenuates its underside toward storm darkness. The same regional sky
  fraction sets both the clear-sky/cloud-dome ambient blend and the target areal
  coverage of detailed canopy bodies and shadows. It derives from the same
  pressure, temperature, moisture, and resulting weather conditions as gameplay;
  ambient never consumes a sampled cloud pixel.
  Clear-sky light combines solar-disc daylight, lower-atmosphere twilight, and
  weaker upper-atmosphere afterglow. Diffuse moonlight uses the same three
  atmospheric bands, scaled by lunar phase and eclipse, rather than inheriting
  the direct lunar-disc edge. Broad regional scattering modulates these diffuse
  components without importing local weather detail into ambient light.
  Unresolved high ice-cloud effects remain part of canopy and sky diffusion
  rather than a third sampled layer.
- **[Explicit]** Diffuse aerosol optical depth varies only with the broad
  regional condensation and pressure regime, including regional storm washout.
  Local humidity, rain, and visible lower clouds do not imprint fragment-scale
  changes on ambient color. Direct Sun and Moon colors instead retain the
  receiver's local aerosol, humidity, and rain response. At low elevation these
  separate paths can redden direct sunlight and moonlight while regional
  scattering colors clear-sky and canopy diffuse light.
- **[Explicit]** Directional sun and moon light and diffuse sky-and-cloud light
  independently affect terrain, water, and visible lower clouds. Gray overcast
  can remain softly luminous while severe storm cover darkens a noon scene.
  A tilted lunar orbit produces two paired solar/lunar eclipse seasons per year
  from celestial geometry. The penumbra dims moonlight subtly, while atmospheric
  light refracted into the umbra makes a total lunar eclipse dim orange-red.
  Celestial discs retain a physical penumbra, so cloud and landform shadows do
  not become point-source sharp at high zoom.
- **[Explicit]** Rain and snow meet open water in world space. Close views may
  resolve physically small surface contacts. Wider views filter unresolved
  contacts away rather than replacing them with atmospheric haze, enlarged
  drops, or screen-aligned streaks.
- **[Explicit]** The local boat has one apparent-wind windsock whose visible
  length communicates speed. The product does not overlay a world grid or field
  of wind arrows.
- **[Explicit]** The browser HUD reports sailing state, water depth, under-keel
  clearance, and grounding. Zoom is not gameplay-limited, middle-click restores
  its original value, and the world retains a readable physical scale.
- **[Explicit]** Authored sprites remain appropriate for discrete detail such as
  trees, rocks, and wreckage even though broad graphics are procedural.

## Interaction and multiplayer

- **[Explicit]** Keyboard, controller, and touchpad users participate in
  cross-play. Keyboard movement is boat-relative; the first gamepad's left stick
  is world-relative and receives no added software deadzone.
- **[Explicit]** Browser is the primary platform. Native desktop and mobile
  clients are desirable later, and a visible frontend slice takes priority once
  a simulation feature can support one.
- **[Explicit]** Nearby boats appear without intentional interpolation delay.
  Prediction and reconciliation, including entry to and exit from physical
  interaction, must remain visually smooth. The manually accepted steering and
  latency compensation define the responsiveness baseline until contrary
  measurement or playtesting exists.
- **[Explicit]** Directional arrows to ships are diagnostics, not required
  product UI.
- **[Explicit]** A new boat spawns beyond the fleet at a random bearing, points
  with local wind, and starts with safe under-keel clearance.
- **[Explicit]** Collision damage depends on impact and the struck part of the
  hull. Destroyed boats remain physical, uncontrolled wrecks visible to others;
  their owners remain connected and spectate them.
- **[Explicit]** A disconnected browser clearly reports the loss. A living local
  boat remains available for local cruising while visible remote boats become
  wrecks; an already-destroyed local boat continues its wreck scene.

## Procedural playground

- **[Explicit]** One browser laboratory covers weather, landscape, and water with
  the production rendering algorithms. A spacious live preview remains visible
  beside one continuous, vertically scrolling control panel. Time, world, scene,
  and diagnostic controls share one world and tuning state.
- **[Explicit]** Seed, geometry, position, and clocks always generate the world's
  environmental causes. Derived conditions are read-only; playground controls
  tune or enable existing mechanisms without replacing world-generated values.
  Derived readouts use the same independently controlled clocks as the rendered
  scene.
- **[Explicit]** Production tuning is available as a preset, while world state
  has no arbitrary default reset and the initial view shows final composition.
- **[Explicit]** The lab supports drag panning, cursor-anchored zoom, typed values,
  immediate slider dragging, eased track clicks, and independent or locked
  real-time multipliers for cloud, pressure, and current evolution and world time.
- **[Explicit]** The lab exposes independent longitude-circumference and
  pole-to-pole-span sliders, initialized to equal production defaults. The
  view-center sample's raw and cylindrical coordinates and normalized geographic
  coordinates remain visible while panning. Changing either scale regenerates
  the live spatial world.
- **[Explicit]** The global world clock is the sole source of day, season, and
  lunar position. A world day lasts 24 hours, the seasonal year lasts 365 days,
  and the independent lunar month lasts 28 days. Lunar illumination phase is
  inferred from Sun and Moon positions and repeats with the lunar month; the
  combined geometry repeats after 28 years. Production and the lab default to
  real time. The lab starts with cloud, pressure, current, and world-clock rates
  locked together; playback changes rates rather than overriding phases. The lab
  exposes manual solar-year, year-day, and time controls alongside the derived
  lunar month and day. Celestial-event presets move only the lab's world clock to
  naturally derived solar, partial-lunar, and total-lunar eclipse stages without
  overriding orbital state, view, playback, tuning, or diagnostics.
- **[Explicit]** The lab exposes bottom attenuation length and visibility without
  changing physical terrain, depth, or grounding.
- **[Explicit]** Landform lighting follows the astronomical sun or moon angle,
  including flat relief under directly overhead light. The lab exposes relief
  strength, cast-shadow strength, and shadow reach while production retains a
  moderate heightfield-lighting preset.
- **[Explicit]** Scene controls group terminal color contributions by rendered
  receiver, followed by appearance or motion tuning. Groups collapse
  independently, changes are immediate, and one action restores production
  tuning.
- **[Explicit]** Scene toggles expose every color-producing material or
  compositing layer, including the individual land, water, ice, and visible-cloud
  color layers, plus each shadow stage that directly attenuates the composed
  color. Geometry, coverage, normals, relief, opacity, and other values that only
  modulate downstream rendering have no independent output toggle. Hiding a
  layer or shadow stage omits its color effect without changing its inputs.
- **[Explicit]** Lit output is the Cartesian product of two orthogonal toggle
  axes: color-producing material rows and direct Sun, direct Moon, clear-sky
  ambient, or canopy ambient light columns. The UI switches whole rows and
  columns rather than individual products. Hiding every color row or every light
  column produces a blank black output.
- **[Explicit]** Scene tuning exposes the meaningful visual stages behind those
  outputs, including material coverage, generated normals and roughness, water
  and bottom response, ice structure, light balance, cloud shape and lighting,
  and low-cloud and canopy shadows. It does not expose low-level noise-kernel
  constants or replace world-generated physical inputs.
- **[Explicit]** Diagnostic outputs present causal world state before rendered
  consequences: primary atmospheric fields first, derived weather conditions
  second, transport third, and cloud, shadow, lighting, and final-composition
  results afterward. They visualize existing model inputs and derivations rather
  than introducing diagnostic-only world fields.
- **[Explicit]** Shadow-cache diagnostics separately expose per-fragment lookup
  activity, axis-aligned resident tiles, and the aspect-preserved live atlas with
  field-specific colors. They report tile occupancy and atlas resolution and let
  the lab compare a small fixed set of tile sizes. Selecting a cache view activates
  final composition and the cached renderer; production keeps its fixed tile size
  and contains no playground instrumentation.
- **[Explicit]** Intermediate weather and water mechanisms have no user-facing
  switches. The playground enables them when an active color stage or
  selected diagnostic consumes them and disables them when nothing does.
- **[Explicit]** The continuous panel orders time and playback first, world seed
  and geometry second, scene controls third, and output selection and view-center
  diagnostics last.
- **[Explicit]** Pressure diagnostics expose the full anomaly and individually
  scaled broad, synoptic, and local contributions. Separate vector diagnostics
  expose surface gusts, resolved surface wind, broad/synoptic circulation, and
  the local weather-anomaly temperature gradient.

## Open questions

| Area | Unresolved choice |
| --- | --- |
| Wrecks | Cleanup timing and whether salvage affects it |
| Progression | Concrete progression, matchmaking, ship-class, weapon, and custom-action rules |
| Grounding | Recovery, rock-impact damage, draft, spawn-clearance, and friction tuning |
| Navigation | Large-ship drafts and the desired availability of reef passages by class |
| Combat | Health, impact thresholds, damage conversion, and hull-zone balance after playtesting |
