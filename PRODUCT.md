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
  maps linearly from the equator to exact finite poles and remains clamped at
  -90 or +90 degrees beyond them. The camera therefore cannot reveal a world
  edge, while the indefinitely continuing authoritative polar ice prevents
  boats from travelling through the non-geographic exterior.
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
  Weather-system scale remains proportional to world circumference, and Coriolis
  reverses pressure-driven turning between hemispheres. Evolving weather has no
  uniform translation direction; system motion is local and its world-scale mean
  is zero.
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
  bounded shadows add depth without moving shorelines or collision. Material
  detail remains stable across boat-scale zoom rather than aliasing or revealing
  its procedural sampling lattice.
- **[Explicit]** Clouds are weather-shaped visual layers that never conceal
  players or affect gameplay. One cloud-amount scalar drives each layer's body,
  optical depth, and shadows; regional canopy coverage determines its diffuse
  share of ambient light.
- **[Explicit]** Small visible clouds form sparsely under suitable humid,
  unstable, non-storm conditions at a fixed low altitude. They add local
  shadows and make system-scale wind direction readable across at most about one
  quarter of the view.
- **[Explicit]** An unseen weather canopy represents broad convective and layered
  clouds through varying amount and optical depth. It casts the primary weather
  shadows and contributes diffuse light alongside the clear sky. Unresolved
  high ice-cloud effects remain part of canopy and sky diffusion rather than a
  third sampled layer.
- **[Explicit]** Aerosol optical depth varies spatially with seeded regional
  structure, dry air, and rain washout. When the sun is low, aerosol and moisture
  can redden direct sunlight, visible clouds, and the canopy's diffuse light.
- **[Explicit]** Directional sun and moon light and diffuse sky-and-cloud light
  independently affect terrain, water, and visible lower clouds. Gray overcast
  can remain softly luminous while severe storm cover darkens a noon scene.
  Celestial discs retain a physical penumbra, so cloud and landform shadows do
  not become point-source sharp at high zoom.
- **[Explicit]** Rain and snow meet open water in world space. Close views may
  resolve physically small surface contacts, while wider views replace
  unresolved precipitation with atmospheric haze rather than enlarged drops or
  screen-aligned streaks.
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
  the production rendering algorithms. It separates world inputs, procedural
  model tuning, shader tuning, and diagnostics into semantic domains.
- **[Explicit]** Seed, geometry, position, and clocks always generate the world's
  environmental causes. Derived conditions are read-only; playground controls
  tune or enable existing mechanisms without replacing world-generated values.
- **[Explicit]** Production tuning is available as a preset, while world state
  has no arbitrary default reset and the initial view shows final composition.
- **[Explicit]** The lab supports drag panning, cursor-anchored zoom, typed values,
  immediate slider dragging, eased track clicks, and independent or locked
  real-time multipliers for cloud, pressure, and current evolution and world time.
- **[Explicit]** The lab exposes independent longitude-circumference and
  pole-to-pole-span sliders, initialized to equal production defaults. A red
  cross marks the view-center sample, whose raw and cylindrical coordinates,
  and normalized geographic coordinates remain visible while panning. A guide
  repeats the reference meridian and longitude seams and marks the equator and
  finite poles. Changing either scale regenerates the live spatial world.
- **[Explicit]** The global world clock is the sole source of day, season, and
  lunar phase. Production and the lab default to real time. The lab starts with
  cloud, pressure, current, and world-clock rates locked together; playback
  changes rates rather than overriding phases.
- **[Explicit]** The lab exposes bottom attenuation length and visibility without
  changing physical terrain, depth, or grounding.
- **[Explicit]** Landform lighting follows the astronomical sun or moon angle,
  including flat relief under directly overhead light. The lab exposes relief
  strength, cast-shadow strength, and shadow reach while production retains a
  moderate heightfield-lighting preset.
- **[Explicit]** World geometry, playback, and the global clock remain immediately
  accessible. Production-derived spatial measurements are read-only; visibility,
  tuning, and mechanism diagnostics use collapsed semantic groups.

## Open questions

| Area | Unresolved choice |
| --- | --- |
| Wrecks | Cleanup timing and whether salvage affects it |
| Progression | Concrete progression, matchmaking, ship-class, weapon, and custom-action rules |
| Grounding | Recovery, rock-impact damage, draft, spawn-clearance, and friction tuning |
| Navigation | Large-ship drafts and the desired availability of reef passages by class |
| Combat | Health, impact thresholds, damage conversion, and hull-zone balance after playtesting |
