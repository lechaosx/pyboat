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
- **[Implicit]** The first implemented procedural-environment slice gives each
  Rust match seeded wind that changes smoothly across the world and over time.
  Water current remains still until a later environment slice.
- **[Explicit]** The browser renders water depth continuously, with a distinct
  transition at the shoreline.
- **[Explicit]** The browser HUD reports water depth, under-keel clearance, and
  how strongly the hull is dragging on the bottom.
- **[Implicit]** The browser renders its static seeded elevation field with a
  continuous shallow-to-deep water gradient and a discrete shoreline transition.
- **[Explicit]** Land uses distinct beach, grass, forest, rock, and high-altitude
  snow materials. Material boundaries remain sharp except for the blend between
  grass and trees; color variation stays within each material.
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
- **[Explicit]** Cloud shapes move at the speed and direction of coherent low-
  and high-altitude winds while also forming and dissipating over time, without
  simulating a global fluid volume. Boat physics uses the related sea-level wind;
  short-scale gusts do not abruptly steer cloud motion.
- **[Explicit]** Each cloud family combines its own isotropic, non-rotating detail
  field with a lower-frequency formation mask aligned to the layer wind. The mask
  may rotate without visibly rotating individual billows or cloud texture.
- **[Explicit]** Cloud advection can be continuously split between detail and
  formation mask. Moving only the mask makes cloud form at its leading edge and
  dissipate at its trailing edge; moving only detail preserves a stationary
  formation envelope.
- **[Explicit]** Familiar cloud families emerge from continuous atmospheric
  conditions rather than named presets: stable moisture favors layered clouds,
  instability favors cellular and towering clouds, rain darkens storm systems,
  and sparse high-level cloud potential produces wind-stretched wisps.
- **[Explicit]** Cloud cover is predominantly low-altitude and detailed at the
  default view. Broad, flat grey overcast is avoided so navigation remains clear;
  storm clouds are rare, while ordinary moving clouds remain useful wind cues.
- **[Explicit]** Ordinary clouds are sparse and mostly translucent, with fine
  internal density and depth variation. Long decks are uncommon; dense dark
  opacity is concentrated in rare storm conditions so the boat usually remains
  readable.
- **[Explicit]** Clouds cast translucent shadows on the world. Formation height
  controls their projected separation, so higher clouds cast shadows farther
  from their visible bodies than low clouds.
- **[Explicit]** Each cloud layer casts its own corresponding shadow. Visual
  separation remains short enough to associate a cloud and its shadow in the
  default view.
- **[Explicit]** Clouds do not conceal players or otherwise affect gameplay.
  Authored sprites remain appropriate for details such as rocks, trees, and
  wreckage.

## Platforms and controls

- **[Explicit]** The procedural playground supports drag panning and cursor-
  anchored mouse-wheel zoom. Its time advances continuously, and an adjustable
  rate scales both weather evolution and cloud motion.
- **[Explicit]** The playground uses the same architectural 1–2–5 metric scale
  indicator as gameplay. Absolute camera position and time are not linear
  sliders; navigation uses drag and wheel, while current time is a live readout.
- **[Implicit]** View height is likewise controlled only through wheel zoom rather
  than an impractically wide linear slider.
- **[Explicit]** Weather presets interpolate smoothly. Clicking a slider track
  eases to the selected value, while dragging a slider remains immediate.
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
