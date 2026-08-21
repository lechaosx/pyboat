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

- **[Explicit]** A statistically connected, dominant sea contains natural-looking
  sparse groups of large procedural islands. Land may be more fractal and rare
  enclosed water may occur, but the world must not become a patchwork of equal
  land and water or commonly isolate players from the open sea.
- **[Explicit]** Large islands retain coherent silhouettes while smaller relief
  forms coasts, reefs, navigable channels, and occasional rocks without making
  the parent landforms look noisy or assembled from separate objects.
- **[Explicit]** Coast character varies from beach-dominant to mixed and
  beachless. Gentle shores can form connected beaches, cliffs continue below the
  waterline, and geology-dependent shallow shelves give way to deep ocean basins
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
- **[Explicit]** Environment models are locally plausible and stateless rather
  than global fluid, climate, storm-lifecycle, or ocean-basin simulations.
  Storm-like conditions emerge from continuous atmospheric fields.
- **[Explicit]** Unsheltered wind can blow in any direction, varies smoothly from
  gust to broad weather system, and follows a Weibull speed distribution with
  `k = 2` and `λ = 9 m/s` at a fixed place and time. Land provides simplified,
  deterministic lee shelter over nearby water.
- **[Explicit]** Sun and moon state produces a stateless astronomical tide.
  Spring/neap range follows moon phase, while local response may sacrifice global
  basin accuracy. Tidal height changes authoritative depth, grounding,
  navigability, and shoreline position.
- **[Explicit]** Seasonal ground and water temperature, snow, vegetation, and
  solid ice derive analytically without stored per-location history.
- **[Explicit]** Solid ice is land for boats. It blends into exposed land but
  ends at water with a sharp, readable boundary. Persistent polar cold expands
  it across the sea and can close the sea entirely.

## Presentation

- **[Explicit]** Procedural graphics communicate physical scale and environment.
  This includes wave lines around islands and moving ships. Water transitions
  continuously with depth and distinctly at the moving shoreline; land has
  recognizable beach, vegetation, rock, snow, and ice materials. The same solid
  ice surface covers land and polar water rather than using a decorative
  floating-ice layer.
- **[Explicit]** Terrain shading reads as relief. Rock exposure responds to slope
  and altitude, beaches span the intertidal zone where geology permits them, and
  ordinary vegetation begins above the permanent tidal range so low tide exposes
  plausible wet rock or sediment. Temperate vegetation may change seasonally
  without making tropical vegetation do the same. Material boundaries remain
  sharp except for the grass-to-forest blend; color variation stays within each
  material. Gameplay-scale relief comes from the authoritative heightfield;
  rough physical surfaces resolve to a one-metre wavelength, while smaller
  material detail may refine lighting without changing collision.
  Heightfield relief lighting and bounded cast shadows add depth without
  displacing shorelines or changing collision. Visual material structure is
  calibrated for boat-scale zoom and does not expose the procedural sampling
  lattice.
- **[Explicit]** Shared weather is deterministic, but fine cloud geometry is a
  local visual. The viewport uses weather at the local boat, and clouds never
  conceal players or affect gameplay.
- **[Explicit]** A visible low layer supplies decorative wisps, detail, and wind
  cues across at most about one quarter of the view. It may cast localized
  shadows but contributes little to global overcast lighting. An invisible upper
  cloud field represents meteorological coverage through broad shadows and
  lighting. Both respond to celestial light, while aerosol, moisture, rain, and
  cloud cover shape scene color and mood.
- **[Explicit]** Cloud bodies read as volumetric layers rather than terrain-like
  height fields.
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
- **[Explicit]** Derived conditions are read-only. Scenarios change their primary
  causes, while independent rendered contributions have hierarchical enable
  controls that preserve subordinate values.
- **[Explicit]** Production tuning is available as a preset, but live world state
  has no arbitrary default reset. Initial world conditions are illustrative and
  the initial view shows final composition.
- **[Explicit]** The lab supports drag panning, cursor-anchored zoom, typed values,
  immediate slider dragging, eased track clicks, and independent or locked
  real-time multipliers for cloud evolution and world time.
- **[Explicit]** The lab presents local solar time in hours, season as an
  approximate real-numbered month, and lunar phase with named landmarks while
  retaining normalized runtime phases internally.
- **[Explicit]** Underwater bottom visibility follows two-way exponential light
  attenuation. The lab exposes attenuation length and bottom-light contribution
  without changing physical terrain, depth, or grounding.
- **[Explicit]** Landform lighting follows the astronomical sun or moon angle,
  including flat relief under directly overhead light. The lab exposes relief
  strength, cast-shadow strength, and shadow reach while production retains a
  moderate heightfield-lighting preset.
- **[Explicit]** The lab orders physical inputs as location and time, climate,
  current weather, and local tide. It keeps those inputs open by default and
  separates visibility switches and advanced tuning into collapsed groups. Each
  group carries one world, model, shader, or debug classification rather than
  repeating classifications on individual controls.
- **[Explicit]** Named weather and seasonal scenarios represent characteristic
  regimes, not generic intensity steps. A deterministic headless mode renders the
  same scenarios and emits images with readable condition summaries.

## Open questions

| Area | Unresolved choice |
| --- | --- |
| Wrecks | Cleanup timing and whether salvage affects it |
| Progression | Concrete progression, matchmaking, ship-class, weapon, and custom-action rules |
| Grounding | Recovery, rock-impact damage, draft, spawn-clearance, and friction tuning |
| Navigation | Large-ship drafts and the desired availability of reef passages by class |
| Combat | Health, impact thresholds, damage conversion, and hull-zone balance after playtesting |
