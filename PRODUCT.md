# Product decisions

This file records intended player experience, scope, and product behavior.

- **[Explicit]** means the user directly requested or affirmatively confirmed the
  decision.
- **[Implicit]** means an agent selected the decision. Later acceptance of the
  resulting behavior does not make that underlying choice explicit.

## Core experience

- **[Explicit]** The product is a multiplayer, top-down naval combat game with the
  accessibility and shared-world appeal of Agar.io and semi-realistic sailing.
- **[Explicit]** The target experience preserves the enjoyable naval-combat
  qualities of *Assassin's Creed IV: Black Flag* while adapting them to a top-down
  multiplayer game.
- **[Explicit]** Matches are persistent rather than round-based. Players may join
  an existing match at any time while it has capacity.
- **[Explicit]** A newly connected boat spawns at a random bearing just beyond
  the radial extent of every other player on the map, with its bow pointing in
  the local wind direction.
- **[Explicit]** Combat should include ramming, cannons, exploding barrels, and
  other weapons.
- **[Explicit]** Weapons should mostly operate automatically. In addition to
  steering, the player should have only two custom actions, analogous to Agar.io's
  boost and split controls.

## Boats, progression, and matchmaking

- **[Explicit]** The product includes multiple boat and ship classes,
  customization, progression, and metaprogression.
- **[Explicit]** Matchmaking should account for skill, equipment tiers, or
  metaprogression.
- **[Explicit]** Advanced AI should use state-driven behavior, perform maneuvers,
  and understand the resources and capabilities of its ship.

## World and presentation

- **[Explicit]** Seas and islands are procedurally generated.
- **[Explicit]** Vast seas dominate the world, with huge islands occurring in
  sparse clusters rather than land and water occupying roughly equal areas.
- **[Implicit]** Broad representative terrain contains 5–15% land, each sampled
  65 km region remains sea-dominated, and major island shapes remain coherent
  across at least 512 m.
- **[Explicit]** Terrain generation takes inspiration from Minecraft's
  multi-scale density composition: broad island fields combine with progressively
  smaller fields that shape coasts, reefs, and rocks.
- **[Explicit]** Procedural terrain uses real metres horizontally and vertically;
  generation does not compress real-world depths or feature sizes.
- **[Explicit]** A continent-scale density layer must not create closed regional
  seas that isolate players. Sparse island clusters sit in a connected dominant
  ocean instead.
- **[Explicit]** Reefs and rocks are features of the continuous depth field rather
  than separately placed objects. Their probability distribution naturally forms
  navigable channels; small boats can commonly use them as cover, while deep
  passages for large ships occur rarely.
- **[Explicit]** Large-island silhouettes remain coherent rather than inheriting
  reef-scale noise. Small-scale relief stays predominantly underwater, with only
  sparse peaks emerging as coastal rocks.
- **[Explicit]** Coast geology varies by island and along long island sectors.
  Some islands have no beach, some combine cliff and beach coasts, and others
  remain beach-dominant.
- **[Explicit]** Cliffs continue below the waterline, and rare deep trenches
  occur in the same continuous terrain field.
- **[Explicit]** Shallow depths remain around coasts, reefs, lagoons, and shelf
  water. The connected water between island clusters deepens into ordinary
  ocean basins measured in hundreds to thousands of metres rather than remaining
  a roughly 30 m-deep shelf.
- **[Explicit]** The production terrain uses `3×` base elevation relief so most
  coasts reach navigable water within hundreds of metres rather than remaining
  shallow for roughly a kilometre. Broad lagoons may still occur naturally.
- **[Explicit]** Cliff terrain emerges from the same landform density that shapes
  an island rather than from a separate cliff-biome mask.
- **[Explicit]** Gentle shoreline renders as beach, while sufficiently steep
  shoreline and inland slopes render as bare rock.
- **[Explicit]** Procedural water depth affects boats physically: shallow water
  slows a dragging boat proportionally, beaches can ground it, and rocks can
  damage it.
- **[Explicit]** Grounding resistance resembles physical static and sliding
  friction: a grounded boat does not creep under a steady force within its hold,
  while a sufficiently large force can break it free.
- **[Explicit]** New boats spawn with enough water depth to avoid beginning
  grounded.
- **[Explicit]** Weather, currents, waves, wind, and storms should be procedural
  but deterministic from a match seed across both space and time.
- **[Explicit]** Weather models only the local atmosphere needed by an observer
  moving through spacetime, without a global fluid simulation or explicit storm
  objects and life cycles. Storm-like conditions emerge when the continuous
  atmospheric fields align.
- **[Explicit]** Pressure, climate and upper-level temperature, current
  temperature anomaly, absolute moisture, aerosol optical depth, solar latitude,
  world day/season phases, moon phase, local tidal response, and terrain forcing
  are primary conditions. These describe a fictional local environment rather
  than a literal Earth location or calendar.
  Current low-level temperature follows seasonal and daily cycles. Sun and moon
  position and illumination, wind at both levels, relative humidity, instability,
  lift, cloud potential, rain, gustiness, and wind shear
  are derived outcomes and cannot be overridden directly.
- **[Explicit]** Vertical wind shear is the difference between derived upper and
  surface wind. Horizontal temperature gradients contribute a thermal-wind
  change aloft, while horizontal shear emerges from spatial pressure variation.
- **[Implicit]** The first implemented procedural-environment slice gives each
  Rust match seeded wind that changes smoothly across the world and over time.
  Water current remains still until a later environment slice.
- **[Explicit]** Seasonal terrain and water need only remain locally plausible,
  deterministic, and smooth through space and time. They do not require a global
  climate simulation or stored per-location history. Analytic thermal lag and
  annual accumulation-versus-melt approximations derive ground and water
  temperature, seasonal snow, sea ice, glaciers, and vegetation season.
- **[Explicit]** Moon and sun position derive a stateless astronomical tide.
  Moon phase changes spring/neap range but does not drive weather. Tidal water
  height changes authoritative depth, grounding, navigability, and the rendered
  shoreline; local range and phase response need not form a globally correct
  ocean basin.
- **[Explicit]** The browser renders water depth continuously, with a distinct
  transition at the shoreline.
- **[Explicit]** The browser HUD reports water depth, under-keel clearance, and
  how strongly the hull is dragging on the bottom.
- **[Implicit]** The browser renders its static seeded elevation field against
  the current tidal water height, with a continuous shallow-to-deep gradient and
  a discrete moving shoreline transition.
- **[Explicit]** Land uses distinct beach, grass, forest, rock, seasonal snow,
  and glacier materials; sufficiently cold water forms broken sea ice. Temperate
  vegetation can turn ochre and orange while tropical vegetation remains driven
  primarily by moisture. Material boundaries remain sharp except for the blend
  between grass and trees; color variation stays within each material.
- **[Explicit]** Rock exposure increases with altitude, allowing high mountains
  to remain rocky on gentler ground. Mountain rock detail follows the same
  height field as physical terrain and must read as relief rather than isolines.
- **[Explicit]** Where a beach occurs, it begins at the shoreline and continues
  inland with a procedurally varying width. Steep rock may instead meet the
  water directly.
- **[Implicit]** Trees are currently represented by a clustered forest canopy
  material rather than individual geometry.
- **[Implicit]** The current dinghy has a 0.6 m effective draft. Lost bottom
  clearance transfers its weight progressively onto the seabed; effective static
  and kinetic friction coefficients are 1.0 and 0.6. Spawns require at least
  1.5 m of water.
- **[Explicit]** Unsheltered wind may blow in every direction. At any fixed place
  and time, its speed follows a Weibull distribution with shape `k = 2` and scale
  `λ = 9 m/s`, allowing near-calms and a decaying tail of increasingly rare strong
  wind.
- **[Explicit]** Wind changes smoothly in space and on every simulation tick and
  includes short-scale gusts.
- **[Explicit]** Wind forms spatially coherent air masses and avoids rapid visible
  oscillation over time.
- **[Explicit]** Wind has a recognizable prevailing direction around an island
  roughly 2 km across, except near a calm. Across the very large world it varies
  smoothly among broad weather systems and has no universal direction.
- **[Explicit]** Near land, a deliberately simplified surface-wind model captures
  the essential lee shelter over the sea. It remains deterministic from the match
  seed so every client computes it without transferring sampled wind.
- **[Implicit]** The current model preserves the unsheltered wind direction while
  upwind terrain reduces its speed. Strong shelter may remove at most 70% of the
  local speed and recovers over several hundred metres.
- **[Explicit]** The browser has no world grid or field of wind arrows. Exactly
  one apparent-wind windsock is attached to the bow of the local boat.
- **[Explicit]** At lower apparent-wind speeds, only the inflated part of the
  windsock is rendered rather than showing the entire compressed sock.
- **[Implicit]** The red-and-white tapered windsock points down local apparent
  airflow. Each of its five bands represents 3 knots, so it is fully extended at
  15 knots; the current final band extends continuously between thresholds.
- **[Explicit]** The mouse wheel zooms the world view in and out without an
  artificial gameplay limit. Clicking the mouse wheel resets the original zoom.
- **[Explicit]** The world view provides a persistent sense of physical scale.
- **[Implicit]** A lower-left segmented graphic scale, styled like the scale bars
  on architectural drawings, selects readable 1–2–5 distances and changes between
  metric units as zoom changes.
- **[Implicit]** Terrain remains visible when zoomed beyond the server's boat
  interest limit, but boats outside that limit are absent.
- **[Explicit]** Graphics should be mostly procedural, including wave lines around
  islands and moving ships and clouds that correspond to weather.
- **[Explicit]** Global weather, including wind and rain intensity, is the
  same for every player and is deterministic from match seed, position, and time.
- **[Explicit]** Visible clouds are a noise-based presentation of that weather,
  not globally shared geometry. Players at the same place and time see similar
  cover and storminess, while fine cloud shapes may differ per player.
- **[Explicit]** Rendering uses the weather sampled at the local boat for the
  whole viewport. Extreme debug zoom therefore shows one uniform weather regime
  across the visible map.
- **[Explicit]** The camera is compositionally below one invisible upper-cloud
  layer and above one visible low-cloud layer. Upper clouds appear only through
  their shadows; visible low clouds provide the detailed weather and wind cue.
- **[Explicit]** Visible low clouds move with the complete unsheltered surface
  wind. The invisible upper layer moves only with the lowest-frequency synoptic
  wind. Both layers also form and dissipate over time without simulating a global
  fluid volume.
- **[Explicit]** The visible layer combines one evolving isotropic, non-rotating,
  unscaled detail field with a lower-frequency formation mask. Detail character,
  contrast, and blob formation may change without transforming its coordinates.
  The mask controls broad shape, rotation, scale, elongation, and sparsity.
- **[Explicit]** Cloud advection can be continuously split between detail and
  formation mask. Moving only the mask makes cloud form at its leading edge and
  dissipate at its trailing edge; moving only detail preserves a stationary
  formation envelope.
- **[Explicit]** The detail field may remain continuous inside mask-defined cloud
  bodies, form disconnected bodies through a gate derived from its own values, or
  combine both mechanisms. The detail gate does not introduce another sampled
  noise field.
- **[Explicit]** Visible clouds range from wisps and small cumulus to occasional
  large bodies, but substantial clear space remains in ordinary cloudy weather.
  Maximum visible coverage is tunable and initially targets 50% statistically
  across the procedural field rather than clamping each viewport.
- **[Explicit]** Storms primarily appear through broad upper-cloud shadows and
  increased visible low scud rather than a screen-covering visible cloud deck.
- **[Explicit]** Ordinary clouds are sparse and mostly translucent, with fine
  internal density and depth variation. Long decks are uncommon; dense dark
  opacity is concentrated in rare storm conditions so the boat usually remains
  readable.
- **[Explicit]** Both cloud layers cast translucent shadows projected from their
  physical formation heights and the current sun or moon direction. Higher clouds
  and lower light elevations produce longer displacement without an artificial
  readability compression.
- **[Explicit]** Upper-cloud shadows also attenuate direct sun or moonlight on the visible
  low clouds while leaving ambient illumination, so low clouds remain readable
  beneath broad upper cover.
- **[Explicit]** Cloud cover and rain also attenuate and cool ambient illumination;
  weather therefore changes the whole scene mood independently of local direct-light
  shadows.
- **[Explicit]** Low sun does not imply an orange scene by itself. Aerosol optical
  depth and moist haze determine scattering strength, while upper-cloud openness
  determines whether warm light reaches the scene. When those conditions align,
  lit cloud tops, terrain, and water become visibly orange or red. Cloud shadows
  remove that warm direct component while retaining cooler ambient fill rather
  than applying one uniform color filter to the viewport.
- **[Explicit]** Visible low clouds have a procedural height field. It shapes
  their local directional shading and the projection of upper-cloud shadows onto
  their tops so cloud bodies do not read as flat textures.
- **[Explicit]** Clouds do not conceal players or otherwise affect gameplay.
  Authored sprites remain appropriate for details such as rocks, trees, and
  wreckage.

## Platforms and controls

- **[Explicit]** One general procedural playground covers generated weather,
  landscape, and water systems. Each domain separates semantic layers,
  world conditions, model tuning, shader tuning, and diagnostic views rather
  than presenting one undifferentiated list of controls.
- **[Explicit]** Weather diagnostics can independently display and enable cloud
  layers, detail and formation fields, masks, optical properties, projected
  shadows, derived surface/upper wind and shear, primary gradients, and
  approximate terrain shelter. Derived atmospheric values are read-only;
  scenarios configure their primary causes. Landscape
  diagnostics likewise expose terrain composition fields and layers.
- **[Explicit]** Independently meaningful rendered contributions have enable
  switches separate from their magnitude tuning. Master switches preserve and
  visually disable their dependent controls; cloud layers, celestial lighting, wind
  components, shelter, and landscape composition follow this hierarchy.
- **[Explicit]** Playground tuning defaults match production game defaults, while
  the initial world conditions are illustrative rather than canonical. The
  initial diagnostic output is always the final composed scene.
- **[Explicit]** The procedural playground supports drag panning and cursor-
  anchored mouse-wheel zoom. Cloud animation and world time have independent
  rates, so cloud advection and texture evolution can continue while time of day
  is frozen. Primary weather fields stay fixed until changed manually or by a
  scenario.
- **[Explicit]** Cloud-evolution and world-clock speeds use the same real-time
  multiplier: `2×` advances either clock twice as fast as real time. Their speeds
  can be locked together or adjusted independently.
- **[Explicit]** The playground uses the same architectural 1–2–5 metric scale
  indicator as gameplay. Absolute camera position and cloud-animation time are
  not linear sliders; navigation uses drag and wheel, while normalized day,
  season, and moon phases are world controls.
- **[Implicit]** View height is likewise controlled only through wheel zoom rather
  than an impractically wide linear slider.
- **[Explicit]** Live world inputs have no reset-to-default action because no one
  world state is canonical. Semantic categories may instead provide presets.
  Weather Conditions exposes scenarios that set every primary atmosphere,
  climate, location, time, and moon world input while preserving shader and model
  tuning, debug state, camera, seed,
  playback, and zoom. Non-world categories initially provide a Production preset
  that restores the corresponding in-game tuning as a group. Clicking a slider
  track eases to the selected value, while dragging a slider remains immediate.
  Every slider also has a directly editable numeric value without spinner arrows.
- **[Explicit]** Weather scenarios are characteristic regimes rather than generic
  intensity steps: clear high pressure, fair cumulus, marine stratus, warm-front
  rain, a cold-front squall, tropical convection, and post-front clearing each
  configure realistic primary causes and retain a distinct visual mood. Dawn mist,
  golden-hour cumulus, winter twilight stratus, clear full moon, a moonlit squall,
  high-latitude summer midnight, polar freeze, and temperate autumn also exercise
  day, season, moonlight, snow, ice, glaciers, and vegetation moods.
- **[Explicit]** The playground has a deterministic headless capture mode that is
  also practical for humans. It renders chosen scenarios through the same shader
  and emits images plus readable derived-condition and image summaries for visual
  comparison and tuning.
- **[Explicit]** Keyboard, controller, and touchpad input participate in cross-play.
- **[Explicit]** Browser support is the primary platform target. Native desktop
  and mobile clients are desirable secondary targets rather than requirements for
  the first frontend.
- **[Explicit]** When the existing simulation can support a meaningful visual,
  establish a visible frontend slice before continuing deeper into gameplay
  mechanics.
- **[Explicit]** The browser slice includes one locally controlled boat from the
  Rust simulation. `W/S` controls boat-relative forward or reverse power, `A/D`
  turns, and the first connected gamepad's left stick controls world-relative
  movement without an added software deadzone.
- **[Implicit]** The browser connects to the Rust authoritative server, sends
  fixed-tick controls, predicts nearby physical interaction, reconciles local
  input, and extrapolates distant visible boats without intentional render delay.
- **[Explicit]** Boats move smoothly when authoritative snapshots arrive and when
  they enter or leave the nearby physics bubble.
- **[Explicit]** The browser presents the player with a server-down notice when
  its connection to the authoritative server ends.
- **[Explicit]** After losing the server connection, a living player's browser
  continues local cruising and presents every other visible boat as a wreck. If
  the local boat is already a wreck, the resulting scene presents every boat as
  dead.
- **[Implicit]** The browser HUD displays speed through water, apparent-wind
  speed, and point of sail from predicted and reconciled local state as immediate
  sailing feedback.
- **[Explicit]** The browser implementation language is not committed to pure
  JavaScript.
- **[Explicit]** The current steering and latency-compensated interaction were
  manually confirmed by the user as feeling great and form the accepted
  responsiveness baseline.

## Multiplayer awareness

- **[Explicit]** Players should see other nearby boats without intentional
  interpolation delay that would make aiming harder.
- **[Explicit]** Directional arrows to other ships are debug-only feedback and may
  be omitted from the browser and product UI.
- **[Implicit]** Wrecks and recently damaged boats use distinct colors, and the
  local client displays numerical health.

## Ramming, damage, and death

- **[Explicit]** Ramming causes damage, and a boat's resistance depends on where
  it is struck.
- **[Implicit]** The bow is the strongest zone, the middle and sides have normal
  strength, and the stern is weakest.
- **[Implicit]** Current tuning gives boats 100 health, ignores collision impulses
  at or below 100 N·s, converts stronger impulse at 8 N·s per damage point, and
  applies strength multipliers of 2× at the bow, 1× amidships, and 0.6× at the
  stern.
- **[Explicit]** When a player's boat is destroyed, that player continues
  spectating the drifting wreck instead of exiting or respawning.
- **[Explicit]** Other players continue to see the destroyed boat as a wreck. The
  wreck follows boat physics and collisions but has no controller.

## Open product questions (not decisions)

- How long wrecks remain and how they are eventually cleaned up.
- Whether current health, impact thresholds, damage conversion, and zone strength
  values remain appropriate after broader playtesting.
- Concrete rules for progression, matchmaking, ship classes, automatic weapons,
  and the two custom actions.
- Whether the current draft, spawn clearance, and grounding-friction tuning
  remain appropriate after playtesting.
- Grounding recovery and rock-impact damage tuning.
- Concrete large-ship drafts and how often each class should find usable reef
  passages.
