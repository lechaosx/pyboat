# Curated user request ledger

This local, unversioned ledger consolidates the user's final intent from the
actual session records listed under **Sources**. It is a prioritized memory aid,
not a transcript or a canonical decision record. Canonical product and
architecture decisions remain in the tracked project documentation.

## Delivery state

### Completed and committed

- **Toroidal world foundation — `2fc5e8f`.** Rectangular world with square
  default, centered coordinates, selectable playground dimensions, visible
  center diagnostics, seamless procedural fields, and cross-boundary sailing,
  collision, combat, interest, and prediction. The selected hybrid is fixed by
  field class rather than a geometry-dependent runtime fallback: broad fields
  use circle-embedded 4D noise, fine fields use periodic 2D simplex, and pressure
  retains integer toroidal Fourier modes. This reconciles the wavelength and
  predictable-cost constraints. [S1 2026-08-23T12:52:47Z
  `01a02eae-43e3-76a0-8727-cd8e73fd2f09`; S1 2026-08-23T15:33:43Z
  `01a02f41-99b6-7bf1-b2d9-ae39bdf5827f`; S2 2026-08-24T15:08:51Z
  `01a03451-3208-7013-827e-b4bcc76eb8c3`]
- **Stateless cloud-transport foundation — `2fc5e8f`.** Neighboring temporal
  noise anchors, bounded wind transport, smootherstep timing, and orthogonal
  blending provide deterministic arbitrary-time cloud detail without accumulated
  advection degradation. Periodicity was explicitly folded into this unit. This
  completes transport infrastructure, not the later spatial forcing or thickness
  rendering work. [S1 2026-08-23T12:35:26Z
  `01a02e9e-5fcd-79b0-a6ea-3ec648cc425d`; S1 2026-08-23T14:22:44Z
  `01a02f00-9b2e-71b1-88d7-e84a670634b2`]
- **Position-aware environment sampler — `296c0ab`.** Canonical latitude, local
  solar phase, global season/moon phase, climate baseline, primary weather,
  surface climate, and tide became queryable by world position. This did not yet
  make the full rendered viewport spatial. [S2 2026-08-24T17:42:11Z
  `01a034dd-93ab-7842-a726-74f3a2abdabf`]
- **Spatial climate and polar rendering — `b491216`.** Surface climate,
  opposite seasons, local solar time, tide, snow, ice, and celestial lighting
  render per world position. Polar ice reuses island-forming noise and depth for
  an irregular stateless front, the moon responds to elevation, playground
  spatial diagnostics/guides remain visible, and circle-embedded cloud noise
  removes the origin cutoff. Visually accepted by the user. [S2
  2026-08-24T17:42:11Z `01a034dd-93ab-7842-a726-74f3a2abdabf`; S2
  2026-08-24T19:16:01Z `01a03533-7bd9-74f0-9d24-5587ec6c2a3a`; S2
  2026-08-24T19:36:51Z `01a03546-8e0c-7bb2-a4d6-ed0a79785663`; S2
  2026-08-25T17:40:18Z `01a03a02-34ca-7863-a7ec-75cee41572f0`]
- **Shared spatial cloud formation and thickness — `62819d7`.** Periodic
  position-aware weather forms signed low and upper cloud thickness on the
  approved finite-horizon transport field. Thickness drives relief, optical
  depth, opacity, and shadows. Cloud shadows use the user-selected spherical
  cloud-shell projection; the temporary comparison toggle and alternative were
  removed after visual selection. [S2 2026-08-25T18:15:19Z
  `01a03a22-4508-72d0-9aae-420d1ecba8ad`; S2 2026-08-25T18:26:41Z
  `01a03a2c-ac1b-70d2-b584-a3815f1e7bec`; S2 2026-08-25T18:40:21Z
  `01a03a39-313b-7090-958e-fd59e2c15788`; S2 2026-08-25T18:41:01Z
  `01a03a39-cbb5-7cc1-a196-85b870121136`]

Completed items stay here as a concise record and are not part of the active
backlog.

### Active production unit — emergent polar freezing and acute snow

Remove the authored polar freeze-front contour. Let physical latitude affect
ice through climate. The initially attempted freezeable-depth model was rejected
because real sea ice floats and flat basin depths still produced a contour-like
edge. Use seasonal sea-surface temperature plus a broad periodic maritime heat
field for the cap edge; bathymetry may affect coastal response indirectly but
must not decide whether deep water can freeze. Do not use distance from islands.
Very cold polar conditions should still close the ocean. Current temperature
should influence snow through a damped stateless ground response rather than
make snow mirror every weather fluctuation. At a given global season the winter
hemisphere must visibly gain broad land snow and sea ice while the opposite
hemisphere remains in its opposing season.
Temperate January land must sometimes carry snow while nearby water remains
unfrozen. The stateless approximation should reflect temperature,
precipitation, and day/night temperature change: seasonal ground temperature
provides persistence, current cold precipitation supplies fresh snow, and warm
daytime conditions promote melt. Day/night must change air and ground
temperature upstream, thereby changing saturation, instability, precipitation
intensity/type, and snow rather than only changing the final snow mask. Snow and
sea ice remain separate outcomes.

Status: reopened after user review; temperate winter snow and precipitation/
diurnal response are active acceptance gaps. The initial direct depth-cap
implementation was observed red against the
new seasonal/depth-independence contracts and was discarded. The current
reversible approximation uses seasonal water temperature plus a broad maritime
heat anomaly derived from the already-evaluated terrain cluster field (no extra
noise samples). Its influence tapers to zero from 75° to the pole, preserving a
fully closed pole while producing lobes and bays at the advancing edge. Snow's
ground temperature receives 25% of the current air anomaly. Rust/WGSL parity,
opposite-season whole-world snow/ice, exact-pole closure, seam periodicity,
workspace tests, WASM, builds, and scoped strict Clippy are green. Representative
review captures: `/tmp/pyboat-cryosphere-winter-final-strong.png` and
`/tmp/pyboat-maritime-ice-debug-strong.png`. These amplitude/coupling values are
implicit tuning until user endorsement.

Frozen-water presentation must be opaque rather than allowing submerged or
coastal material to show through. It should carry land-quality visual structure,
including readable floe-scale and fine ice variation plus a shallow-water ice
response, instead of a uniform fill. This is part of the cryosphere unit rather
than a later general sea-rendering pass. [current session 2026-08-25]

## Prioritized queued requirements

1. **Globally dynamic weather distribution.** Make weather vary more strongly
   across the finite world so a whole-world view normally contains active
   systems, plausibly including at least one storm at any time. Keep the amount
   tunable. Whether “one storm” is a strict conditioned guarantee or a very-high
   statistical likelihood remains open; do not make cloud rendering invent
   weather absent from the shared model. [S2 2026-08-25T18:17:32Z
   `01a03a24-4a6f-7a50-bb57-15ac04e79d9c`]
2. **Streamlined playground controls.** Rework the playground after the spatial
   model changes: remove settings that no longer affect production behavior,
   expose only meaningful live-world inputs and diagnostics, and make the UI
   clean and direct rather than preserving obsolete controls. Plan and deliver
   this as its own focused pass. [S2 2026-08-25T18:10:59Z
   `01a03a1e-4cad-78e1-8cf7-7b2bc3964ca0`]
3. **Continuous terrain covers.** Replace uncanny low-frequency categorical
   zones with emergent, continuously competing rock/sediment/vegetation/canopy/
   snow influences. Avoid classification-heavy machinery and distance from zone
   boundaries; preserve stylized material identity while allowing different
   boundary characteristics to emerge. [S1 2026-08-23T10:34:41Z
   `01a02e2f-d3cd-7572-90a5-8fad3d2151bb`; S1 2026-08-23T10:40:49Z
   `01a02e35-705e-7732-9d4b-3b5e1a614a38`]
4. **Atmospheric lighting and finite-source shadows.** Produce independently
   varying light intensity and color on land and visible clouds, graded rather
   than binary cloud/island shadows, gray-bright overcast conditions, and dark
   severe storms. Visible lower clouds receive both sun and moon contributions
   when the sources are present, rather than selecting only one primary light.
   The enduring physical constraint is: “sun is still a disc, not a point.” The
   user's leading idea is a complementary directional direct-light component
   plus nondirectional sky/cloud-scattered glow, but keep alternative models open
   if they achieve the requested behavior more naturally or cheaply.
   [S1
   2026-08-23T10:47:14Z `01a02e3b-4f5e-7871-b492-51552ecc47d2`; S1
   2026-08-23T10:48:54Z `01a02e3c-d7b7-78f1-bc68-5498c45c2b95`; S2
   2026-08-24T19:31:21Z `01a03541-83bc-73f0-b1dd-d8736956f183`; S2
   2026-08-24T19:32:42Z `01a03542-c20d-7570-9f97-d079da6d5274`; S2
   2026-08-24T19:33:56Z `01a03543-e0cd-70c2-8d88-cad622cd784d`]
5. **Precipitation presentation.** Make authoritative rain and snow visibly
   affect the water surface. Optional procedural screen-space rain/snow particles
   may be per-player rather than shared simulation state. [S2
   2026-08-24T19:26:53Z `01a0353d-6f85-7993-83c7-77cbb23aa8f3`]
6. **Deterministic gameplay ocean.** Add cheap shared currents and physical wave
   state influenced through plausible paths by weather, wind, islands,
   bathymetry, latitude, and tide. Rogue waves should emerge under severe
   weather/interference rather than from an arbitrary event flag. Explore
   wind-like mechanisms and alternatives before committing the architecture.
   [S1 2026-08-23T10:53:24Z `01a02e40-f4e9-7ca2-9fac-a8a1e7f80955`]
7. **Sea rendering.** Render condition-dependent open-sea surface, sun
   reflection/glitter, readable physical wave/current cues, coastal shoaling and
   breaking, rock/beach impacts, and foam. Shared gameplay-scale behavior must be
   deterministic; fine visual detail may be local. [S1 2026-08-23T10:58:50Z
   `01a02e45-ee2e-7971-bcdd-dc96bb516fc8`]

## Open decisions

- **North-south topology.** The committed world is a torus whose identified
  `y` edge is one long polar loop with a deliberate signed-latitude/season jump.
  The user is now considering non-periodic `y` because the day/night and season
  jump is jarring, but has not selected boundary behavior. Do not alter topology
  until the border/gameplay consequences are compared and the user decides.
  [S1 2026-08-23T11:05:06Z `01a02e4b-ac51-7c81-805f-b2a5cfbf9d75`; S2
  2026-08-24T18:23:31Z `01a03503-6c45-7260-aec3-3a23c1803864`]
- **Ocean model scope/calibration.** The first physical gameplay effects, rogue
  hazard calibration, and how much coast-wave refraction/diffraction belongs in
  the first deployable unit remain intentionally deferred.

## Maintenance and workflow

- Record each incoming user request here before acting. Consolidate repetition
  into final intent, retain source traceability, and reconcile superseded
  statements chronologically rather than accumulating contradictions.
- Prioritize instead of jumping between requests. Work on one sensible,
  deployable production unit at a time; “Unit should be deployable.” [S1
  2026-08-23T15:33:43Z `01a02f41-99b6-7bf1-b2d9-ae39bdf5827f`; S2
  2026-08-24T18:29:15Z `01a03508-aaac-79c3-ad49-c19b6da46869`]
- Keep the user in the loop at planning and review gates. Do not commit a unit
  until the user ACKs it. The current autonomous-away instruction temporarily
  supersedes that gate: finish each deployable unit, update canonical docs,
  commit, and push before advancing, then report the reversible decisions when
  the user returns. [S2 2026-08-24T18:29:15Z
  `01a03508-aaac-79c3-ad49-c19b6da46869`; S1 2026-08-23T11:16:59Z
  `01a02e56-8d07-7941-9db6-648b9cbd46cc`; current session 2026-08-25]
- Defer decisions while they do not block safe progress. Surface genuine
  blockers; make cheap, reversible choices when needed, document them, and flag
  them to the user. When the user is away, continue autonomously through safe
  verification and stop only at a real decision or review/commit gate. [S2 2026-08-24T16:41:33Z
  `01a034a6-116c-7a31-aa29-39cd94e883a9`]
- When a request is committed/done, remove it from active or queued work and keep
  only a concise entry under completed work.
- Keep this ledger and other WIP notes unversioned. Tracked code changes must
  update canonical Markdown according to repository rules. [S1
  2026-08-23T11:15:06Z `01a02e54-d42f-7021-8dd6-502e836f09e3`; S1
  2026-08-23T11:16:59Z `01a02e56-8d07-7941-9db6-648b9cbd46cc`]
- Use subagents only when useful under normal agent judgment. This supersedes the
  initial prescriptive delegation rules; the later explicit request to spawn an
  archival subagent applied only to this restoration task. [S2
  2026-08-24T17:59:06Z `01a034ed-0e7b-70e0-8c7d-1f2099d5fb2a`; S2
  2026-08-24T18:31:48Z `01a0350b-0117-7f43-aa58-a92a18c1d0e9`]
- When the user changes workflow instructions, update this section in the same
  turn: replace superseded rules with the latest chronological intent and keep
  the supporting source citation.

## Sources

- **S1 — original thread:** `/home/dlabaja/.codex/sessions/2026/08/23/rollout-2026-08-23T12-26-56-01a02e28-bb25-7c03-93b3-7b99903aba39.jsonl`
  (session `01a02e28-bb25-7c03-93b3-7b99903aba39`).
- **S2 — continued root thread:** `/home/dlabaja/.codex/sessions/2026/08/24/rollout-2026-08-24T16-38-07-01a03435-0fe8-7fd1-89c0-8dea3ed81148.jsonl`
  (session `01a03435-0fe8-7fd1-89c0-8dea3ed81148`, forked from S1 at ordinal
  1794).
- `/home/dlabaja/.codex/history.jsonl` corroborates user text, session IDs, and
  epoch timestamps; the session JSONL files supply stable message IDs and local
  image paths.

All request wording and chronology used here came from these local records. No
request was reconstructed from inherited or compacted conversational context.
