# Product decisions

These choices constrain the intended player experience, including future scope.
See [README.md](README.md) for what is playable now and
[ARCHITECTURE.md](ARCHITECTURE.md) for technical contracts.

**[Explicit]** means requested or clearly endorsed by the user.
**[Implicit]** means selected by an agent; passive acceptance does not change it.

## Experience and scope

- **[Explicit]** An accessible, multiplayer, top-down naval combat game with
  semi-realistic sailing combines Agar.io's shared-world immediacy with the naval
  combat of *Assassin's Creed IV: Black Flag*.
- **[Explicit]** The game experience is primary; learning sailing may be a supporting goal.
- **[Explicit]** Persistent matches accept players while capacity remains.
- **[Explicit]** Combat includes ramming, cannons, exploding barrels, and other
  mostly automatic weapons, leaving steering and two custom actions analogous
  to Agar.io's boost and split.
- **[Explicit]** Ship classes, customization, progression, and metaprogression
  belong in the game. Matchmaking accounts for skill, equipment tier, or metaprogression.
- **[Explicit]** Advanced AI uses state-driven behavior and maneuvers informed by
  its ship's resources and capabilities.
- **[Explicit]** Browser is the primary platform; native desktop and mobile are
  future scope. A visible frontend slice takes priority when a simulation feature supports one.

## World and sailing

- **[Explicit]** The world is an infinite cylinder: boats can sail longitude laps
  and interact across the seam, while north-south travel has no edge. Independent
  longitude circumference and pole-to-pole span start at equal defaults.
- **[Explicit]** Latitude and seasons mirror-repeat through successive poles,
  while terrain and seeded weather continue without repeating. Daylight and
  climate remain continuous through each pole.
- **[Explicit]** A dominant, statistically connected sea contains sparse groups
  of large, coherent procedural islands. Fine coasts, reefs, channels, and rocks
  preserve their silhouettes. Rare enclosed water is acceptable; common isolation
  from open sea and equal land/water patchworks are not.
- **[Explicit]** Coast geology varies from connected gentle beaches to beachless
  cliffs that continue underwater. Smooth sediment, rugged rock, sparse outcrops,
  shallow shelves, deep basins, and rare trenches form a coherent landscape;
  outcrops may cross the shoreline and beaches may cover rocky shallows.
- **[Explicit]** Depth affects sailing and ship-class access. Shallows slow and
  ground boats, rocks can damage them, and small boats commonly reach reef
  passages unavailable to larger ships. Grounding holds at rest with lower
  resistance while sliding; spawning requires safe under-keel clearance.
- **[Explicit]** Weather, wind, currents, waves, storms, seasons, and tides are
  procedural and deterministic across space and time from match state.
- **[Explicit]** Currents include gyres, coastal flow, and reversing tides with
  zero world-scale net transport. Rotational circulation reverses between
  hemispheres and fades near the equator.
- **[Explicit]** Shared waves affect boat-relative water motion. Exceptional
  crests emerge through interference in severe seas.
- **[Explicit]** Storm conditions emerge continuously from weather.
- **[Implicit]** Default humidity and variation normally leave severe rain active
  somewhere at world scale, without guaranteeing a storm.
- **[Explicit]** Wind spans gusts through broad systems, can blow in any
  direction, and weakens in the lee of land. Pressure-driven turning reverses
  between hemispheres; evolving systems have zero world-scale mean motion.
- **[Explicit]** Sun and Moon drive tides that change depth, grounding,
  navigability, and shorelines.
- **[Explicit]** Seasonal temperature, snow, vegetation, and ice derive
  analytically without stored per-location history.
- **[Explicit]** Day/night temperature changes feed humidity, instability, and
  precipitation. Latitude climate caricatures observed daily land-temperature
  distributions, including seasonal, diurnal, and extreme variation.
- **[Explicit]** Snow responds to freezing opportunity, moisture, cold
  precipitation, elevation, and melt independently of sea ice, allowing snow
  beside open water and bare land beside frozen water.
- **[Explicit]** Perennial land ice requires colder annual conditions or
  elevation so ordinary polar lowlands still change seasonally.
- **[Explicit]** Solid ice is one immovable surface across land and polar water.
  Its climate-derived edge is sharp at water and blended into land; the poles
  stay frozen while the caps advance in winter and retreat in summer.

## Readable presentation

- **[Explicit]** A view adapted to clear-sky daylight retains its calibrated
  daytime brightness and material colors.
- **[Explicit]** Modest compensation keeps night dark while boats, coastlines,
  and navigation hazards stay readable; moonlight and storm cover retain their
  visual differences.
- **[Explicit]** Exposure adapts gradually when moving between dark and bright
  environments, like eyes adjusting to light.
- **[Explicit]** Exposure and tone mapping apply to the whole rendered view.
- **[Explicit]** Dark adaptation progressively shifts the rendered view from
  color-sensitive cone vision toward achromatic rod vision, making red dimmer
  relative to blue-green.
- **[Explicit]** Faint night detail has subtle, evolving visual noise.
- **[Explicit]** HDR-capable displays use brighter highlights when the browser
  supports HDR output, with SDR output as the fallback.
- **[Explicit]** Procedural graphics communicate physical scale and environment,
  including waves around islands and moving ships.
- **[Explicit]** Authored sprites are appropriate for discrete detail such as
  trees, rocks, and wreckage.
- **[Explicit]** Water changes continuously with depth and distinctly at the
  moving shoreline. Tinted shallow water reveals bottom relief and reefs;
  moving open-water structure responds to shared waves and wind. Fine detail
  stays stable at boat zoom and filters away at wider scales.
- **[Explicit]** Beach, vegetation, rock, snow, and ice remain recognizable.
- **[Explicit]** Relief shading and landform shadows add depth without moving
  physical shorelines.
- **[Explicit]** Terrain covers follow continuous physical causes, with different
  transitions rather than classified zones or a shared decorative edge. Beaches occupy
  suitable intertidal geology and vegetation starts above permanent tidal reach;
  temperate vegetation can change seasonally while tropical vegetation remains stable.
- **[Explicit]** Frozen water is opaque, with angular plates, fine relief, and
  bedrock-derived shallow and shore response on the authoritative solid surface.
- **[Explicit]** Weather-shaped clouds never conceal players or affect gameplay.
- **[Explicit]** Lower clouds are sparse fractus and scud fragments below the
  camera that add parallax and show system wind.
- **[Explicit]** Local cumulus shadows come from the upper field, where convective
  triggers set cumulus cover.
- **[Explicit]** One unseen upper field holds every cloud above the camera, from
  cumulus to decks, fronts, cirrus, and anvils, and supplies all cloud shadows.
- **[Explicit]** Diffuse light from cloud stays regional. Thin cloud approaches
  clear sky, moderate cover can glow at sunset or under moonlight, and thick
  storm cover darkens the scene while gray overcast can remain softly luminous.
- **[Explicit]** Celestial lighting follows world curvature, allowing lower
  clouds to retain filtered edge and internal sunlight after ground sunset.
- **[Explicit]** Finite Sun and Moon discs keep landform shadows physically soft.
- **[Explicit]** Cloud shadows take their softness from the cloud field.
- **[Explicit]** Sunrise, sunset, moonrise, and moonset change light and
  shadows gradually.
- **[Explicit]** A tilted lunar orbit produces paired solar/lunar eclipse seasons.
  Lunar penumbra subtly dims moonlight; totality becomes dim orange-red from
  atmospheric refraction.
- **[Explicit]** Rain and snow contact open water at physical world-space scale;
  unresolved contacts filter away without enlarged drops, haze, or screen-aligned streaks.
- **[Explicit]** One local apparent-wind windsock indicates speed through visible
  length. Ship-direction arrows are diagnostic UI; world grids and wind-arrow
  fields are outside the product UI.
- **[Explicit]** The browser HUD reports sailing state, depth, under-keel clearance,
  and grounding. Zoom has no gameplay limit and middle-click restores its original value.

## Graphics settings

- **[Explicit]** No graphics setting, whether detail, resolution, or frame rate,
  gives any player an advantage or affects physics or gameplay.
- **[Implicit]** Night and storm darkening, shallow-water hazards, shorelines,
  the ice edge, and boats stay visible and legible at every graphics setting.
- **[Explicit]** Graphics quality tiers trade visual detail for rendering cost,
  from battery-friendly mobile through richer mobile to the best desktop graphics.
- **[Implicit]** Touch screens default to battery-friendly frame rate,
  resolution, and quality tier; other devices default to full quality.

## Controls and multiplayer

- **[Explicit]** Keyboard, controller, and touchpad users cross-play. Keyboard
  movement is boat-relative; the first gamepad's left stick is world-relative
  with no added software deadzone.
- **[Explicit]** Nearby boats appear without intentional interpolation delay.
  Prediction, reconciliation, and entry/exit from physical interaction stay smooth.
  Accepted steering and latency compensation remain the responsiveness baseline
  until contrary measurement or playtesting exists.
- **[Explicit]** Boats spawn beyond the fleet at a random bearing, point with
  local wind, and have safe under-keel clearance.
- **[Explicit]** Collision damage depends on impact and hull region. Destroyed
  boats remain physical uncontrolled wrecks; connected owners spectate them.
- **[Explicit]** Disconnection is clearly reported. A surviving local boat can
  cruise locally, remote boats become wrecks, and an already-destroyed local boat
  continues its wreck scene.

## Procedural playground

- **[Explicit]** One browser lab covers weather, landscape, and water with
  production rendering. A spacious live preview remains beside one continuous
  scrolling panel ordered as time/playback, world, scene, then outputs/diagnostics.
- **[Explicit]** Seed, geometry, position, and the world clock generate
  environmental causes; derived conditions stay read-only. Controls tune or enable
  existing mechanisms and readouts follow the same clock as rendering.
- **[Explicit]** The initial view is final composition. One action restores
  production appearance tuning while world state has no arbitrary default reset.
- **[Explicit]** Navigation supports drag panning and cursor-anchored zoom;
  controls support typed values, immediate slider dragging, and eased track clicks.
- **[Explicit]** Independent world dimensions regenerate the live world. Raw,
  cylindrical, and geographic view-center coordinates remain visible while panning.
- **[Explicit]** Playback starts in real time and runs one world clock, at one
  adjustable rate, for clouds, pressure, currents, and every other procedural
  process. Calendar controls and eclipse presets move the world clock without
  overriding orbital state, view, playback, tuning, or diagnostics.
- **[Explicit]** Collapsible scene controls group colors by the surface being
  rendered, followed by meaningful appearance and motion tuning. Changes take
  effect immediately; physical terrain, depth, and grounding remain authoritative.
- **[Explicit]** Material and light switches work independently; hiding either
  removes its color contribution without changing the underlying world. Sun,
  Moon, clear-sky light, canopy light, and shadow effects stay separately
  inspectable. Hiding all materials or all light gives black output; intermediate
  values have no separate output switches.
- **[Explicit]** Diagnostics follow causes before consequences: primary
  atmosphere, derived weather, transport, then clouds, shadows, lighting, and
  composition. Individual pressure bands and wind mechanisms stay separately
  inspectable without diagnostic-only world fields.
- **[Explicit]** Occluder-grid inspection shows each per-frame occluder grid
  window aspect-preserved with field colors and coverage/resolution readouts, and
  marks any lookup outside its window. A direct reference composition evaluates every
  field per pixel; production contains no playground instrumentation.

## Open questions

| Area | Unresolved choice |
| --- | --- |
| Wreck lifecycle | Cleanup timing and whether time, distance, capacity, or salvage governs it |
| Progression | Progression, matchmaking, ship-class, weapon, and custom-action rules |
| Grounding | Recovery, rock damage, draft, spawn clearance, and friction tuning |
| Navigation | Large-ship drafts and reef-passage availability by class |
| Combat | Health, impact thresholds, damage conversion, and hull-zone balance after playtesting |
| Graphics settings | Where players choose graphics settings, and which presets they see |
